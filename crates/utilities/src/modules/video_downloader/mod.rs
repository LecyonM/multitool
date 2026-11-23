//! Téléchargeur de vidéo à partir d'un lien internet, avec barre d'avancement.
//!
//! Deux moteurs, choisis automatiquement :
//! - **lien direct** (fichier .mp4, .webm…) : téléchargé nativement (`direct.rs`) ;
//! - **page d'un site vidéo** (YouTube, etc.) : confié à yt-dlp s'il est installé (`ytdlp.rs`).
//!
//! Ce module est un dossier (`mod.rs` + fichiers) : exemple d'utilitaire découpé en plusieurs fichiers.

mod direct;
mod files;
mod format;
mod ytdlp;

use std::path::{Path, PathBuf};

use multitool_core::{
    Field, Input, Output, ProgressUpdate, Registry, Utility, UtilityError, UtilityInfo,
};

struct VideoDownloader;

impl Utility for VideoDownloader {
    fn info(&self) -> UtilityInfo {
        UtilityInfo::new(
            "video_downloader",
            "Téléchargeur de vidéo",
            "Télécharge une vidéo à partir d'un lien internet : fichier direct ou page d'un site vidéo (avec yt-dlp).",
            "Internet",
        )
    }

    fn fields(&self) -> Vec<Field> {
        vec![
            Field::text("url", "Lien de la vidéo").placeholder("https://…"),
            Field::folder("folder", "Dossier de destination"),
            Field::select(
                "quality",
                "Qualité maximale (sites vidéo)",
                &[
                    ("best", "La meilleure disponible"),
                    ("1080", "1080p"),
                    ("720", "720p"),
                    ("480", "480p"),
                ],
            ),
        ]
    }

    fn run(&self, input: &Input) -> Result<Output, UtilityError> {
        self.run_with_progress(input, &|_| {})
    }

    fn run_with_progress(
        &self,
        input: &Input,
        progress: &dyn Fn(ProgressUpdate),
    ) -> Result<Output, UtilityError> {
        execute(input, progress, &ytdlp::find)
    }
}

/// Cœur de l'utilitaire. `find_ytdlp` n'est appelé que si le lien n'est pas un
/// fichier direct ; il est injectable pour pouvoir tester sans yt-dlp installé.
fn execute(
    input: &Input,
    progress: &dyn Fn(ProgressUpdate),
    find_ytdlp: &dyn Fn() -> Option<PathBuf>,
) -> Result<Output, UtilityError> {
    let url = validate_url(&input.string("url"))?;
    let dir = prepare_folder(&input.string("folder"))?;
    let quality = ytdlp::Quality::parse(&input.string("quality"));

    let saved = match direct::download(&url, &dir, progress)? {
        direct::Outcome::Saved(saved) => saved,
        direct::Outcome::NotAFile(reason) => match find_ytdlp() {
            Some(ytdlp) => ytdlp::download(&ytdlp, &url, &dir, quality, progress)?,
            None => {
                return Err(UtilityError::Failed(format!(
                    "{reason} {}",
                    ytdlp::NOT_INSTALLED
                )))
            }
        },
    };

    progress(ProgressUpdate::new(1.0, "Téléchargement terminé"));
    let name = saved
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder = saved
        .path
        .parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_default();
    Ok(Output::table(vec![
        ("Fichier", name),
        ("Dossier", folder),
        ("Taille", format::bytes(saved.bytes)),
    ]))
}

fn validate_url(raw: &str) -> Result<String, UtilityError> {
    let url = raw.trim();
    if url.is_empty() {
        return Err(UtilityError::InvalidInput(
            "Saisissez le lien de la vidéo.".into(),
        ));
    }
    let lower = url.to_ascii_lowercase();
    let host = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .ok_or_else(|| {
            UtilityError::InvalidInput("Le lien doit commencer par http:// ou https://.".into())
        })?;
    if url.chars().any(char::is_whitespace) {
        return Err(UtilityError::InvalidInput(
            "Le lien ne doit pas contenir d'espaces.".into(),
        ));
    }
    if host.is_empty() || host.starts_with('/') {
        return Err(UtilityError::InvalidInput(
            "Ce lien n'est pas une adresse valide.".into(),
        ));
    }
    Ok(url.to_string())
}

fn prepare_folder(raw: &str) -> Result<PathBuf, UtilityError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(UtilityError::InvalidInput(
            "Choisissez le dossier de destination.".into(),
        ));
    }
    let dir = Path::new(raw);
    if dir.is_file() {
        return Err(UtilityError::InvalidInput(
            "Le dossier de destination est en réalité un fichier.".into(),
        ));
    }
    std::fs::create_dir_all(dir).map_err(|err| {
        UtilityError::Failed(format!("Impossible de créer le dossier de destination : {err}."))
    })?;
    Ok(dir.to_path_buf())
}

pub fn register(registry: &mut Registry) {
    registry.register(Box::new(VideoDownloader));
}

#[cfg(test)]
mod tests {
    use super::*;
    use multitool_core::FieldKind;
    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Mutex;
    use std::thread;
    use std::time::Duration;

    /// Sérialise les tests qui écrivent puis lancent un script, pour éviter la
    /// course « text file busy » quand un autre test crée un processus en parallèle.
    static PROCESS_LOCK: Mutex<()> = Mutex::new(());

    /// Contenu servi pour `/clip.mp4` : 600 Ko reconnaissables.
    fn clip() -> Vec<u8> {
        (0..600 * 1024).map(|i| (i % 251) as u8).collect()
    }

    /// Serveur HTTP local minimal ; renvoie son adresse (`http://127.0.0.1:port`).
    fn serve() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                thread::spawn(move || handle(stream));
            }
        });
        address
    }

    fn handle(mut stream: TcpStream) {
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        while !request.ends_with(b"\r\n\r\n") {
            if stream.read(&mut byte).unwrap_or(0) == 0 {
                return;
            }
            request.push(byte[0]);
        }
        let first_line = String::from_utf8_lossy(&request);
        let path = first_line
            .lines()
            .next()
            .unwrap_or_default()
            .split(' ')
            .nth(1)
            .unwrap_or("/")
            .to_string();

        let body = clip();
        match path.as_str() {
            "/clip.mp4" | "/slow-name" => {
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                for chunk in body.chunks(100 * 1024) {
                    if stream.write_all(chunk).is_err() {
                        return;
                    }
                    thread::sleep(Duration::from_millis(60));
                }
            }
            "/no-length.mp4" => {
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nConnection: close\r\n\r\n",
                );
                for chunk in body.chunks(150 * 1024) {
                    let _ = stream.write_all(chunk);
                    thread::sleep(Duration::from_millis(60));
                }
            }
            "/typed" => {
                // Aucun nom dans le lien ni l'en-tête : l'extension vient du type MIME.
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: video/webm\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
            "/attachment" => {
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename*=UTF-8''%C3%A9t%C3%A9.mp4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
            "/cut.mp4" => {
                // Annonce 1000 octets, n'en envoie que 400 puis coupe la connexion.
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: 1000\r\nConnection: close\r\n\r\n",
                );
                let _ = stream.write_all(&[7u8; 400]);
            }
            "/page" => {
                let html = "<html><body>Une page</body></html>";
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
                    html.len()
                );
                let _ = stream.write_all(head.as_bytes());
            }
            "/redirect" => {
                let _ = stream.write_all(
                    b"HTTP/1.1 302 Found\r\nLocation: /clip.mp4\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
            "/..%2F..%2Fevil.mp4" => {
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: 10\r\nConnection: close\r\n\r\n",
                );
                let _ = stream.write_all(&[1u8; 10]);
            }
            _ => {
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        }
    }

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mt-dl-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn input(url: &str, dir: &Path) -> Input {
        Input::new()
            .with("url", url)
            .with("folder", dir.to_str().unwrap())
            .with("quality", "best")
    }

    /// Exécute l'utilitaire en recueillant toutes les mises à jour de progression.
    fn run_collecting(
        input: &Input,
        find: &dyn Fn() -> Option<PathBuf>,
    ) -> (Result<Output, UtilityError>, Vec<ProgressUpdate>) {
        let updates = RefCell::new(Vec::new());
        let result = execute(input, &|update| updates.borrow_mut().push(update), find);
        (result, updates.into_inner())
    }

    fn no_ytdlp() -> Option<PathBuf> {
        None
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn downloads_a_direct_link_and_reports_progress() {
        let server = serve();
        let dir = temp_dir("direct");
        let (result, updates) =
            run_collecting(&input(&format!("{server}/clip.mp4"), &dir), &no_ytdlp);

        let output = result.unwrap();
        assert_eq!(output.table[0].value, "clip.mp4");
        assert_eq!(output.table[2].value, "600,0 Ko");
        assert_eq!(std::fs::read(dir.join("clip.mp4")).unwrap(), clip());
        assert_eq!(entries(&dir), ["clip.mp4"], "aucun fichier .part ne doit rester");

        // La barre avance, sans jamais reculer, et finit à 100 %.
        let fractions: Vec<f64> = updates.iter().filter_map(|u| u.fraction).collect();
        assert!(
            fractions.len() >= 3,
            "au moins 3 mises à jour attendues : {fractions:?}"
        );
        assert!(fractions.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(*fractions.last().unwrap(), 1.0);
        assert!(updates
            .iter()
            .any(|u| u.message.contains("Ko sur 600,0 Ko")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn never_overwrites_an_existing_file() {
        let server = serve();
        let dir = temp_dir("unique");
        let url = format!("{server}/clip.mp4");
        run_collecting(&input(&url, &dir), &no_ytdlp).0.unwrap();
        let second = run_collecting(&input(&url, &dir), &no_ytdlp).0.unwrap();
        assert_eq!(second.table[0].value, "clip (1).mp4");
        assert_eq!(entries(&dir), ["clip (1).mp4", "clip.mp4"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reports_an_indeterminate_bar_when_the_size_is_unknown() {
        let server = serve();
        let dir = temp_dir("nolen");
        let (result, updates) =
            run_collecting(&input(&format!("{server}/no-length.mp4"), &dir), &no_ytdlp);
        result.unwrap();
        assert_eq!(std::fs::read(dir.join("no-length.mp4")).unwrap(), clip());
        assert!(updates
            .iter()
            .any(|u| u.fraction.is_none() && u.message.contains("reçus")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn follows_redirects() {
        let server = serve();
        let dir = temp_dir("redirect");
        let output = run_collecting(&input(&format!("{server}/redirect"), &dir), &no_ytdlp)
            .0
            .unwrap();
        assert_eq!(output.table[0].value, "clip.mp4");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn uses_the_server_filename_or_the_mime_type() {
        let server = serve();
        let dir = temp_dir("names");
        let named = run_collecting(&input(&format!("{server}/attachment"), &dir), &no_ytdlp)
            .0
            .unwrap();
        assert_eq!(named.table[0].value, "été.mp4");
        let typed = run_collecting(&input(&format!("{server}/typed"), &dir), &no_ytdlp)
            .0
            .unwrap();
        assert_eq!(typed.table[0].value, "typed.webm");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_malicious_filename_cannot_escape_the_folder() {
        let server = serve();
        let base = temp_dir("escape");
        let dir = base.join("dest");
        run_collecting(&input(&format!("{server}/..%2F..%2Fevil.mp4"), &dir), &no_ytdlp)
            .0
            .unwrap();
        assert_eq!(entries(&dir), ["_.._evil.mp4"]);
        assert_eq!(entries(&base), ["dest"], "rien ne doit sortir du dossier");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn creates_the_destination_folder_when_missing() {
        let server = serve();
        let base = temp_dir("mkdir");
        let dir = base.join("a").join("b");
        run_collecting(&input(&format!("{server}/clip.mp4"), &dir), &no_ytdlp)
            .0
            .unwrap();
        assert!(dir.join("clip.mp4").is_file());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_truncated_download_fails_and_leaves_nothing_behind() {
        let server = serve();
        let dir = temp_dir("cut");
        let (result, _) = run_collecting(&input(&format!("{server}/cut.mp4"), &dir), &no_ytdlp);
        let message = result.unwrap_err().to_string();
        assert!(
            message.contains("interrompu") || message.contains("incomplet"),
            "{message}"
        );
        assert!(entries(&dir).is_empty(), "{:?}", entries(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_web_page_without_ytdlp_explains_how_to_install_it() {
        let server = serve();
        let dir = temp_dir("page");
        let (result, _) = run_collecting(&input(&format!("{server}/page"), &dir), &no_ytdlp);
        let message = result.unwrap_err().to_string();
        assert!(message.contains("page web"), "{message}");
        assert!(message.contains("winget install yt-dlp"), "{message}");
        assert!(entries(&dir).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_link_is_reported_in_french_without_involving_ytdlp() {
        let server = serve();
        let dir = temp_dir("404");
        // yt-dlp ne pourrait rien pour un lien qui n'existe plus : il ne doit pas être appelé,
        // et l'utilisateur ne doit pas se voir proposer de l'installer.
        let (result, _) = run_collecting(
            &input(&format!("{server}/absent.mp4"), &dir),
            &|| -> Option<PathBuf> { panic!("yt-dlp ne doit pas être cherché pour un 404") },
        );
        let message = result.unwrap_err().to_string();
        assert!(message.contains("introuvable"), "{message}");
        assert!(message.contains("404"), "{message}");
        assert!(!message.contains("yt-dlp"), "{message}");
        assert!(entries(&dir).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreachable_server_gives_a_clear_error() {
        let dir = temp_dir("down");
        // Port 1 : personne n'écoute.
        let (result, _) = run_collecting(&input("http://127.0.0.1:1/x.mp4", &dir), &no_ytdlp);
        let message = result.unwrap_err().to_string();
        assert!(message.contains("Connexion impossible"), "{message}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_invalid_input_before_any_network_access() {
        let dir = temp_dir("invalid");
        let check = |url: &str, folder: &str, expected: &str| {
            let input = Input::new().with("url", url).with("folder", folder);
            let message = execute(&input, &|_| {}, &no_ytdlp).unwrap_err().to_string();
            assert!(message.contains(expected), "{url:?} : {message}");
        };
        let folder = dir.to_str().unwrap();
        check("", folder, "Saisissez le lien");
        check("   ", folder, "Saisissez le lien");
        check("ftp://exemple.fr/a.mp4", folder, "http://");
        check("exemple.fr/a.mp4", folder, "http://");
        check("https://exemple.fr/a b.mp4", folder, "espaces");
        check("https:///a.mp4", folder, "adresse valide");
        check("https://exemple.fr/a.mp4", "", "dossier de destination");
        std::fs::write(dir.join("fichier"), b"x").unwrap();
        check(
            "https://exemple.fr/a.mp4",
            dir.join("fichier").to_str().unwrap(),
            "en réalité un fichier",
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn declares_a_folder_field_with_a_default() {
        let fields = VideoDownloader.fields();
        let folder = fields.iter().find(|field| field.id == "folder").unwrap();
        assert_eq!(folder.kind, FieldKind::Folder);
    }

    /// Faux yt-dlp : un script qui imite sa sortie, pour tester le pilotage du
    /// processus (arguments, lecture de la progression, erreurs) sans réseau.
    #[cfg(unix)]
    mod fake_ytdlp {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn script(dir: &Path, body: &str) -> PathBuf {
            let path = dir.join("fake-yt-dlp");
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        const SUCCESS: &str = r#"
dir=""
while [ $# -gt 0 ]; do
  if [ "$1" = "-P" ]; then dir="$2"; fi
  shift
done
echo "[youtube] bruit ignoré"
echo "MTPROG|100000|400000|NA|50000|6|avc1|none"
sleep 0.2
echo "MTPROG|400000|400000|NA|50000|0|avc1|none"
sleep 0.2
echo "MTPROG|10000|20000|NA|NA|NA|none|mp4a"
sleep 0.2
echo "MTPOST|Merger|started"
printf 'donnees' > "$dir/Ma vidéo [abc].mp4"
echo "MTFILE|$dir/Ma vidéo [abc].mp4"
"#;

        #[test]
        fn drives_a_ytdlp_process_and_reports_its_progress() {
            let _guard = PROCESS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir = temp_dir("fake-ok");
            let fake = script(&dir, SUCCESS);
            let server = serve();
            let target = dir.join("out");
            let (result, updates) = run_collecting(
                &input(&format!("{server}/page"), &target),
                &|| Some(fake.clone()),
            );

            let output = result.unwrap();
            assert_eq!(output.table[0].value, "Ma vidéo [abc].mp4");
            assert_eq!(output.table[2].value, "7 o");

            let messages: Vec<&str> = updates.iter().map(|u| u.message.as_str()).collect();
            assert!(messages.iter().any(|m| m.contains("Analyse du lien")));
            assert!(messages
                .iter()
                .any(|m| m.starts_with("Vidéo : 97,7 Ko sur 390,6 Ko")));
            assert!(messages.iter().any(|m| m.starts_with("Audio : ")));
            assert!(messages.iter().any(|m| m.contains("Assemblage")));
            let last = updates.last().unwrap();
            assert_eq!(last.fraction, Some(1.0));
            std::fs::remove_dir_all(&dir).unwrap();
        }

        #[test]
        fn translates_a_ytdlp_failure() {
            let _guard = PROCESS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir = temp_dir("fake-fail");
            let fake = script(&dir, "echo 'ERROR: Video unavailable' >&2\nexit 1");
            let server = serve();
            let (result, _) = run_collecting(
                &input(&format!("{server}/page"), &dir.join("out")),
                &|| Some(fake.clone()),
            );
            let message = result.unwrap_err().to_string();
            assert!(message.contains("Video unavailable"), "{message}");
            std::fs::remove_dir_all(&dir).unwrap();
        }

        #[test]
        fn fails_when_ytdlp_succeeds_without_producing_a_file() {
            let _guard = PROCESS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir = temp_dir("fake-empty");
            let fake = script(&dir, "exit 0");
            let server = serve();
            let (result, _) = run_collecting(
                &input(&format!("{server}/page"), &dir.join("out")),
                &|| Some(fake.clone()),
            );
            assert!(result.unwrap_err().to_string().contains("aucun fichier"));
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    /// Test avec le vrai yt-dlp, uniquement s'il est installé sur la machine.
    /// Son extracteur générique sait télécharger un lien de fichier vidéo.
    #[test]
    fn works_with_the_real_ytdlp_when_installed() {
        let _guard = PROCESS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let Some(ytdlp) = ytdlp::find() else {
            eprintln!("yt-dlp absent : test ignoré");
            return;
        };
        let server = serve();
        let dir = temp_dir("real-ytdlp");
        let updates = RefCell::new(Vec::new());
        let saved = ytdlp::download(
            &ytdlp,
            &format!("{server}/clip.mp4"),
            &dir,
            ytdlp::Quality::Best,
            &|update| updates.borrow_mut().push(update),
        )
        .unwrap();
        assert!(saved.path.is_file());
        assert_eq!(saved.bytes, clip().len() as u64);
        assert!(updates
            .borrow()
            .iter()
            .any(|u| u.fraction.is_some() && u.message.contains("sur")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
