//! « Traducteur » : traduit un texte entre plus de 60 langues, parmi les plus parlées au monde.
//!
//! Le texte est envoyé au service gratuit MyMemory (internet requis, aucun compte).
//! Les longs textes sont découpés en morceaux (le service en accepte 500 octets à la fois)
//! et la barre de progression indique le morceau en cours.
//!
//! - `languages.rs` : la liste des langues ;
//! - `chunks.rs`    : le découpage du texte, sans perte ;
//! - `api.rs`       : les appels au service et la lecture des réponses.

mod api;
mod chunks;
mod languages;

use multitool_core::{
    Field, Input, Output, ProgressUpdate, Registry, Utility, UtilityError, UtilityInfo,
};
use serde_json::json;

use languages::{name_of, AUTO, LANGUAGES};

/// Taille maximale d'un morceau envoyé au service (sa limite est de 500 octets).
const CHUNK_BYTES: usize = 450;
/// Longueur maximale d'un texte, pour garder un traitement raisonnable.
const MAX_CHARS: usize = 10_000;

pub fn register(registry: &mut Registry) {
    registry.register(Box::new(Translator::new()));
}

pub struct Translator {
    base_url: String,
}

impl Translator {
    fn new() -> Self {
        // Variable d'environnement réservée aux essais : pointer vers un autre serveur.
        let base_url =
            std::env::var("MULTITOOL_TRANSLATE_URL").unwrap_or_else(|_| api::DEFAULT_URL.into());
        Self { base_url }
    }
}

impl Utility for Translator {
    fn info(&self) -> UtilityInfo {
        UtilityInfo::new(
            "translator",
            "Traducteur",
            "Traduit un texte entre plus de 60 langues (anglais, chinois, hindi, espagnol, arabe…). Nécessite internet.",
            "Texte",
        )
    }

    fn fields(&self) -> Vec<Field> {
        let mut source: Vec<(&str, &str)> = vec![(AUTO, "Détecter la langue")];
        source.extend(LANGUAGES.iter().copied());
        vec![
            Field::textarea("text", "Texte à traduire").placeholder("Saisissez ou collez votre texte ici"),
            Field::select("from", "Langue d'origine", &source).default_value(json!(AUTO)),
            Field::select("to", "Traduire en", LANGUAGES).default_value(json!("fr")),
            Field::text("email", "Adresse e-mail (facultatif : relève la limite quotidienne gratuite)")
                .optional(),
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
        let text = input.string("text");
        if text.trim().is_empty() {
            return Err(UtilityError::InvalidInput(
                "Saisissez le texte à traduire.".into(),
            ));
        }
        let length = text.chars().count();
        if length > MAX_CHARS {
            return Err(UtilityError::InvalidInput(format!(
                "Texte trop long ({length} caractères) : le maximum est {MAX_CHARS}. Traduisez-le en plusieurs fois."
            )));
        }
        let from = input.string("from");
        let from = if from.is_empty() { AUTO.to_string() } else { from };
        let to = input.string("to");
        if from != AUTO && !languages::is_known(&from) {
            return Err(UtilityError::InvalidInput(format!("Langue d'origine inconnue : « {from} »."))); 
        }
        if !languages::is_known(&to) {
            return Err(UtilityError::InvalidInput(
                "Choisissez la langue dans laquelle traduire.".into(),
            ));
        }
        if from == to {
            return Err(UtilityError::InvalidInput(
                "La langue d'origine et la langue de traduction sont identiques.".into(),
            ));
        }
        let email = input.string("email");
        let email = email.trim();
        if !email.is_empty() && !email.contains('@') {
            return Err(UtilityError::InvalidInput(
                "L'adresse e-mail n'est pas valide (il manque le « @ »).".into(),
            ));
        }

        let client = api::Client::new(&self.base_url, (!email.is_empty()).then(|| email.to_string()));
        let service_from = if from == AUTO { "Autodetect" } else { from.as_str() };
        let chunks = chunks::split_chunks(&text, CHUNK_BYTES);
        let total = chunks.len();
        let mut translated = String::new();
        let mut parts = 0;
        for (index, chunk) in chunks.iter().enumerate() {
            progress(ProgressUpdate::new(
                index as f64 / total as f64,
                if total > 1 {
                    format!("Traduction : partie {} sur {total}", index + 1)
                } else {
                    "Traduction en cours".into()
                },
            ));
            let core = chunk.trim();
            if core.is_empty() {
                translated.push_str(chunk);
                continue;
            }
            // Les espaces et sauts de ligne autour du morceau sont conservés tels quels.
            let leading = &chunk[..chunk.len() - chunk.trim_start().len()];
            let trailing = &chunk[chunk.trim_end().len()..];
            translated.push_str(leading);
            translated.push_str(&client.translate(core, service_from, &to)?);
            translated.push_str(trailing);
            parts += 1;
        }
        progress(ProgressUpdate::new(1.0, "Traduction terminée"));

        let origin = if from == AUTO {
            "langue détectée automatiquement".to_string()
        } else {
            name_of(&from).to_string()
        };
        Ok(Output::table(vec![
            ("Traduction".to_string(), format!("{origin} → {}", name_of(&to))),
            ("Caractères traduits".to_string(), length.to_string()),
            ("Parties envoyées".to_string(), parts.to_string()),
        ])
        .with_text(translated))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    type Seen = Arc<Mutex<Vec<(String, String, Option<String>)>>>;

    fn percent_decode(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'%' => {
                    out.push(u8::from_str_radix(&text[i + 1..i + 3], 16).unwrap());
                    i += 3;
                }
                b'+' => {
                    out.push(b' ');
                    i += 1;
                }
                b => {
                    out.push(b);
                    i += 1;
                }
            }
        }
        String::from_utf8(out).unwrap()
    }

    /// Faux MyMemory : répond « <langue cible>texte ». Les mots-clés QUOTA et FAIL500
    /// dans le texte déclenchent les réponses d'erreur correspondantes.
    fn mock_server() -> (String, Seen) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/get", listener.local_addr().unwrap());
        let seen: Seen = Arc::default();
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let log = log.clone();
                std::thread::spawn(move || {
                    let mut stream = stream;
                    let mut request = Vec::new();
                    let mut buffer = [0u8; 4096];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut buffer) {
                            Ok(0) | Err(_) => return,
                            Ok(n) => request.extend_from_slice(&buffer[..n]),
                        }
                    }
                    let request = String::from_utf8_lossy(&request).to_string();
                    let target = request.lines().next().unwrap().split(' ').nth(1).unwrap().to_string();
                    let query = target.split_once('?').map_or("", |(_, q)| q);
                    let param = |name: &str| {
                        query
                            .split('&')
                            .find_map(|pair| pair.strip_prefix(&format!("{name}=")))
                            .map(percent_decode)
                    };
                    let q = param("q").unwrap_or_default();
                    let pair = param("langpair").unwrap_or_default();
                    log.lock().unwrap().push((q.clone(), pair.clone(), param("de")));
                    let (status, body) = if q.contains("FAIL500") {
                        ("500 Internal Server Error", String::from("oops"))
                    } else if q.contains("QUOTA") {
                        (
                            "200 OK",
                            json!({"responseData":{"translatedText":"MYMEMORY WARNING: YOU USED ALL AVAILABLE FREE TRANSLATIONS FOR TODAY."},"quotaFinished":true,"responseStatus":200}).to_string(),
                        )
                    } else {
                        let target = pair.split('|').nth(1).unwrap_or("?");
                        (
                            "200 OK",
                            json!({"responseData":{"translatedText":format!("<{target}>{q}")},"quotaFinished":false,"responseStatus":200}).to_string(),
                        )
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                });
            }
        });
        (url, seen)
    }

    fn translator(url: &str) -> Translator {
        Translator { base_url: url.to_string() }
    }

    fn input(text: &str, from: &str, to: &str) -> Input {
        Input::new().with("text", text).with("from", from).with("to", to)
    }

    #[test]
    fn translates_and_detects_the_language() {
        let (url, seen) = mock_server();
        let output = translator(&url).run(&input("Bonjour le monde", AUTO, "en")).unwrap();
        assert_eq!(output.text, "<en>Bonjour le monde");
        assert_eq!(output.table[0].value, "langue détectée automatiquement → Anglais");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].1, "Autodetect|en");
        assert_eq!(seen[0].2, None);
    }

    #[test]
    fn a_chosen_source_language_and_the_email_are_sent() {
        let (url, seen) = mock_server();
        let input = input("Hola", "es", "zh-CN").with("email", " moi@example.org ");
        translator(&url).run(&input).unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].1, "es|zh-CN");
        assert_eq!(seen[0].2.as_deref(), Some("moi@example.org"));
    }

    #[test]
    fn special_characters_survive_the_trip() {
        let (url, seen) = mock_server();
        let text = "Café & thé à 100 % + « guillemets » #1 ?=;/ 😀 漢字 العربية";
        let output = translator(&url).run(&input(text, "fr", "ja")).unwrap();
        assert_eq!(seen.lock().unwrap()[0].0, text);
        assert_eq!(output.text, format!("<ja>{text}"));
    }

    #[test]
    fn a_long_text_is_split_translated_and_reassembled_with_its_layout() {
        let (url, seen) = mock_server();
        let paragraph = "Ceci est une phrase de test. Voici une autre phrase un peu plus longue que la précédente ! Et une question ?\n\n";
        let text = format!("  {}Fin.\n", paragraph.repeat(12));
        let updates = Mutex::new(Vec::new());
        let output = translator(&url)
            .run_with_progress(&input(&text, "fr", "de"), &|u| updates.lock().unwrap().push(u))
            .unwrap();
        let seen = seen.lock().unwrap();
        assert!(seen.len() > 2, "{} morceaux", seen.len());
        assert!(seen.iter().all(|(q, _, _)| q.len() <= 450));
        // Sans les marqueurs du faux serveur, on retrouve exactement le texte d'origine (mise en page comprise).
        assert_eq!(output.text.replace("<de>", ""), text);
        assert_eq!(output.table[2].value, seen.len().to_string());
        let updates = updates.lock().unwrap();
        let fractions: Vec<f64> = updates.iter().filter_map(|u| u.fraction).collect();
        assert!(fractions.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(fractions.last(), Some(&1.0));
        assert!(updates.iter().any(|u| u.message.contains("partie 2 sur")));
    }

    #[test]
    fn service_errors_are_explained() {
        let (url, _) = mock_server();
        let quota = translator(&url).run(&input("QUOTA", "fr", "en")).unwrap_err().to_string();
        assert!(quota.contains("limite gratuite"), "{quota}");
        let down = translator(&url).run(&input("FAIL500", "fr", "en")).unwrap_err().to_string();
        assert!(down.contains("indisponible"), "{down}");
    }

    #[test]
    fn an_unreachable_service_is_reported() {
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/get", closed.local_addr().unwrap());
        drop(closed);
        let error = translator(&url).run(&input("Salut", "fr", "en")).unwrap_err().to_string();
        assert!(error.contains("Impossible de joindre"), "{error}");
    }

    #[test]
    fn invalid_input_is_refused_before_any_request() {
        let (url, seen) = mock_server();
        let t = translator(&url);
        let message = |i: Input| t.run(&i).unwrap_err().to_string();
        assert!(message(input("   ", "fr", "en")).contains("Saisissez"));
        assert!(message(input("Salut", "fr", "fr")).contains("identiques"));
        assert!(message(input("Salut", AUTO, "xx")).contains("Choisissez"));
        assert!(message(input("Salut", "xx", "en")).contains("inconnue"));
        assert!(message(input("Salut", "fr", "en").with("email", "pas-un-email")).contains("@"));
        assert!(message(input(&"a".repeat(MAX_CHARS + 1), "fr", "en")).contains("trop long"));
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn the_form_offers_every_language() {
        let fields = Translator::new().fields();
        let ids: Vec<_> = fields.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["text", "from", "to", "email"]);
        let count = |index: usize| match &fields[index].kind {
            multitool_core::FieldKind::Select { options } => options.len(),
            _ => 0,
        };
        assert_eq!(count(1), LANGUAGES.len() + 1, "« détecter » + toutes les langues");
        assert_eq!(count(2), LANGUAGES.len());
        assert_eq!(fields[1].default, Some(json!("auto")));
        assert_eq!(fields[2].default, Some(json!("fr")));
        assert!(!fields[3].required);
    }
}
