//! Téléchargement d'un lien qui pointe directement vers un fichier vidéo ou audio.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use multitool_core::{ProgressUpdate, UtilityError};
use ureq::ResponseExt;

use super::{files, format};

/// Fichier enregistré avec succès.
pub struct Saved {
    pub path: PathBuf,
    pub bytes: u64,
}

pub enum Outcome {
    Saved(Saved),
    /// Le lien ne mène pas à un fichier (page web, liste de lecture, erreur HTTP) :
    /// seul un extracteur comme yt-dlp peut peut-être en tirer une vidéo.
    /// Le texte explique pourquoi, en français.
    NotAFile(String),
}

/// Délai minimal entre deux mises à jour envoyées à l'interface.
const REPORT_EVERY: Duration = Duration::from_millis(120);

pub fn download(
    url: &str,
    dir: &Path,
    progress: &dyn Fn(ProgressUpdate),
) -> Result<Outcome, UtilityError> {
    let config = ureq::Agent::config_builder()
        // Les codes 4xx/5xx sont traités ici : ils orientent vers yt-dlp.
        .http_status_as_error(false)
        .user_agent("Multitool/0.1")
        .timeout_connect(Some(Duration::from_secs(15)))
        // Coupure d'une connexion qui ne répond plus ; ne limite pas la durée totale.
        .timeout_per_read(Some(Duration::from_secs(30)))
        .build();
    let agent = ureq::Agent::new_with_config(config);

    progress(ProgressUpdate::indeterminate("Connexion au serveur…"));
    let mut response = agent.get(url).call().map_err(network_error)?;

    let status = response.status().as_u16();
    // 404 et 410 : la page n'existe pas, aucun extracteur n'y trouvera de vidéo.
    if status == 404 || status == 410 {
        return Err(UtilityError::Failed(format!(
            "Lien introuvable : le serveur a répondu avec l'erreur {status}. Vérifiez l'adresse."
        )));
    }
    // Autres erreurs (403, 429, 5xx…) : certains sites refusent les programmes
    // mais restent accessibles à yt-dlp, qui prend alors le relais.
    if status >= 400 {
        return Ok(Outcome::NotAFile(format!(
            "Le serveur a répondu avec l'erreur {status}."
        )));
    }

    let mime = response
        .body()
        .mime_type()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if needs_extractor(&mime) {
        return Ok(Outcome::NotAFile(
            "Ce lien mène à une page web et non à un fichier vidéo.".into(),
        ));
    }

    let total = response.body().content_length().filter(|&len| len > 0);
    let disposition = response
        .headers()
        .get("content-disposition")
        .and_then(|value| value.to_str().ok())
        .and_then(files::name_from_content_disposition);
    // Adresse finale : un lien court qui redirige vers « …/video.mp4 » doit donner ce nom.
    let final_url = response.get_uri().to_string();
    let name = choose_name(disposition, files::name_from_url(&final_url), &mime);
    let target = files::unique_path(dir, &name);
    let part = files::part_path(&target);

    let result = copy_with_progress(response.body_mut().as_reader(), &part, total, progress);
    match result {
        Ok(bytes) => {
            fs::rename(&part, &target).map_err(|err| {
                let _ = fs::remove_file(&part);
                UtilityError::Failed(format!("Impossible de finaliser le fichier : {err}."))
            })?;
            Ok(Outcome::Saved(Saved {
                path: target,
                bytes,
            }))
        }
        Err(err) => {
            // Un fichier à moitié téléchargé ne doit jamais ressembler à une vidéo valide.
            let _ = fs::remove_file(&part);
            Err(err)
        }
    }
}

/// Type de contenu qui désigne une page ou un manifeste plutôt qu'un fichier média.
fn needs_extractor(mime: &str) -> bool {
    mime.starts_with("text/")
        || ["html", "xml", "json", "mpegurl"]
            .iter()
            .any(|marker| mime.contains(marker))
}

/// Nom du fichier : en-tête du serveur, sinon nom présent dans le lien, sinon « video ».
/// Une extension déduite du type MIME complète un nom qui n'en a pas.
fn choose_name(disposition: Option<String>, from_url: Option<String>, mime: &str) -> String {
    let raw = disposition.or(from_url).unwrap_or_else(|| "video".into());
    let mut name = files::sanitize(&raw);
    if files::split_extension(&name).1.is_empty() {
        if let Some(extension) = files::extension_for_mime(mime) {
            name = format!("{name}.{extension}");
        }
    }
    name
}

fn copy_with_progress(
    mut reader: impl Read,
    part: &Path,
    total: Option<u64>,
    progress: &dyn Fn(ProgressUpdate),
) -> Result<u64, UtilityError> {
    let mut file = File::create(part).map_err(|err| {
        UtilityError::Failed(format!(
            "Impossible de créer le fichier dans ce dossier : {err}."
        ))
    })?;

    let started = Instant::now();
    let mut last_report = started;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut done: u64 = 0;

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| UtilityError::Failed(format!("Téléchargement interrompu : {err}.")))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|err| UtilityError::Failed(format!("Écriture impossible : {err}.")))?;
        done += read as u64;

        if last_report.elapsed() >= REPORT_EVERY {
            last_report = Instant::now();
            progress(update(done, total, started.elapsed()));
        }
    }
    file.flush()
        .map_err(|err| UtilityError::Failed(format!("Écriture impossible : {err}.")))?;

    if let Some(expected) = total {
        if done < expected {
            return Err(UtilityError::Failed(format!(
                "Téléchargement incomplet : {} reçus sur {}.",
                format::bytes(done),
                format::bytes(expected)
            )));
        }
    }
    progress(update(done, total.or(Some(done)), started.elapsed()));
    Ok(done)
}

/// Barre et message pour `done` octets reçus (sur `total` si connu).
fn update(done: u64, total: Option<u64>, elapsed: Duration) -> ProgressUpdate {
    let seconds = elapsed.as_secs_f64();
    let speed = if seconds > 0.2 {
        Some(done as f64 / seconds)
    } else {
        None
    };

    match total {
        Some(total) => {
            let mut message = format!("{} sur {}", format::bytes(done), format::bytes(total));
            if let Some(speed) = speed.filter(|s| *s > 0.0) {
                message.push_str(&format!(", {}", format::speed(speed)));
                if done < total {
                    let remaining = ((total - done) as f64 / speed).ceil() as u64;
                    message.push_str(&format!(", {} restantes", format::duration(remaining)));
                }
            }
            ProgressUpdate::new(done as f64 / total as f64, message)
        }
        None => {
            let mut message = format!("{} reçus", format::bytes(done));
            if let Some(speed) = speed {
                message.push_str(&format!(", {}", format::speed(speed)));
            }
            ProgressUpdate::indeterminate(message)
        }
    }
}

fn network_error(err: ureq::Error) -> UtilityError {
    use std::io::ErrorKind;

    let message = match err {
        ureq::Error::Io(ref io) => match io.kind() {
            ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionAborted
            | ErrorKind::NotConnected
            | ErrorKind::AddrNotAvailable => {
                "Connexion impossible : le serveur est injoignable.".to_string()
            }
            ErrorKind::TimedOut | ErrorKind::WouldBlock => {
                "Le serveur ne répond pas (délai dépassé).".to_string()
            }
            ErrorKind::ConnectionReset | ErrorKind::UnexpectedEof | ErrorKind::BrokenPipe => {
                "La connexion a été coupée par le serveur.".to_string()
            }
            _ => format!("Erreur réseau : {io}."),
        },
        ureq::Error::HostNotFound => {
            "Adresse introuvable : vérifiez le lien et votre connexion internet.".to_string()
        }
        ureq::Error::ConnectionFailed => {
            "Connexion impossible : le serveur est injoignable.".to_string()
        }
        ureq::Error::Timeout(_) => "Le serveur ne répond pas (délai dépassé).".to_string(),
        ureq::Error::BadUri(_) => "Ce lien n'est pas une adresse valide.".to_string(),
        ureq::Error::TooManyRedirects | ureq::Error::RedirectFailed => {
            "Le lien redirige trop de fois ou vers une adresse invalide.".to_string()
        }
        other => format!("Erreur réseau : {other}."),
    };
    UtilityError::Failed(message)
}
