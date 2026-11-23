//! Collection des utilitaires de Multitool.
//!
//! Pour ajouter un utilitaire, créez un fichier dans `src/modules/` qui
//! expose `pub fn register(registry: &mut Registry)`. Il est découvert
//! automatiquement à la compilation (voir `build.rs` et le README).

use multitool_core::Registry;

mod modules {
    include!(concat!(env!("OUT_DIR"), "/modules_generated.rs"));
}

/// Construit le registre contenant tous les utilitaires.
pub fn registry() -> Registry {
    let mut registry = Registry::new();
    modules::register_all(&mut registry);
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn at_least_one_utility_is_registered() {
        assert!(!registry().is_empty());
    }

    #[test]
    fn every_utility_declares_valid_metadata() {
        for descriptor in registry().descriptors() {
            let id = &descriptor.info.id;
            assert!(
                !id.is_empty()
                    && id
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "identifiant invalide : « {id} » (minuscules, chiffres et _)"
            );
            assert!(!descriptor.info.name.trim().is_empty(), "{id} : nom vide");
            assert!(
                !descriptor.info.category.trim().is_empty(),
                "{id} : catégorie vide"
            );

            let mut seen = HashSet::new();
            for field in &descriptor.fields {
                assert!(
                    seen.insert(field.id.clone()),
                    "{id} : champ « {} » déclaré deux fois",
                    field.id
                );
            }
        }
    }
}
