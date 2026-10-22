//! Motore di elaborazione immagini di Photo Restore.
//!
//! Crate puro (nessuna dipendenza dalla UI): elaborazione tradizionale,
//! rilevamento di fotografie nelle scansioni, ritaglio, correzione prospettica,
//! regolazioni conservative ed esportazione.

pub mod adjust;
pub mod bg;
pub mod detect;
pub mod error;
pub mod io;
pub mod resize;
pub mod restore;
pub mod transform;

pub use error::{PhotoError, Result};

/// Versione del motore di elaborazione.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
