//! Export d'un résultat Multitool en PDF (A4) : titre, récapitulatif, tableau à colonnes et texte.
//!
//! Le PDF contient sa propre police (DejaVu Sans, libre) : il s'affiche à l'identique partout.
//! DejaVu couvre le latin (accents français compris), le grec, le cyrillique, l'arménien, le géorgien…
//! Ce moteur n'assemble pas les lettres : les écritures qui l'exigent (arabe, hébreu, langues
//! indiennes, thaï, khmer, birman, de droite à gauche ou à lettres liées) seraient écrites faussement,
//! même quand la police a leurs glyphes. Elles, comme le chinois, le japonais et le coréen, sont
//! donc considérées comme non gérées : `count_unsupported` les compte (l'appelant choisit alors une
//! autre voie) et `render_pdf` les remplace par « ? » en le signalant dans `Report`.

use genpdf::elements::{Break, FrameCellDecorator, LinearLayout, Paragraph, TableLayout};
use genpdf::fonts::{FontData, FontFamily};
use genpdf::style::Style;
use genpdf::{Alignment, Document, Element, Margins, SimplePageDecorator};
use multitool_core::{Grid, Output};

const REGULAR: &[u8] = include_bytes!("../fonts/DejaVuSans.ttf");
const BOLD: &[u8] = include_bytes!("../fonts/DejaVuSans-Bold.ttf");

/// Ce que l'appelant doit savoir sur le document produit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    /// Caractères absents de la police, remplacés par « ? ».
    pub replaced: usize,
}

/// Écritures qui demandent un assemblage des lettres (liaisons, voyelles combinées, sens de lecture
/// de droite à gauche) : ce moteur ne le fait pas, il ne faut donc pas les dessiner lettre à lettre.
fn needs_shaping(c: char) -> bool {
    matches!(c as u32,
        0x0590..=0x0EFF   // hébreu, arabe, syriaque, thaana, n'ko, langues indiennes, thaï, lao
        | 0x0F00..=0x109F // tibétain, birman
        | 0x1780..=0x17FF // khmer
        | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF // formes de présentation hébreu / arabe
    )
}

/// Vrai si le caractère peut être dessiné fidèlement : présent dans la police et sans assemblage.
fn can_draw(face: &ttf_parser::Face<'_>, c: char) -> bool {
    !needs_shaping(c) && face.glyph_index(c).is_some()
}

/// Nombre de caractères que la police intégrée ne sait pas dessiner (arabe, chinois, hindi…).
/// À 0, le PDF produit est fidèle ; sinon l'appelant peut préférer une autre voie (impression).
pub fn count_unsupported(title: &str, output: &Output) -> usize {
    let Ok(face) = ttf_parser::Face::parse(REGULAR, 0) else {
        return usize::MAX;
    };
    let count = |text: &str| sanitize(text, |c| can_draw(&face, c)).1;
    let mut total = count(title) + count(&output.text);
    for entry in &output.table {
        total += count(&entry.label) + count(&entry.value);
    }
    if let Some(grid) = &output.grid {
        total += grid.columns.iter().map(|c| count(c)).sum::<usize>();
        total += grid
            .rows
            .iter()
            .flatten()
            .map(|cell| count(cell))
            .sum::<usize>();
    }
    total
}

/// Fabrique le PDF. `generated_at` est la date affichée sous le titre (déjà mise en forme).
pub fn render_pdf(
    title: &str,
    generated_at: &str,
    output: &Output,
) -> Result<(Vec<u8>, Report), String> {
    let face =
        ttf_parser::Face::parse(REGULAR, 0).map_err(|e| format!("police illisible : {e}"))?;
    let mut replaced = 0;
    let mut clean = |text: &str| {
        let (clean, count) = sanitize(text, |c| can_draw(&face, c));
        replaced += count;
        clean
    };

    let title = clean(title);
    let generated_at = clean(generated_at);
    let summary: Vec<(String, String)> = output
        .table
        .iter()
        .map(|entry| (clean(&entry.label), clean(&entry.value)))
        .collect();
    let grid = output.grid.as_ref().map(|grid| Grid {
        columns: grid.columns.iter().map(|c| clean(c)).collect(),
        rows: grid
            .rows
            .iter()
            .map(|row| row.iter().map(|c| clean(c)).collect())
            .collect(),
    });
    let text = clean(&output.text);

    let font =
        |data: &[u8]| FontData::new(data.to_vec(), None).map_err(|e| format!("police : {e}"));
    let family = FontFamily {
        regular: font(REGULAR)?,
        bold: font(BOLD)?,
        italic: font(REGULAR)?,
        bold_italic: font(BOLD)?,
    };
    let mut document = Document::new(family);
    // Les métadonnées PDF n'acceptent que de l'ASCII avec ce moteur : un accent y deviendrait « Ã© ».
    document.set_title(ascii_fold(&title));
    document.set_font_size(10);
    let mut decorator = SimplePageDecorator::new();
    decorator.set_margins(Margins::trbl(18, 16, 16, 16));
    decorator.set_header(|page| {
        let mut header = LinearLayout::vertical();
        header.push(
            Paragraph::new(format!("Multitool  -  page {page}"))
                .aligned(Alignment::Right)
                .styled(Style::new().with_font_size(8)),
        );
        header.push(Break::new(0.6));
        header
    });
    document.set_page_decorator(decorator);

    document.push(Paragraph::new(title).styled(Style::new().bold().with_font_size(18)));
    document.push(
        Paragraph::new(format!("Généré le {generated_at}")).styled(Style::new().with_font_size(9)),
    );
    document.push(Break::new(1.2));

    if !summary.is_empty() {
        let mut table = TableLayout::new(vec![2, 5]);
        for (label, value) in &summary {
            table
                .row()
                .element(
                    Paragraph::new(label.as_str())
                        .styled(Style::new().bold())
                        .padded(Margins::vh(0.8, 1)),
                )
                .element(Paragraph::new(value.as_str()).padded(Margins::vh(0.8, 1)))
                .push()
                .map_err(|e| format!("tableau : {e}"))?;
        }
        document.push(table);
        document.push(Break::new(1.2));
    }

    if let Some(grid) = grid.filter(|g| !g.columns.is_empty()) {
        document.push(grid_table(&grid)?);
        document.push(Break::new(1.2));
    }

    for line in text.lines() {
        if line.trim().is_empty() {
            document.push(Break::new(0.8));
        } else {
            document.push(Paragraph::new(line));
        }
    }

    let mut bytes = Vec::new();
    document
        .render(&mut bytes)
        .map_err(|e| format!("création du PDF : {e}"))?;
    Ok((bytes, Report { replaced }))
}

/// Tableau à colonnes : largeurs proportionnelles au contenu (bornées), cadre fin, en-tête en gras.
fn grid_table(grid: &Grid) -> Result<TableLayout, String> {
    let weights: Vec<usize> = (0..grid.columns.len())
        .map(|column| {
            let longest = grid
                .rows
                .iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .chain(std::iter::once(grid.columns[column].chars().count()))
                .max()
                .unwrap_or(1);
            longest.clamp(4, 40)
        })
        .collect();
    let mut table = TableLayout::new(weights);
    table.set_cell_decorator(FrameCellDecorator::new(true, true, false));
    let mut header = table.row();
    for column in &grid.columns {
        header = header.element(
            Paragraph::new(column.as_str())
                .styled(Style::new().bold())
                .padded(Margins::vh(1, 1.5)),
        );
    }
    header.push().map_err(|e| format!("tableau : {e}"))?;
    for row in &grid.rows {
        let mut line = table.row();
        for cell in row {
            line = line.element(Paragraph::new(cell.as_str()).padded(Margins::vh(0.8, 1.5)));
        }
        line.push().map_err(|e| format!("tableau : {e}"))?;
    }
    Ok(table)
}

/// Texte sans accents pour les métadonnées du PDF (« réseau » → « reseau »).
fn ascii_fold(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
            'À' | 'Â' | 'Ä' | 'Á' | 'Ã' | 'Å' => 'A',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'Î' | 'Ï' | 'Í' | 'Ì' => 'I',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'Ô' | 'Ö' | 'Ó' | 'Ò' | 'Õ' => 'O',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'Ù' | 'Û' | 'Ü' | 'Ú' => 'U',
            'ç' => 'c',
            'Ç' => 'C',
            'ñ' => 'n',
            'ÿ' => 'y',
            c if c.is_ascii() => c,
            _ => '?',
        })
        .collect()
}

/// Retire les caractères de contrôle, remplace les tabulations par des espaces et les caractères
/// absents de la police par « ? ». Renvoie le texte nettoyé et le nombre de remplacements.
fn sanitize(text: &str, has_glyph: impl Fn(char) -> bool) -> (String, usize) {
    let mut clean = String::with_capacity(text.len());
    let mut replaced = 0;
    for c in text.chars() {
        match c {
            '\n' => clean.push('\n'),
            '\t' => clean.push_str("    "),
            '\r' => {}
            c if c.is_control() => {}
            c if has_glyph(c) => clean.push(c),
            // Joints sans largeur : invisibles, inutile de les signaler.
            '\u{200b}'..='\u{200f}' | '\u{2060}' | '\u{feff}' => {}
            _ => {
                clean.push('?');
                replaced += 1;
            }
        }
    }
    (clean, replaced)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_what_the_font_cannot_draw() {
        let only_ascii = |c: char| c.is_ascii();
        assert_eq!(sanitize("abc é 漢", only_ascii), ("abc ? ?".to_string(), 2));
        assert_eq!(
            sanitize("a\tb\r\nc\u{7}\u{200b}", only_ascii),
            ("a    b\nc".to_string(), 0)
        );
    }

    #[test]
    fn the_bundled_font_covers_french_greek_and_cyrillic_but_not_cjk() {
        let face = ttf_parser::Face::parse(REGULAR, 0).unwrap();
        let covered = |text: &str| text.chars().all(|c| face.glyph_index(c).is_some());
        assert!(covered("àâçéèêëîïôùûüÿœæ ÀÉÈÇŒ « » ’ … € → ★ ☺"));
        assert!(covered("Привет мир Γειά σου κόσμε"));
        assert!(!covered("漢字"));
    }

    #[test]
    fn an_empty_result_still_gives_a_valid_pdf() {
        let (bytes, report) = render_pdf("Vide", "24/11/2026", &Output::default()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert_eq!(report.replaced, 0);
    }

    #[test]
    fn metadata_titles_are_plain_ascii() {
        assert_eq!(
            ascii_fold("Multitool : Scanner de réseau"),
            "Multitool : Scanner de reseau"
        );
        assert_eq!(ascii_fold("Ça ÉCRIT à l'œil"), "Ca ECRIT a l'?il");
    }

    #[test]
    fn coverage_counts_only_what_the_font_cannot_draw() {
        let french = Output::table(vec![("Durée", "12,9 s"), ("Café", "été à Zürich Ω Я")])
            .with_text("Texte accentué : œ € « »");
        assert_eq!(
            count_unsupported("Multitool : Scanner de réseau", &french),
            0
        );
        // L'arabe a des glyphes dans la police mais exige l'assemblage des lettres : non gérée.
        let arabic = Output::text("مرحبا 你好");
        assert_eq!(count_unsupported("Traducteur", &arabic), 7);
        for text in [
            "שלום",
            "नमस्ते",
            "สวัสดี",
            "ជំរាបសួរ",
            "မင်္ဂလာပါ",
            "こんにちは",
            "안녕",
        ] {
            assert!(count_unsupported("x", &Output::text(text)) > 0, "{text}");
        }
        for text in [
            "Привет",
            "Γειά σου",
            "Բարեւ",
            "გამარჯობა",
            "Zażółć gęślą jaźń",
            "Ünïcödé ŝ ǧ",
        ] {
            assert_eq!(count_unsupported("x", &Output::text(text)), 0, "{text}");
        }
        let mut grid = Grid::new(&["IP", "Nom"]);
        grid.push(vec!["10.0.0.1".into(), "漢".into()]);
        assert_eq!(
            count_unsupported("x", &Output::default().with_grid(grid)),
            1
        );
    }
}
