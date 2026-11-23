//! Téléchargement depuis une page de site vidéo, en pilotant le programme externe yt-dlp.
//!
//! yt-dlp n'est pas embarqué : il est cherché (dans l'ordre) via la variable
//! `MULTITOOL_YTDLP`, à côté de l'exécutable de Multitool, puis dans le PATH.

use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use multitool_core::{ProgressUpdate, UtilityError};

use super::direct::Saved;
use super::format;

const REPORT_EVERY: Duration = Duration::from_millis(120);

/// Préfixes des lignes que nous demandons à yt-dlp d'écrire (voir `build_args`).
const PROGRESS_TAG: &str = "MTPROG";
const POST_TAG: &str = "MTPOST";
const FILE_TAG: &str = "MTFILE";

pub const NOT_INSTALLED: &str = "yt-dlp est introuvable. Installez-le (Windows : « winget install yt-dlp » ; macOS : « brew install yt-dlp » ; Linux : « pip install yt-dlp »), ou placez yt-dlp à côté de l'application, puis relancez.";

/// Qualité maximale demandée (en hauteur d'image) ; `best` = pas de limite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Best,
    Max(u32),
}

impl Quality {
    pub fn parse(value: &str) -> Self {
        match value.trim().parse::<u32>() {
            Ok(height) if height > 0 => Self::Max(height),
            _ => Self::Best,
        }
    }
}

/// Programme externe lancé sans ouvrir de fenêtre de console sous Windows.
fn command(program: &OsStr) -> Command {
    // `mut` ne sert que sous Windows (voir ci-dessous).
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command
}

fn binary_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

fn runs(program: &Path, arg: &str) -> bool {
    command(program.as_os_str())
        .arg(arg)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Chemin de yt-dlp s'il est utilisable.
pub fn find() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(custom) = std::env::var_os("MULTITOOL_YTDLP").filter(|v| !v.is_empty()) {
        candidates.push(PathBuf::from(custom));
    }
    if let Some(dir) = exe_dir() {
        candidates.push(dir.join(binary_name("yt-dlp")));
    }
    candidates.push(PathBuf::from(binary_name("yt-dlp"))); // résolu par le PATH
    candidates
        .into_iter()
        .find(|candidate| runs(candidate, "--version"))
}

/// ffmpeg est nécessaire pour assembler la vidéo et l'audio, que les sites
/// fournissent souvent séparément. `Some(Some(dossier))` : trouvé à côté de
/// l'application (à indiquer à yt-dlp) ; `Some(None)` : dans le PATH ; `None` : absent.
fn find_ffmpeg() -> Option<Option<PathBuf>> {
    if let Some(dir) = exe_dir() {
        if runs(&dir.join(binary_name("ffmpeg")), "-version") {
            return Some(Some(dir));
        }
    }
    runs(Path::new(&binary_name("ffmpeg")), "-version").then_some(None)
}

/// Sélecteur de format yt-dlp. Sans ffmpeg, on ne demande que des fichiers
/// déjà complets (image et son ensemble), quitte à perdre en qualité.
fn format_selector(quality: Quality, has_ffmpeg: bool) -> String {
    match (quality, has_ffmpeg) {
        (Quality::Best, true) => "bv*+ba/b".into(),
        (Quality::Best, false) => "b".into(),
        (Quality::Max(h), true) => format!("bv*[height<={h}]+ba/b[height<={h}]/b"),
        (Quality::Max(h), false) => format!("b[height<={h}]/b"),
    }
}

fn build_args(
    url: &str,
    dir: &Path,
    selector: &str,
    ffmpeg_dir: Option<&Path>,
) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = Vec::new();
    let mut push = |value: &str| args.push(value.into());

    push("--no-playlist");
    push("--no-mtime");
    push("--no-colors");
    push("--newline");
    push("--progress");
    push("--socket-timeout");
    push("30");
    push("-f");
    push(selector);
    push("-o");
    push("%(title).150B [%(id)s].%(ext)s");
    push("--progress-template");
    push(&format!(
        "download:{PROGRESS_TAG}|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(info.vcodec)s|%(info.acodec)s"
    ));
    push("--progress-template");
    push(&format!(
        "postprocess:{POST_TAG}|%(progress.postprocessor)s|%(progress.status)s"
    ));
    push("--print");
    push(&format!("after_move:{FILE_TAG}|%(filepath)s"));

    args.push("-P".into());
    args.push(dir.as_os_str().to_os_string());
    if let Some(ffmpeg_dir) = ffmpeg_dir {
        args.push("--ffmpeg-location".into());
        args.push(ffmpeg_dir.as_os_str().to_os_string());
    }
    // « -- » : ce qui suit est toujours une adresse, jamais une option.
    args.push("--".into());
    args.push(url.into());
    args
}

/// Ce que contient un flux téléchargé (yt-dlp récupère image et son à part).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Track {
    Video,
    Audio,
    Both,
}

#[derive(Debug, PartialEq)]
enum Line {
    Progress {
        downloaded: u64,
        total: Option<u64>,
        speed: Option<f64>,
        eta: Option<u64>,
        track: Track,
    },
    PostProcess(String),
    File(String),
}

fn number<T: std::str::FromStr>(text: &str) -> Option<T> {
    match text.trim() {
        "" | "NA" | "None" => None,
        other => other.parse().ok(),
    }
}

/// yt-dlp écrit parfois des nombres à virgule flottante (`1234.0`) pour des octets.
fn whole(text: &str) -> Option<u64> {
    number::<u64>(text).or_else(|| number::<f64>(text).map(|value| value.max(0.0) as u64))
}

fn parse_line(line: &str) -> Option<Line> {
    let line = line.trim();
    let mut parts = line.split('|');
    match parts.next()? {
        PROGRESS_TAG => {
            let fields: Vec<&str> = parts.collect();
            if fields.len() < 7 {
                return None;
            }
            let video = !matches!(fields[5].trim(), "none" | "NA" | "");
            let audio = !matches!(fields[6].trim(), "none" | "NA" | "");
            let track = match (video, audio) {
                (true, false) => Track::Video,
                (false, true) => Track::Audio,
                _ => Track::Both,
            };
            Some(Line::Progress {
                downloaded: whole(fields[0])?,
                total: whole(fields[1])
                    .filter(|&t| t > 0)
                    .or_else(|| whole(fields[2]).filter(|&t| t > 0)),
                speed: number::<f64>(fields[3]),
                eta: whole(fields[4]),
                track,
            })
        }
        POST_TAG => {
            let fields: Vec<&str> = parts.collect();
            (fields.get(1).map(|s| s.trim()) == Some("started"))
                .then(|| Line::PostProcess(fields[0].trim().to_string()))
        }
        FILE_TAG => {
            // Le chemin peut contenir « | » : on reprend tout ce qui suit la balise.
            let path = line[FILE_TAG.len() + 1..].trim();
            (!path.is_empty()).then(|| Line::File(path.to_string()))
        }
        _ => None,
    }
}

fn progress_for(
    downloaded: u64,
    total: Option<u64>,
    speed: Option<f64>,
    eta: Option<u64>,
    track: Track,
) -> ProgressUpdate {
    let prefix = match track {
        Track::Video => "Vidéo : ",
        Track::Audio => "Audio : ",
        Track::Both => "",
    };
    let mut message = match total {
        Some(total) => format!(
            "{prefix}{} sur {}",
            format::bytes(downloaded),
            format::bytes(total)
        ),
        None => format!("{prefix}{} reçus", format::bytes(downloaded)),
    };
    if let Some(speed) = speed.filter(|s| *s > 0.0) {
        message.push_str(&format!(", {}", format::speed(speed)));
    }
    // yt-dlp renvoie 0 à la toute fin du transfert : inutile à afficher.
    if let Some(eta) = eta.filter(|&seconds| seconds > 0) {
        message.push_str(&format!(", {} restantes", format::duration(eta)));
    }
    match total {
        Some(total) => ProgressUpdate::new(downloaded as f64 / total as f64, message),
        None => ProgressUpdate::indeterminate(message),
    }
}

fn postprocess_message(processor: &str) -> &'static str {
    match processor {
        "Merger" => "Assemblage de la vidéo et de l'audio…",
        "ExtractAudio" => "Extraction de l'audio…",
        _ => "Finalisation du fichier…",
    }
}

/// Convertit le message d'erreur de yt-dlp (en anglais) en conseil clair.
fn explain_error(stderr: &str, code: Option<i32>) -> String {
    let message = stderr
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("ERROR:"))
        .map(|text| text.trim().to_string());

    match message {
        Some(text) if text.contains("Unsupported URL") => {
            "Ce lien n'est ni un fichier vidéo ni une page prise en charge par yt-dlp.".into()
        }
        Some(text) if text.to_lowercase().contains("ffmpeg") => format!(
            "yt-dlp a besoin de ffmpeg pour cette vidéo. Installez ffmpeg (Windows : « winget install ffmpeg »), puis réessayez. Détail : {text}"
        ),
        Some(text) => format!("yt-dlp n'a pas pu télécharger cette vidéo : {text}"),
        None => match code {
            Some(code) => format!("yt-dlp s'est arrêté avec le code {code}."),
            None => "yt-dlp a été interrompu.".into(),
        },
    }
}

pub fn download(
    ytdlp: &Path,
    url: &str,
    dir: &Path,
    quality: Quality,
    progress: &dyn Fn(ProgressUpdate),
) -> Result<Saved, UtilityError> {
    progress(ProgressUpdate::indeterminate("Analyse du lien avec yt-dlp…"));

    let ffmpeg = find_ffmpeg();
    let selector = format_selector(quality, ffmpeg.is_some());
    let ffmpeg_dir = ffmpeg.as_ref().and_then(|location| location.as_deref());
    let args = build_args(url, dir, &selector, ffmpeg_dir);

    let mut child = command(ytdlp.as_os_str())
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| UtilityError::Failed(format!("Impossible de lancer yt-dlp : {err}.")))?;

    // stderr est lu à part : s'il se remplissait sans être vidé, yt-dlp se bloquerait.
    let mut stderr_pipe = child.stderr.take().expect("stderr redirigé");
    let stderr_reader = thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr_pipe.read_to_string(&mut text);
        text
    });

    let stdout = child.stdout.take().expect("stdout redirigé");
    let mut final_path: Option<String> = None;
    let mut last_report = Instant::now() - REPORT_EVERY;

    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        match parse_line(&line) {
            Some(Line::Progress {
                downloaded,
                total,
                speed,
                eta,
                track,
            }) => {
                if last_report.elapsed() >= REPORT_EVERY {
                    last_report = Instant::now();
                    progress(progress_for(downloaded, total, speed, eta, track));
                }
            }
            Some(Line::PostProcess(processor)) => {
                progress(ProgressUpdate::indeterminate(postprocess_message(
                    &processor,
                )));
            }
            Some(Line::File(path)) => final_path = Some(path),
            None => {}
        }
    }

    let status = child
        .wait()
        .map_err(|err| UtilityError::Failed(format!("yt-dlp s'est arrêté brutalement : {err}.")))?;
    let stderr = stderr_reader.join().unwrap_or_default();

    if !status.success() {
        return Err(UtilityError::Failed(explain_error(&stderr, status.code())));
    }

    let path = final_path
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .ok_or_else(|| UtilityError::Failed("yt-dlp n'a produit aucun fichier.".into()))?;
    let bytes = path.metadata().map(|meta| meta.len()).unwrap_or(0);
    progress(ProgressUpdate::new(1.0, "Terminé"));
    Ok(Saved { path, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_progress_line() {
        let line = "MTPROG|1048576|4194304|NA|524288.5|6|avc1.64001F|none";
        assert_eq!(
            parse_line(line),
            Some(Line::Progress {
                downloaded: 1_048_576,
                total: Some(4_194_304),
                speed: Some(524_288.5),
                eta: Some(6),
                track: Track::Video,
            })
        );
    }

    #[test]
    fn falls_back_to_the_estimated_total_and_tolerates_missing_values() {
        let line = "MTPROG|2048.0|NA|9000|NA|NA|none|mp4a.40.2";
        assert_eq!(
            parse_line(line),
            Some(Line::Progress {
                downloaded: 2048,
                total: Some(9000),
                speed: None,
                eta: None,
                track: Track::Audio,
            })
        );
    }

    #[test]
    fn ignores_lines_that_are_not_ours() {
        assert_eq!(parse_line("[youtube] Extracting URL"), None);
        assert_eq!(parse_line("MTPROG|abc|NA|NA|NA|NA|x|y"), None);
        assert_eq!(parse_line("MTPROG|1|2"), None);
    }

    #[test]
    fn parses_the_final_path_even_with_a_pipe_in_it() {
        assert_eq!(
            parse_line("MTFILE|/tmp/a | b [id].mp4"),
            Some(Line::File("/tmp/a | b [id].mp4".into()))
        );
    }

    #[test]
    fn only_started_postprocessing_is_reported() {
        assert_eq!(
            parse_line("MTPOST|Merger|started"),
            Some(Line::PostProcess("Merger".into()))
        );
        assert_eq!(parse_line("MTPOST|Merger|finished"), None);
    }

    #[test]
    fn picks_the_format_selector() {
        assert_eq!(format_selector(Quality::Best, true), "bv*+ba/b");
        assert_eq!(format_selector(Quality::Best, false), "b");
        assert_eq!(
            format_selector(Quality::Max(720), true),
            "bv*[height<=720]+ba/b[height<=720]/b"
        );
        assert_eq!(format_selector(Quality::Max(480), false), "b[height<=480]/b");
        assert_eq!(Quality::parse("1080"), Quality::Max(1080));
        assert_eq!(Quality::parse("best"), Quality::Best);
    }

    #[test]
    fn the_url_always_comes_after_the_option_separator() {
        let args = build_args("https://ex.com/--version", Path::new("/d"), "b", None);
        let n = args.len();
        assert_eq!(args[n - 2], "--");
        assert_eq!(args[n - 1], "https://ex.com/--version");
    }

    #[test]
    fn explains_ytdlp_errors() {
        assert!(explain_error("ERROR: Unsupported URL: x", Some(1)).contains("prise en charge"));
        assert!(explain_error("ERROR: ffmpeg not found", Some(1)).contains("ffmpeg"));
        assert!(explain_error("WARNING: x\nERROR: Video unavailable", Some(1))
            .contains("Video unavailable"));
        assert!(explain_error("", Some(3)).contains("code 3"));
    }

    #[test]
    fn progress_message_combines_size_speed_and_eta() {
        let update = progress_for(
            1024 * 1024,
            Some(4 * 1024 * 1024),
            Some(512.0 * 1024.0),
            Some(6),
            Track::Video,
        );
        assert_eq!(update.fraction, Some(0.25));
        assert_eq!(
            update.message,
            "Vidéo : 1,0 Mo sur 4,0 Mo, 512,0 Ko/s, 6 s restantes"
        );
    }
}
