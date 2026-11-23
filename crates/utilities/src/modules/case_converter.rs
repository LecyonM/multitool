//! Utilitaire d'exemple : conversion de casse (MAJUSCULES, snake_case, camelCase…).

use multitool_core::{Field, Input, Output, Registry, Utility, UtilityError, UtilityInfo};

struct CaseConverter;

impl Utility for CaseConverter {
    fn info(&self) -> UtilityInfo {
        UtilityInfo::new(
            "case_converter",
            "Convertisseur de casse",
            "Change la casse d'un texte : majuscules, snake_case, camelCase, etc.",
            "Texte",
        )
    }

    fn fields(&self) -> Vec<Field> {
        vec![
            Field::textarea("text", "Texte à convertir"),
            Field::select(
                "mode",
                "Format de sortie",
                &[
                    ("upper", "MAJUSCULES"),
                    ("lower", "minuscules"),
                    ("title", "Première Lettre De Chaque Mot"),
                    ("snake", "snake_case"),
                    ("kebab", "kebab-case"),
                    ("camel", "camelCase"),
                    ("pascal", "PascalCase"),
                ],
            ),
        ]
    }

    fn run(&self, input: &Input) -> Result<Output, UtilityError> {
        let text = input.string("text");
        if text.trim().is_empty() {
            return Err(UtilityError::InvalidInput("Le texte est vide.".into()));
        }
        let words = split_words(&text);

        let result = match input.string("mode").as_str() {
            "upper" => text.to_uppercase(),
            "lower" => text.to_lowercase(),
            "title" => title_case(&text),
            "snake" => join_lower(&words, "_"),
            "kebab" => join_lower(&words, "-"),
            "camel" => words
                .iter()
                .enumerate()
                .map(|(i, w)| if i == 0 { w.to_lowercase() } else { capitalize(w) })
                .collect(),
            "pascal" => words.iter().map(|w| capitalize(w)).collect(),
            other => {
                return Err(UtilityError::InvalidInput(format!(
                    "Format inconnu : « {other} »."
                )))
            }
        };
        Ok(Output::text(result))
    }
}

/// Découpe en mots : sur tout caractère non alphanumérique et aux transitions
/// minuscule → majuscule (`monTexte` donne `mon`, `Texte`).
fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_is_lower = false;

    for c in text.chars() {
        if c.is_alphanumeric() {
            if c.is_uppercase() && previous_is_lower && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(c);
            previous_is_lower = c.is_lowercase() || c.is_numeric();
        } else {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            previous_is_lower = false;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn join_lower(words: &[String], separator: &str) -> String {
    words
        .iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(separator)
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect(),
        None => String::new(),
    }
}

fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_word_start = true;
    for c in text.chars() {
        if c.is_whitespace() {
            at_word_start = true;
            out.push(c);
        } else if at_word_start {
            out.extend(c.to_uppercase());
            at_word_start = false;
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

pub fn register(registry: &mut Registry) {
    registry.register(Box::new(CaseConverter));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(text: &str, mode: &str) -> String {
        let input = Input::new().with("text", text).with("mode", mode);
        CaseConverter.run(&input).unwrap().text
    }

    #[test]
    fn converts_to_the_requested_format() {
        assert_eq!(convert("Mon petit texte", "upper"), "MON PETIT TEXTE");
        assert_eq!(convert("Mon petit texte", "lower"), "mon petit texte");
        assert_eq!(convert("mON petit TEXTE", "title"), "Mon Petit Texte");
        assert_eq!(convert("Mon petit texte", "snake"), "mon_petit_texte");
        assert_eq!(convert("Mon petit texte", "kebab"), "mon-petit-texte");
        assert_eq!(convert("Mon petit texte", "camel"), "monPetitTexte");
        assert_eq!(convert("Mon petit texte", "pascal"), "MonPetitTexte");
    }

    #[test]
    fn understands_camel_case_input() {
        assert_eq!(convert("monPetitTexte", "snake"), "mon_petit_texte");
    }

    #[test]
    fn handles_accents() {
        assert_eq!(convert("école élémentaire", "pascal"), "ÉcoleÉlémentaire");
    }

    #[test]
    fn rejects_unknown_mode() {
        let input = Input::new().with("text", "a").with("mode", "???");
        assert!(CaseConverter.run(&input).is_err());
    }
}
