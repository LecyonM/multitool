//! Noms de fichiers : extraction depuis le lien ou les en-têtes, nettoyage et unicité.

use std::path::{Path, PathBuf};

const MAX_NAME_CHARS: usize = 150;

/// Noms que Windows interdit, quelle que soit l'extension (« CON.mp4 » compris).
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Rend un nom utilisable sur Windows, macOS et Linux, sans jamais pouvoir
/// sortir du dossier de destination (« ../../x » devient « .._.._x »).
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim().to_string();

    let (stem, extension) = split_extension(&cleaned);
    // Une extension anormalement longue n'en est pas une : on la garde dans le nom.
    let (stem, extension) = if extension.chars().count() > 8 {
        (cleaned.as_str(), "")
    } else {
        (stem, extension)
    };

    let extension_len = if extension.is_empty() {
        0
    } else {
        extension.chars().count() + 1
    };
    let budget = MAX_NAME_CHARS.saturating_sub(extension_len).max(1);
    let mut stem: String = stem.chars().take(budget).collect();
    stem = stem.trim().to_string();

    if stem.is_empty() {
        stem = "video".into();
    }
    if RESERVED.contains(&stem.to_uppercase().as_str()) {
        stem.insert(0, '_');
    }

    if extension.is_empty() {
        stem
    } else {
        format!("{stem}.{extension}")
    }
}

/// Sépare `film.final.mp4` en (`film.final`, `mp4`) ; extension vide s'il n'y en a pas.
pub fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => (&name[..dot], &name[dot + 1..]),
        _ => (name, ""),
    }
}

/// Dernier segment du chemin du lien, sans paramètres ni fragment, décodé.
pub fn name_from_url(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let path = after_scheme.split_once('/')?.1;
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let last = path.rsplit('/').next().unwrap_or_default();
    let decoded = percent_decode(last);
    (!decoded.trim().is_empty()).then_some(decoded)
}

/// Nom proposé par l'en-tête `Content-Disposition` (`filename*=` prioritaire).
pub fn name_from_content_disposition(header: &str) -> Option<String> {
    let mut plain = None;
    for part in header.split(';').map(str::trim) {
        let lower = part.to_ascii_lowercase();
        if lower.starts_with("filename*=") {
            // Forme `UTF-8''nom%20encod%C3%A9`.
            let value = part["filename*=".len()..].trim();
            let encoded = value.rsplit("''").next().unwrap_or(value);
            let decoded = percent_decode(encoded.trim_matches('"'));
            if !decoded.trim().is_empty() {
                return Some(decoded);
            }
        } else if lower.starts_with("filename=") {
            let value = part["filename=".len()..].trim().trim_matches('"');
            if !value.trim().is_empty() {
                plain = Some(value.to_string());
            }
        }
    }
    plain
}

/// Extension usuelle d'un type MIME vidéo ou audio.
pub fn extension_for_mime(mime: &str) -> Option<&'static str> {
    match mime.split(';').next()?.trim().to_ascii_lowercase().as_str() {
        "video/mp4" => Some("mp4"),
        "video/webm" => Some("webm"),
        "video/x-matroska" => Some("mkv"),
        "video/quicktime" => Some("mov"),
        "video/x-msvideo" => Some("avi"),
        "video/mpeg" => Some("mpeg"),
        "video/ogg" => Some("ogv"),
        "video/x-flv" => Some("flv"),
        "video/3gpp" => Some("3gp"),
        "video/mp2t" => Some("ts"),
        "audio/mpeg" => Some("mp3"),
        "audio/mp4" | "audio/x-m4a" => Some("m4a"),
        "audio/ogg" => Some("ogg"),
        "audio/wav" | "audio/x-wav" => Some("wav"),
        "audio/webm" => Some("weba"),
        _ => None,
    }
}

/// Premier chemin libre : `video.mp4`, puis `video (1).mp4`, `video (2).mp4`…
/// Un téléchargement n'écrase donc jamais un fichier existant.
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() && !part_path(&first).exists() {
        return first;
    }
    let (stem, extension) = split_extension(name);
    for n in 1..10_000 {
        let candidate = if extension.is_empty() {
            format!("{stem} ({n})")
        } else {
            format!("{stem} ({n}).{extension}")
        };
        let path = dir.join(candidate);
        if !path.exists() && !part_path(&path).exists() {
            return path;
        }
    }
    first
}

/// Fichier temporaire pendant le téléchargement : `video.mp4` donne `video.mp4.part`.
pub fn part_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // Il faut deux caractères après le « % » (indices i+1 et i+2).
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(value) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_removes_forbidden_characters_and_paths() {
        assert_eq!(sanitize("a/b\\c:d*e?.mp4"), "a_b_c_d_e_.mp4");
        assert_eq!(sanitize("../../secret.mp4"), "_.._secret.mp4");
        assert_eq!(sanitize("  film.mp4  "), "film.mp4");
        assert_eq!(sanitize("...."), "video");
        assert_eq!(sanitize(""), "video");
    }

    #[test]
    fn sanitize_avoids_windows_reserved_names() {
        assert_eq!(sanitize("CON.mp4"), "_CON.mp4");
        assert_eq!(sanitize("nul"), "_nul");
        assert_eq!(sanitize("console.mp4"), "console.mp4");
    }

    #[test]
    fn sanitize_truncates_long_names_but_keeps_the_extension() {
        let name = format!("{}.mp4", "é".repeat(400));
        let out = sanitize(&name);
        assert!(out.chars().count() <= MAX_NAME_CHARS);
        assert!(out.ends_with(".mp4"));
    }

    #[test]
    fn name_from_url_handles_queries_and_encoding() {
        assert_eq!(
            name_from_url("https://ex.com/a/b/mon%20film.mp4?token=1#t=5").as_deref(),
            Some("mon film.mp4")
        );
        assert_eq!(name_from_url("https://ex.com/"), None);
        assert_eq!(name_from_url("https://ex.com"), None);
    }

    #[test]
    fn name_from_content_disposition_prefers_the_utf8_form() {
        assert_eq!(
            name_from_content_disposition("attachment; filename=\"a.mp4\"").as_deref(),
            Some("a.mp4")
        );
        assert_eq!(
            name_from_content_disposition(
                "attachment; filename=\"fallback.mp4\"; filename*=UTF-8''%C3%A9t%C3%A9.mp4"
            )
            .as_deref(),
            Some("été.mp4")
        );
        assert_eq!(name_from_content_disposition("inline"), None);
    }

    #[test]
    fn percent_decode_leaves_invalid_sequences_alone() {
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("a%zzb"), "a%zzb");
        assert_eq!(percent_decode("%41%"), "A%");
    }

    #[test]
    fn mime_types_map_to_extensions() {
        assert_eq!(extension_for_mime("video/mp4; codecs=avc1"), Some("mp4"));
        assert_eq!(extension_for_mime("text/html"), None);
    }

    #[test]
    fn unique_path_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("mt-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unique_path(&dir, "v.mp4"), dir.join("v.mp4"));
        std::fs::write(dir.join("v.mp4"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "v.mp4"), dir.join("v (1).mp4"));
        std::fs::write(dir.join("v (1).mp4.part"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "v.mp4"), dir.join("v (2).mp4"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
