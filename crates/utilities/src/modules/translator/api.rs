//! Appels au service de traduction gratuit MyMemory (https://mymemory.translated.net),
//! qui ne demande ni compte ni clé.

use std::time::Duration;

use multitool_core::UtilityError;
use serde_json::Value;

pub const DEFAULT_URL: &str = "https://api.mymemory.translated.net/get";

pub struct Client {
    base_url: String,
    email: Option<String>,
    agent: ureq::Agent,
}

impl Client {
    pub fn new(base_url: &str, email: Option<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent("Multitool/0.1")
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        Self {
            base_url: base_url.to_string(),
            email,
            agent: ureq::Agent::new_with_config(config),
        }
    }

    /// Traduit un morceau de texte. `from` est un code de langue ou `Autodetect`.
    pub fn translate(&self, text: &str, from: &str, to: &str) -> Result<String, UtilityError> {
        let mut request = self
            .agent
            .get(&self.base_url)
            .query("q", text)
            .query("langpair", format!("{from}|{to}"));
        if let Some(email) = &self.email {
            request = request.query("de", email);
        }
        let mut response = request.call().map_err(network_error)?;
        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string().map_err(network_error)?;
        match status {
            429 => Err(quota_error()),
            500..=599 => Err(UtilityError::Failed(format!(
                "Le service de traduction est indisponible pour le moment (erreur {status}). Réessayez dans quelques minutes."
            ))),
            _ => parse_response(&body),
        }
    }
}

fn quota_error() -> UtilityError {
    UtilityError::Failed(
        "La limite gratuite de traduction est atteinte pour aujourd'hui. Indiquez une adresse e-mail dans le champ prévu pour obtenir une limite plus élevée, ou réessayez demain.".into(),
    )
}

fn network_error(error: ureq::Error) -> UtilityError {
    let message = match error {
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Io(_) => {
            "Impossible de joindre le service de traduction. Vérifiez votre connexion internet."
        }
        ureq::Error::Timeout(_) => "Le service de traduction ne répond pas (délai dépassé).",
        _ => "La communication avec le service de traduction a échoué.",
    };
    UtilityError::Failed(message.into())
}

/// Lit la réponse JSON du service et en extrait le texte traduit.
pub fn parse_response(body: &str) -> Result<String, UtilityError> {
    let json: Value = serde_json::from_str(body).map_err(|_| {
        UtilityError::Failed("Réponse inattendue du service de traduction.".into())
    })?;
    let translated = json["responseData"]["translatedText"].as_str().unwrap_or("");
    if json["quotaFinished"].as_bool() == Some(true) || translated.starts_with("MYMEMORY WARNING") {
        return Err(quota_error());
    }
    // Le code arrive tantôt en nombre, tantôt en texte.
    let status = match &json["responseStatus"] {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
    .unwrap_or(200);
    if status != 200 {
        let details = json["responseDetails"].as_str().unwrap_or("").to_uppercase();
        let message = if details.contains("INVALID") && details.contains("LANGUAGE") {
            "Le service ne gère pas cette combinaison de langues.".to_string()
        } else if status == 429 {
            return Err(quota_error());
        } else {
            format!("Le service de traduction a refusé la demande (code {status}).")
        };
        return Err(UtilityError::Failed(message));
    }
    if translated.is_empty() {
        return Err(UtilityError::Failed(
            "Le service n'a renvoyé aucune traduction pour ce texte.".into(),
        ));
    }
    Ok(decode_entities(translated))
}

/// MyMemory renvoie parfois des entités HTML (`&#39;`, `&quot;`) : on les remplace par les vrais caractères.
fn decode_entities(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        result.push_str(&rest[..start]);
        rest = &rest[start..];
        let decoded = rest.find(';').filter(|end| *end <= 8).and_then(|end| {
            let entity = &rest[1..end];
            let character = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => entity
                    .strip_prefix('#')
                    .and_then(|n| {
                        n.strip_prefix(['x', 'X'])
                            .map_or_else(|| n.parse().ok(), |hex| u32::from_str_radix(hex, 16).ok())
                    })
                    .and_then(char::from_u32),
            };
            character.map(|c| (c, end))
        });
        match decoded {
            Some((character, end)) => {
                result.push(character);
                rest = &rest[end + 1..];
            }
            None => {
                result.push('&');
                rest = &rest[1..];
            }
        }
    }
    result.push_str(rest);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(result: Result<String, UtilityError>) -> String {
        result.unwrap_err().to_string()
    }

    #[test]
    fn reads_a_normal_response() {
        let body = r#"{"responseData":{"translatedText":"Hello world","match":1},"quotaFinished":false,"responseStatus":200}"#;
        assert_eq!(parse_response(body).unwrap(), "Hello world");
    }

    #[test]
    fn status_can_be_a_string() {
        let body = r#"{"responseData":{"translatedText":"Hi"},"responseStatus":"200"}"#;
        assert_eq!(parse_response(body).unwrap(), "Hi");
    }

    #[test]
    fn html_entities_are_decoded() {
        let body = r#"{"responseData":{"translatedText":"It&#39;s &quot;fine&quot; &amp; 100 &lt; 200 &#x41; &unknown; AT&T"},"responseStatus":200}"#;
        assert_eq!(parse_response(body).unwrap(), "It's \"fine\" & 100 < 200 A &unknown; AT&T");
    }

    #[test]
    fn the_daily_quota_gets_a_clear_message() {
        let by_flag = r#"{"responseData":{"translatedText":"x"},"quotaFinished":true,"responseStatus":200}"#;
        assert!(message(parse_response(by_flag)).contains("limite gratuite"));
        let by_text = r#"{"responseData":{"translatedText":"MYMEMORY WARNING: YOU USED ALL AVAILABLE FREE TRANSLATIONS FOR TODAY."},"responseStatus":200}"#;
        assert!(message(parse_response(by_text)).contains("limite gratuite"));
    }

    #[test]
    fn errors_are_explained_in_french() {
        let invalid = r#"{"responseData":{"translatedText":""},"responseStatus":403,"responseDetails":"'XX' IS AN INVALID TARGET LANGUAGE"}"#;
        assert!(message(parse_response(invalid)).contains("combinaison de langues"));
        let other = r#"{"responseData":{"translatedText":""},"responseStatus":500,"responseDetails":"boom"}"#;
        assert!(message(parse_response(other)).contains("code 500"));
        assert!(message(parse_response("<html>")).contains("inattendue"));
        let empty = r#"{"responseData":{"translatedText":""},"responseStatus":200}"#;
        assert!(message(parse_response(empty)).contains("aucune traduction"));
    }
}
