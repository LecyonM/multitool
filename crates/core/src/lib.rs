//! Noyau de Multitool.
//!
//! Ce crate ne dépend d'aucune interface graphique : il définit uniquement le
//! contrat que doit respecter chaque utilitaire ([`Utility`]) et le registre
//! qui les regroupe ([`Registry`]).

mod registry;
mod utility;

pub use registry::{Registry, UtilityDescriptor};
pub use utility::{
    default_downloads_dir, Entry, Field, FieldKind, Grid, Input, Output, ProgressUpdate,
    SelectOption, Utility, UtilityError, UtilityInfo,
};
