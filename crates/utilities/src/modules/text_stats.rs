//! Utilitaire d'exemple : statistiques sur un texte.

use multitool_core::{Field, Input, Output, Registry, Utility, UtilityError, UtilityInfo};

struct TextStats;

impl Utility for TextStats {
    fn info(&self) -> UtilityInfo {
        UtilityInfo::new(
            "text_stats",
            "Statistiques de texte",
            "Compte les caractères, mots, lignes et phrases d'un texte.",
            "Texte",
        )
    }

    fn fields(&self) -> Vec<Field> {
        vec![Field::textarea("text", "Texte à analyser")
            .placeholder("Saisissez ou collez votre texte ici…")]
    }

    fn run(&self, input: &Input) -> Result<Output, UtilityError> {
        let text = input.string("text");
        if text.trim().is_empty() {
            return Err(UtilityError::InvalidInput("Le texte est vide.".into()));
        }

        let characters = text.chars().count();
        let without_spaces = text.chars().filter(|c| !c.is_whitespace()).count();
        // Un mot contient au moins une lettre ou un chiffre (« ? » seul n'en est pas un).
        let words = text
            .split_whitespace()
            .filter(|word| word.chars().any(char::is_alphanumeric))
            .count();
        let lines = text.lines().count();
        let sentences = text
            .split(['.', '!', '?', '…'])
            .filter(|part| !part.trim().is_empty())
            .count();

        Ok(Output::table(vec![
            ("Caractères", characters.to_string()),
            ("Caractères sans espaces", without_spaces.to_string()),
            ("Mots", words.to_string()),
            ("Lignes", lines.to_string()),
            ("Phrases (estimation)", sentences.to_string()),
        ]))
    }
}

pub fn register(registry: &mut Registry) {
    registry.register(Box::new(TextStats));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value_of(output: &Output, label: &str) -> String {
        output
            .table
            .iter()
            .find(|entry| entry.label == label)
            .map(|entry| entry.value.clone())
            .unwrap()
    }

    #[test]
    fn counts_words_lines_and_sentences() {
        let input = Input::new().with("text", "Bonjour le monde. Ça va ?\nTrès bien !");
        let out = TextStats.run(&input).unwrap();
        assert_eq!(value_of(&out, "Mots"), "7");
        assert_eq!(value_of(&out, "Lignes"), "2");
        assert_eq!(value_of(&out, "Phrases (estimation)"), "3");
    }

    #[test]
    fn counts_accented_characters_as_one() {
        let out = TextStats.run(&Input::new().with("text", "été")).unwrap();
        assert_eq!(value_of(&out, "Caractères"), "3");
    }

    #[test]
    fn rejects_empty_text() {
        assert!(TextStats.run(&Input::new().with("text", "  \n ")).is_err());
    }
}
