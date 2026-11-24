//! Découpe d'un long texte en morceaux que le service accepte (500 octets au maximum),
//! sans jamais couper au milieu d'une phrase quand c'est évitable.

/// Découpe `text` en morceaux d'au plus `max_bytes` octets.
/// Sans perte : `chunks.concat() == text`, espaces et sauts de ligne compris.
pub fn split_chunks(text: &str, max_bytes: usize) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for sentence in sentences(text) {
        for piece in hard_split(sentence, max_bytes) {
            if !current.is_empty() && current.len() + piece.len() > max_bytes {
                chunks.push(std::mem::take(&mut current));
            }
            current.push_str(piece);
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Phrases : un segment se termine après `. ! ? 。！？` ou un saut de ligne, et les espaces qui suivent.
fn sentences(text: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut after_end = false;
    for (index, c) in text.char_indices() {
        if after_end && !c.is_whitespace() {
            result.push(&text[start..index]);
            start = index;
            after_end = false;
        }
        if matches!(c, '.' | '!' | '?' | '。' | '！' | '？' | '\n') {
            after_end = true;
        }
    }
    if start < text.len() {
        result.push(&text[start..]);
    }
    result
}

/// Coupe une phrase trop longue, de préférence après une espace, toujours sur une limite de caractère.
fn hard_split(sentence: &str, max_bytes: usize) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut rest = sentence;
    while rest.len() > max_bytes {
        let mut cut = max_bytes;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        if let Some(space) = rest[..cut].rfind(char::is_whitespace) {
            if space > 0 {
                cut = space + rest[space..].chars().next().map_or(1, char::len_utf8);
            }
        }
        if cut == 0 {
            // Un seul caractère plus long que la limite : impossible en pratique (4 octets au plus).
            cut = rest.chars().next().map_or(rest.len(), char::len_utf8);
        }
        pieces.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    if !rest.is_empty() {
        pieces.push(rest);
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_stays_in_one_chunk() {
        assert_eq!(split_chunks("Bonjour. Ça va ?", 450), vec!["Bonjour. Ça va ?"]);
    }

    #[test]
    fn chunks_respect_the_limit_and_never_lose_anything() {
        let text = "Première phrase assez longue. Deuxième phrase tout aussi longue ! Troisième ?\nQuatrième ligne.\n\n   Cinquième après des espaces. ".repeat(20);
        for max in [30, 60, 100, 450] {
            let chunks = split_chunks(&text, max);
            assert!(chunks.iter().all(|c| c.len() <= max), "max {max}");
            assert_eq!(chunks.concat(), text, "max {max}");
        }
    }

    #[test]
    fn sentences_are_kept_together_when_they_fit() {
        let chunks = split_chunks("Une phrase. Une autre phrase. Encore une.", 30);
        assert_eq!(chunks, vec!["Une phrase. Une autre phrase. ", "Encore une."]);
    }

    #[test]
    fn a_very_long_sentence_is_cut_at_spaces() {
        let text = "mot ".repeat(200);
        let chunks = split_chunks(&text, 50);
        assert!(chunks.iter().all(|c| c.len() <= 50));
        assert!(chunks.iter().all(|c| c.ends_with(' ')), "coupé au milieu d'un mot");
        assert_eq!(chunks.concat(), text);
    }

    #[test]
    fn text_without_spaces_and_multibyte_characters_is_cut_on_character_boundaries() {
        let text = "漢字".repeat(300); // 3 octets par caractère, aucune espace
        let chunks = split_chunks(&text, 100);
        assert!(chunks.iter().all(|c| c.len() <= 100));
        assert_eq!(chunks.concat(), text);
        let emoji = "😀".repeat(50);
        assert_eq!(split_chunks(&emoji, 10).concat(), emoji);
    }

    #[test]
    fn cjk_sentence_marks_split_sentences() {
        let chunks = split_chunks("你好。再见。", 9);
        assert_eq!(chunks, vec!["你好。", "再见。"]);
    }

    #[test]
    fn empty_text_gives_no_chunk() {
        assert!(split_chunks("", 100).is_empty());
    }
}
