//! Langues proposées, des plus parlées au monde aux moins parlées (locuteurs natifs et seconds).

/// `(code, nom en français)`. Les codes sont ceux attendus par MyMemory (ISO 639-1, `zh-CN`, `zh-TW`).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "Anglais"),
    ("zh-CN", "Chinois (mandarin, simplifié)"),
    ("hi", "Hindi"),
    ("es", "Espagnol"),
    ("fr", "Français"),
    ("ar", "Arabe"),
    ("bn", "Bengali"),
    ("pt", "Portugais"),
    ("ru", "Russe"),
    ("ur", "Ourdou"),
    ("id", "Indonésien"),
    ("de", "Allemand"),
    ("ja", "Japonais"),
    ("sw", "Swahili"),
    ("mr", "Marathi"),
    ("te", "Télougou"),
    ("tr", "Turc"),
    ("ta", "Tamoul"),
    ("vi", "Vietnamien"),
    ("ko", "Coréen"),
    ("it", "Italien"),
    ("th", "Thaï"),
    ("gu", "Gujarati"),
    ("fa", "Persan"),
    ("pl", "Polonais"),
    ("uk", "Ukrainien"),
    ("pa", "Pendjabi"),
    ("kn", "Kannada"),
    ("ml", "Malayalam"),
    ("ms", "Malais"),
    ("jv", "Javanais"),
    ("ha", "Haoussa"),
    ("my", "Birman"),
    ("ro", "Roumain"),
    ("nl", "Néerlandais"),
    ("am", "Amharique"),
    ("yo", "Yoruba"),
    ("ne", "Népalais"),
    ("si", "Cingalais"),
    ("km", "Khmer"),
    ("fil", "Filipino (tagalog)"),
    ("el", "Grec"),
    ("cs", "Tchèque"),
    ("sv", "Suédois"),
    ("hu", "Hongrois"),
    ("he", "Hébreu"),
    ("zh-TW", "Chinois (traditionnel)"),
    ("bg", "Bulgare"),
    ("da", "Danois"),
    ("fi", "Finnois"),
    ("no", "Norvégien"),
    ("sk", "Slovaque"),
    ("sr", "Serbe"),
    ("hr", "Croate"),
    ("ca", "Catalan"),
    ("lt", "Lituanien"),
    ("lv", "Letton"),
    ("et", "Estonien"),
    ("sl", "Slovène"),
    ("af", "Afrikaans"),
    ("az", "Azéri"),
    ("kk", "Kazakh"),
    ("uz", "Ouzbek"),
    ("hy", "Arménien"),
    ("ka", "Géorgien"),
];

/// Valeur du menu « langue d'origine » pour laisser le service deviner la langue.
pub const AUTO: &str = "auto";

/// Nom français d'une langue, ou le code lui-même s'il est inconnu.
pub fn name_of(code: &str) -> &str {
    LANGUAGES
        .iter()
        .find(|(known, _)| *known == code)
        .map_or(code, |(_, name)| name)
}

pub fn is_known(code: &str) -> bool {
    LANGUAGES.iter().any(|(known, _)| *known == code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn codes_and_names_are_unique() {
        let codes: HashSet<_> = LANGUAGES.iter().map(|(c, _)| *c).collect();
        let names: HashSet<_> = LANGUAGES.iter().map(|(_, n)| *n).collect();
        assert_eq!(codes.len(), LANGUAGES.len());
        assert_eq!(names.len(), LANGUAGES.len());
    }

    #[test]
    fn the_most_spoken_languages_are_all_there() {
        for code in ["en", "zh-CN", "hi", "es", "fr", "ar", "bn", "pt", "ru", "ur", "id", "de", "ja"] {
            assert!(is_known(code), "{code}");
        }
        assert!(LANGUAGES.len() >= 60);
    }

    #[test]
    fn lookup_falls_back_to_the_code() {
        assert_eq!(name_of("fr"), "Français");
        assert_eq!(name_of("xx"), "xx");
        assert!(!is_known(AUTO));
    }
}
