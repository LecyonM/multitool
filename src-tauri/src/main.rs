// Empêche l'ouverture d'une console supplémentaire sous Windows en version finale.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use multitool_core::{Input, Output, ProgressUpdate, Registry, UtilityDescriptor};
use tauri::ipc::Channel;
use tauri::State;

struct AppState {
    registry: Arc<Registry>,
}

/// Liste des utilitaires (infos + champs) : l'interface s'en sert pour se construire.
#[tauri::command]
fn list_utilities(state: State<'_, AppState>) -> Vec<UtilityDescriptor> {
    state.registry.descriptors()
}

/// Exécute un utilitaire hors du thread principal pour ne jamais figer la fenêtre.
/// L'avancement est envoyé à l'interface au fil de l'eau par `on_progress`.
#[tauri::command]
async fn run_utility(
    state: State<'_, AppState>,
    id: String,
    input: Input,
    on_progress: Channel<ProgressUpdate>,
) -> Result<Output, String> {
    let registry = Arc::clone(&state.registry);
    tauri::async_runtime::spawn_blocking(move || {
        registry.run_with_progress(&id, &input, &|update| {
            // Si la fenêtre ne reçoit plus rien (fermée), on continue quand même.
            let _ = on_progress.send(update);
        })
    })
    .await
    .map_err(|err| format!("Tâche interrompue : {err}"))?
    .map_err(|err| err.to_string())
}

/// Nombre de caractères du résultat que le PDF direct ne saurait pas dessiner.
#[tauri::command]
fn pdf_coverage(title: String, output: Output) -> usize {
    multitool_export::count_unsupported(&title, &output)
}

/// Écrit le résultat dans un fichier PDF, à l'endroit choisi par l'utilisateur.
/// Renvoie le chemin réellement écrit (l'extension `.pdf` est ajoutée si elle manque).
#[tauri::command]
async fn export_pdf(
    title: String,
    generated_at: String,
    output: Output,
    path: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut target = std::path::PathBuf::from(path);
        if !target
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            target.as_mut_os_string().push(".pdf");
        }
        let (bytes, _) = multitool_export::render_pdf(&title, &generated_at, &output)?;
        std::fs::write(&target, bytes).map_err(|err| {
            format!(
                "Impossible d'écrire le fichier « {} » : {err}",
                target.display()
            )
        })?;
        Ok(target.to_string_lossy().into_owned())
    })
    .await
    .map_err(|err| format!("Tâche interrompue : {err}"))?
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            registry: Arc::new(multitool_utilities::registry()),
        })
        .invoke_handler(tauri::generate_handler![
            list_utilities,
            run_utility,
            pdf_coverage,
            export_pdf
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Multitool");
}
