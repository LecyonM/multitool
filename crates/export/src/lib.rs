//! Export d'un résultat Multitool en PDF (A4) : titre, récapitulatif, tableau à colonnes et texte.
//!
//! Le PDF contient sa propre police (DejaVu Sans, libre) : il s'affiche à l'identique partout.
//! DejaVu couvre le latin (accents français compris), le grec, le cyrillique et bien d'autres écritures
//! mais ni l'arabe, ni le chinois, le japonais, le coréen ou les écritures indiennes : ces caractères
//! sont remplacés par « ? » et le nombre de remplacements est renvoyé pour être signalé à l'utilisateur.

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

/// Fabrique le PDF. `generated_at` est la date affichée sous le titre (déjà mise en forme).
pub fn render_pdf(title: &str, generated_at: &str, output: &Output) -> Result<(Vec<u8>, Report), String> {
    let face = ttf_parser::Face::parse(REGULAR, 0).map_err(|e| format!("police illisible : {e}"))?;
    let mut replaced = 0;
    let mut clean = |text: &str| {
        let (clean, count) = sanitize(text, |c| face.glyph_index(c).is_some());
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
    let grid = output.grid.as_ref().map(|grid| {
        let mut copy = Grid::default();
        copy.columns = grid.columns.iter().map(|c| clean(c)).collect();
        copy.rows = grid
            .rows
            .iter()
            .map(|row| row.iter().map(|c| clean(c)).collect())
            .collect();
        copy
    });
    let text = clean(&output.text);

    let font = |data: &[u8]| FontData::new(data.to_vec(), None).map_err(|e| format!("police : {e}"));
    let family = FontFamily {
        regular: font(REGULAR)?,
        bold: font(BOLD)?,
        italic: font(REGULAR)?,
        bold_italic: font(BOLD)?,
    };
    let mut document = Document::new(family);
    document.set_title(title.clone());
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
    document.push(Paragraph::new(format!("Généré le {generated_at}")).styled(Style::new().with_font_size(9)));
    document.push(Break::new(1.2));

    if !summary.is_empty() {
        let mut table = TableLayout::new(vec![2, 5]);
        for (label, value) in &summary {
            table
                .row()
                .element(Paragraph::new(label.as_str()).styled(Style::new().bold()).padded(Margins::vh(0.8, 1)))
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
    document.render(&mut bytes).map_err(|e| format!("création du PDF : {e}"))?;
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
        assert_eq!(sanitize("a\tb\r\nc\u{7}\u{200b}", only_ascii), ("a    b\nc".to_string(), 0));
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
}
