use serde::{Deserialize, Serialize};

use crate::utility::{Field, Input, Output, ProgressUpdate, Utility, UtilityError, UtilityInfo};

/// Ce que l'interface reçoit pour chaque utilitaire : ses infos et ses champs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UtilityDescriptor {
    #[serde(flatten)]
    pub info: UtilityInfo,
    pub fields: Vec<Field>,
}

/// Regroupe tous les utilitaires disponibles.
#[derive(Default)]
pub struct Registry {
    utilities: Vec<Box<dyn Utility>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute un utilitaire.
    ///
    /// # Panics
    /// Si l'identifiant est déjà pris : c'est une erreur de programmation,
    /// détectée par les tests du crate `multitool-utilities`.
    pub fn register(&mut self, utility: Box<dyn Utility>) -> &mut Self {
        let id = utility.info().id;
        assert!(
            self.get(&id).is_none(),
            "Identifiant d'utilitaire en double : « {id} »"
        );
        self.utilities.push(utility);
        self
    }

    pub fn get(&self, id: &str) -> Option<&dyn Utility> {
        self.utilities
            .iter()
            .find(|utility| utility.info().id == id)
            .map(|utility| utility.as_ref())
    }

    pub fn len(&self) -> usize {
        self.utilities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.utilities.is_empty()
    }

    /// Liste triée par catégorie puis par nom.
    pub fn descriptors(&self) -> Vec<UtilityDescriptor> {
        let mut list: Vec<UtilityDescriptor> = self
            .utilities
            .iter()
            .map(|utility| UtilityDescriptor {
                info: utility.info(),
                fields: utility.fields(),
            })
            .collect();
        list.sort_by(|a, b| {
            (&a.info.category, &a.info.name).cmp(&(&b.info.category, &b.info.name))
        });
        list
    }

    pub fn run(&self, id: &str, input: &Input) -> Result<Output, UtilityError> {
        self.get(id)
            .ok_or_else(|| UtilityError::NotFound(id.into()))?
            .run(input)
    }

    /// Comme [`run`](Registry::run), en transmettant l'avancement à `progress`.
    pub fn run_with_progress(
        &self,
        id: &str,
        input: &Input,
        progress: &dyn Fn(ProgressUpdate),
    ) -> Result<Output, UtilityError> {
        self.get(id)
            .ok_or_else(|| UtilityError::NotFound(id.into()))?
            .run_with_progress(input, progress)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo(&'static str);

    impl Utility for Echo {
        fn info(&self) -> UtilityInfo {
            UtilityInfo::new(self.0, "Écho", "Renvoie le texte saisi", "Test")
        }
        fn fields(&self) -> Vec<Field> {
            vec![Field::text("text", "Texte")]
        }
        fn run(&self, input: &Input) -> Result<Output, UtilityError> {
            Ok(Output::text(input.string("text")))
        }
    }

    #[test]
    fn runs_a_registered_utility() {
        let mut registry = Registry::new();
        registry.register(Box::new(Echo("echo")));
        let out = registry
            .run("echo", &Input::new().with("text", "salut"))
            .unwrap();
        assert_eq!(out.text, "salut");
    }

    #[test]
    fn unknown_id_is_reported() {
        let registry = Registry::new();
        let err = registry.run("nope", &Input::new()).unwrap_err();
        assert_eq!(err, UtilityError::NotFound("nope".into()));
    }

    #[test]
    #[should_panic(expected = "en double")]
    fn duplicate_id_panics() {
        let mut registry = Registry::new();
        registry.register(Box::new(Echo("echo")));
        registry.register(Box::new(Echo("echo")));
    }

    /// Utilitaire qui signale deux étapes de progression avant de répondre.
    struct Slow;

    impl Utility for Slow {
        fn info(&self) -> UtilityInfo {
            UtilityInfo::new("slow", "Lent", "Signale sa progression", "Test")
        }
        fn fields(&self) -> Vec<Field> {
            vec![Field::folder("dest", "Dossier")]
        }
        fn run(&self, _input: &Input) -> Result<Output, UtilityError> {
            Ok(Output::text("sans progression"))
        }
        fn run_with_progress(
            &self,
            _input: &Input,
            progress: &dyn Fn(ProgressUpdate),
        ) -> Result<Output, UtilityError> {
            progress(ProgressUpdate::new(0.5, "moitié"));
            progress(ProgressUpdate::new(1.7, "fini"));
            Ok(Output::text("avec progression"))
        }
    }

    #[test]
    fn progress_updates_reach_the_caller() {
        use std::cell::RefCell;

        let mut registry = Registry::new();
        registry.register(Box::new(Slow));
        let seen = RefCell::new(Vec::new());
        let out = registry
            .run_with_progress("slow", &Input::new(), &|update| {
                seen.borrow_mut().push(update)
            })
            .unwrap();

        assert_eq!(out.text, "avec progression");
        let seen = seen.into_inner();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].fraction, Some(0.5));
        // Une fraction hors bornes est ramenée à 1.
        assert_eq!(seen[1].fraction, Some(1.0));
    }

    #[test]
    fn utilities_without_progress_still_run_through_the_progress_entry_point() {
        use std::cell::Cell;

        let mut registry = Registry::new();
        registry.register(Box::new(Echo("echo")));
        let calls = Cell::new(0);
        let out = registry
            .run_with_progress("echo", &Input::new().with("text", "ok"), &|_| {
                calls.set(calls.get() + 1)
            })
            .unwrap();
        assert_eq!(out.text, "ok");
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn indeterminate_progress_has_no_fraction() {
        assert_eq!(ProgressUpdate::indeterminate("attente").fraction, None);
    }

    #[test]
    fn folder_field_is_prefilled_with_the_downloads_directory() {
        let descriptors = {
            let mut registry = Registry::new();
            registry.register(Box::new(Slow));
            registry.descriptors()
        };
        let field = &descriptors[0].fields[0];
        let json = serde_json::to_value(field).unwrap();
        assert_eq!(json["kind"]["type"], "folder");
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            let default = field.default.as_ref().and_then(|v| v.as_str()).unwrap();
            assert!(default.starts_with(home.to_str().unwrap()));
            assert!(default.ends_with("Downloads"));
        }
    }

    #[test]
    fn descriptors_serialize_flat_for_the_ui() {
        let mut registry = Registry::new();
        registry.register(Box::new(Echo("echo")));
        let json = serde_json::to_value(registry.descriptors()).unwrap();
        assert_eq!(json[0]["id"], "echo");
        assert_eq!(json[0]["fields"][0]["kind"]["type"], "text");
    }
}
