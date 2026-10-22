use thiserror::Error;

#[derive(Debug, Error)]
pub enum InferenceError {
    #[error("modello non trovato: {0}")]
    MissingModel(String),
    #[error("attestazione del file errata (SHA-256 previsto {0})")]
    HashMismatch(String),
    #[error("inferenza: {0}")]
    Ort(#[from] ort::Error),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("dimensioni letterali non valide: {0}")]
    InvalidSize(String),
    #[error("modello: {0}")]
    Model(String),
}

pub type Result<T> = std::result::Result<T, InferenceError>;
