//! Découvre automatiquement les utilitaires présents dans `src/modules/`.
//!
//! Chaque fichier `src/modules/<nom>.rs` (ou dossier `src/modules/<nom>/mod.rs`)
//! devient un module et son `register(&mut Registry)` est appelé au démarrage.
//! Les fichiers dont le nom commence par `_` sont ignorés.
//! Ajouter un utilitaire ne demande donc aucune autre modification.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let modules_dir = manifest_dir.join("src").join("modules");
    println!("cargo:rerun-if-changed={}", modules_dir.display());
    println!("cargo:rerun-if-changed=build.rs");

    let mut modules: Vec<(String, PathBuf)> = Vec::new();
    for entry in fs::read_dir(&modules_dir).expect("dossier src/modules introuvable") {
        let path = entry.expect("lecture de src/modules impossible").path();
        let (name, source) = if path.is_dir() {
            let source = path.join("mod.rs");
            if !source.is_file() {
                continue;
            }
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                source,
            )
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            (
                path.file_stem().unwrap().to_string_lossy().into_owned(),
                path.clone(),
            )
        } else {
            continue;
        };

        if name.starts_with('_') {
            continue;
        }
        let valid = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !name.chars().next().unwrap().is_ascii_digit();
        assert!(
            valid,
            "Nom de module invalide : « {name} » (lettres, chiffres et _ uniquement, sans chiffre au début)"
        );
        modules.push((name, source));
    }
    modules.sort();

    let mut code = String::from("// Fichier généré par build.rs : ne pas modifier.\n\n");
    for (name, source) in &modules {
        let path = source.to_string_lossy().replace('\\', "/");
        code.push_str(&format!("#[path = {path:?}]\npub mod {name};\n\n"));
    }
    code.push_str("pub fn register_all(registry: &mut multitool_core::Registry) {\n");
    for (name, _) in &modules {
        code.push_str(&format!("    {name}::register(registry);\n"));
    }
    code.push_str("}\n");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("modules_generated.rs");
    fs::write(out, code).expect("écriture du fichier généré impossible");
}
