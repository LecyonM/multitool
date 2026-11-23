# Multitool : consignes de travail

- À **chaque nouvelle tâche ou modification du code**, mettre à jour `README.md` :
  ajouter une entrée datée (la plus récente en premier) dans « Journal des modifications »
  et corriger les sections concernées (structure, utilitaires disponibles, etc.).
- Ajouter un utilitaire = créer un fichier dans `crates/utilities/src/modules/` (voir le README). Rien d'autre à modifier.
- Interface et README en français.
- Avant de livrer : `cargo test`, `cargo fmt --all --check` et `cargo clippy --workspace --all-targets`.
