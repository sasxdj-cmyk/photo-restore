//! Errori tipizzati del motore di elaborazione.

use std::fmt;
use std::io;

/// Errori possibili durante l'elaborazione.
#[derive(Debug)]
pub enum PhotoError {
    /// Errore di accesso ai file.
    Io(io::Error),
    /// Errore di decodifica/encoding immagine.
    Image(image::ImageError),
    /// Parametri di input non validi.
    Input(String),
    /// Errore durante la scrittura del file di destinazione.
    Output(String),
}

impl fmt::Display for PhotoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhotoError::Io(e) => write!(f, "errore di I/O: {e}"),
            PhotoError::Image(e) => write!(f, "errore immagine: {e}"),
            PhotoError::Input(msg) => write!(f, "parametro non valido: {msg}"),
            PhotoError::Output(msg) => write!(f, "errore di destinazione: {msg}"),
        }
    }
}

impl std::error::Error for PhotoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PhotoError::Io(e) => Some(e),
            PhotoError::Image(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for PhotoError {
    fn from(e: io::Error) -> Self {
        PhotoError::Io(e)
    }
}

impl From<image::ImageError> for PhotoError {
    fn from(e: image::ImageError) -> Self {
        PhotoError::Image(e)
    }
}

/// Risultato tipizzato del motore.
pub type Result<T, E = PhotoError> = std::result::Result<T, E>;
