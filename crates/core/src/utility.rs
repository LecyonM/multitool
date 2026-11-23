use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Informations affichées dans la liste des utilitaires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UtilityInfo {
    /// Identifiant unique et stable (ex. `text_stats`).
    pub id: String,
    pub name: String,
    pub description: String,
    /// Sert à regrouper les utilitaires dans la barre latérale.
    pub category: String,
}

impl UtilityInfo {
    pub fn new(id: &str, name: &str, description: &str, category: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            category: category.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

/// Type d'un champ de saisie ; l'interface génère le bon composant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    TextArea,
    Number {
        min: Option<f64>,
        max: Option<f64>,
    },
    Select {
        options: Vec<SelectOption>,
    },
    Checkbox,
    /// Chemin d'un dossier : l'interface ajoute un bouton « Parcourir ».
    Folder,
}

/// Description d'un champ de saisie d'un utilitaire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    pub label: String,
    pub kind: FieldKind,
    pub placeholder: Option<String>,
    pub default: Option<Value>,
    pub required: bool,
}

impl Field {
    fn new(id: &str, label: &str, kind: FieldKind) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind,
            placeholder: None,
            default: None,
            required: true,
        }
    }

    pub fn text(id: &str, label: &str) -> Self {
        Self::new(id, label, FieldKind::Text)
    }

    pub fn textarea(id: &str, label: &str) -> Self {
        Self::new(id, label, FieldKind::TextArea)
    }

    pub fn number(id: &str, label: &str) -> Self {
        Self::new(
            id,
            label,
            FieldKind::Number {
                min: None,
                max: None,
            },
        )
    }

    /// `options` : liste de couples `(valeur, libellé affiché)`.
    pub fn select(id: &str, label: &str, options: &[(&str, &str)]) -> Self {
        let options = options
            .iter()
            .map(|(value, label)| SelectOption {
                value: (*value).into(),
                label: (*label).into(),
            })
            .collect();
        Self::new(id, label, FieldKind::Select { options })
    }

    pub fn checkbox(id: &str, label: &str) -> Self {
        let mut field = Self::new(id, label, FieldKind::Checkbox);
        field.required = false;
        field
    }

    /// Champ de choix d'un dossier, prérempli avec le dossier Téléchargements.
    pub fn folder(id: &str, label: &str) -> Self {
        let mut field = Self::new(id, label, FieldKind::Folder);
        field.default = default_downloads_dir().map(Value::String);
        field
    }

    pub fn placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn default_value(mut self, value: Value) -> Self {
        self.default = Some(value);
        self
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    /// Bornes d'un champ numérique (sans effet sur les autres types).
    pub fn range(mut self, min: f64, max: f64) -> Self {
        if let FieldKind::Number { min: lo, max: hi } = &mut self.kind {
            *lo = Some(min);
            *hi = Some(max);
        }
        self
    }
}

/// Dossier Téléchargements de l'utilisateur (`~/Downloads`, présent sur Windows,
/// macOS et la plupart des Linux). `None` si le dossier personnel est introuvable.
pub fn default_downloads_dir() -> Option<String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .filter(|home| !home.is_empty())?;
    Some(
        std::path::Path::new(&home)
            .join("Downloads")
            .to_string_lossy()
            .into_owned(),
    )
}

/// Valeurs saisies par l'utilisateur, indexées par identifiant de champ.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Input(pub HashMap<String, Value>);

impl Input {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construit une saisie à partir de couples clé / valeur (pratique pour les tests).
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.0.insert(key.into(), value.into());
        self
    }

    /// Texte saisi, chaîne vide si absent.
    pub fn string(&self, key: &str) -> String {
        self.0
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    /// Nombre saisi (accepte aussi une chaîne numérique).
    pub fn number(&self, key: &str) -> Option<f64> {
        match self.0.get(key)? {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().replace(',', ".").parse().ok(),
            _ => None,
        }
    }

    pub fn bool(&self, key: &str) -> bool {
        self.0.get(key).and_then(Value::as_bool).unwrap_or(false)
    }
}

/// Ligne « libellé : valeur » d'un résultat tabulaire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub label: String,
    pub value: String,
}

/// Tableau à colonnes (en-têtes et lignes), pour les résultats longs comme une liste de machines.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grid {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Grid {
    pub fn new(columns: &[&str]) -> Self {
        Self {
            columns: columns.iter().map(|c| (*c).to_string()).collect(),
            rows: Vec::new(),
        }
    }

    /// Ajoute une ligne. Les cellules en trop sont ignorées et les cellules
    /// manquantes complétées par du vide : le tableau reste toujours rectangulaire.
    pub fn push(&mut self, cells: Vec<String>) {
        let mut cells = cells;
        cells.resize(self.columns.len(), String::new());
        self.rows.push(cells);
    }
}

/// Résultat d'un utilitaire : du texte, un récapitulatif libellé / valeur,
/// un tableau à colonnes, ou plusieurs de ces éléments.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    pub text: String,
    pub table: Vec<Entry>,
    /// Absent des anciens utilitaires : la valeur par défaut garde la compatibilité.
    #[serde(default)]
    pub grid: Option<Grid>,
}

impl Output {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn table<L, V>(rows: Vec<(L, V)>) -> Self
    where
        L: Into<String>,
        V: Into<String>,
    {
        Self {
            table: rows
                .into_iter()
                .map(|(label, value)| Entry {
                    label: label.into(),
                    value: value.into(),
                })
                .collect(),
            ..Self::default()
        }
    }

    /// Ajoute un tableau à colonnes sous le récapitulatif.
    pub fn with_grid(mut self, grid: Grid) -> Self {
        self.grid = Some(grid);
        self
    }

    /// Ajoute (ou remplace) le texte affiché sous le tableau.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UtilityError {
    /// La saisie est incorrecte : le message est affiché tel quel à l'utilisateur.
    InvalidInput(String),
    /// L'utilitaire a échoué pendant son exécution.
    Failed(String),
    /// Aucun utilitaire ne porte cet identifiant.
    NotFound(String),
}

impl fmt::Display for UtilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(msg) | Self::Failed(msg) => write!(f, "{msg}"),
            Self::NotFound(id) => write!(f, "Utilitaire introuvable : « {id} »."),
        }
    }
}

impl std::error::Error for UtilityError {}

/// Avancement d'un traitement long, affiché par l'interface sous forme de barre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressUpdate {
    /// Avancement entre 0.0 et 1.0, ou `None` si la durée est inconnue
    /// (l'interface affiche alors une barre « en attente »).
    pub fraction: Option<f64>,
    /// Texte affiché sous la barre (ex. « 12,4 Mo sur 48 Mo, 3,2 Mo/s »).
    pub message: String,
}

impl ProgressUpdate {
    /// Avancement connu ; `fraction` est ramenée entre 0 et 1.
    pub fn new(fraction: f64, message: impl Into<String>) -> Self {
        Self {
            fraction: Some(fraction.clamp(0.0, 1.0)),
            message: message.into(),
        }
    }

    /// Avancement inconnu : seul le message est mis à jour.
    pub fn indeterminate(message: impl Into<String>) -> Self {
        Self {
            fraction: None,
            message: message.into(),
        }
    }
}

/// Contrat à respecter pour ajouter un utilitaire à Multitool.
pub trait Utility: Send + Sync {
    fn info(&self) -> UtilityInfo;
    /// Champs de saisie : l'interface se construit automatiquement à partir d'eux.
    fn fields(&self) -> Vec<Field>;
    fn run(&self, input: &Input) -> Result<Output, UtilityError>;

    /// Variante de [`run`](Utility::run) qui signale son avancement.
    ///
    /// À surcharger pour les traitements longs ; par défaut, l'utilitaire
    /// s'exécute sans barre de progression.
    fn run_with_progress(
        &self,
        input: &Input,
        _progress: &dyn Fn(ProgressUpdate),
    ) -> Result<Output, UtilityError> {
        self.run(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grid_stays_rectangular() {
        let mut grid = Grid::new(&["A", "B", "C"]);
        grid.push(vec!["1".into()]);
        grid.push(vec!["1".into(), "2".into(), "3".into(), "4".into()]);
        assert_eq!(grid.rows[0], vec!["1", "", ""]);
        assert_eq!(grid.rows[1], vec!["1", "2", "3"]);
    }

    #[test]
    fn output_builders_combine_table_grid_and_text() {
        let output = Output::table(vec![("Total", "2")])
            .with_grid(Grid::new(&["Port"]))
            .with_text("Remarque");
        assert_eq!(output.table.len(), 1);
        assert_eq!(output.grid.as_ref().unwrap().columns, vec!["Port"]);
        assert_eq!(output.text, "Remarque");
    }

    #[test]
    fn an_old_output_without_grid_still_deserializes() {
        let output: Output = serde_json::from_str(r#"{"text":"ok","table":[]}"#).unwrap();
        assert_eq!(output.grid, None);
    }
}
