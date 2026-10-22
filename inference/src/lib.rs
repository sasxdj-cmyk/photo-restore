// Inferenza locale per Photo Restore: carica i modelli ONNX scaricati
// dall'utente, applica la tiling e restituisce l'immagine potenziata.
pub mod download;
pub mod engine;
pub mod error;
pub mod faces;
pub mod mgmt;
pub mod models;
pub mod seg;

pub use error::{InferenceError, Result};
pub use models::ModelInfo;
