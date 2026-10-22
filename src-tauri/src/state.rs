//! Stato della sessione di lavoro.
//!
//! Elaborazione non distruttiva: l'immagine originale resta separata dal
//! buffer di lavoro e ogni comando salva nello storico il contenuto
//! precedente, consentendo undo/redo limitati da un tetto di memoria.

use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use image::RgbaImage;
use serde::Serialize;

/// Tetto di memoria dello storico in megapixel (wa ogni snapshot).
const HISTORY_MAX_MEGAPIXELS: u64 = 200;
/// Numero massimo di snapshot nello storico.
const HISTORY_MAX_ENTRIES: usize = 5;

#[derive(Debug)]
pub struct Session {
    pub original: RgbaImage,
    pub original_path: Option<PathBuf>,
    pub original_file_size: Option<u64>,
    pub current: RgbaImage,
    pub undo: Vec<RgbaImage>,
    pub redo: Vec<RgbaImage>,
    /// Descrizione dell'ultima operazione (in italiano).
    pub method: String,
    /// Cartella dei file di anteprima della sessione.
    pub out_dir: PathBuf,
    /// Immagine di riferimento della fusione: quella di prima del restauro
    /// volti, oppure l'ingrandimento normale (Lanczos) per l'upscaling.
    pub fusione_base: Option<RgbaImage>,
    /// Risultato AI pieno, di cui la barra di visibilita' dosa la presenza.
    pub fusione_ai: Option<RgbaImage>,
    /// Quota della correzione attualmente in `current` (0..1).
    pub fusione: f32,
    /// Storico gia' spintato per il risultato AI corrente (una volta sola).
    pub fusione_spinta: bool,
    /// Quota applicata prima dell'ultimo annullamento (per il redo).
    pub fusione_p_precedente: f32,
}

impl Session {
    pub fn new(original: RgbaImage, original_path: Option<PathBuf>) -> Self {
        let out_dir = default_out_dir();
        let original_file_size = original_path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len());
        Session {
            original: original.clone(),
            original_path,
            original_file_size,
            current: original,
            undo: Vec::new(),
            redo: Vec::new(),
            method: "Importazione".to_string(),
            out_dir,
            fusione_base: None,
            fusione_ai: None,
            fusione: 1.0,
            fusione_spinta: false,
            fusione_p_precedente: 1.0,
        }
    }

    /// Aggiunge lo snapshot corrente allo storico, senza toccare la fusione.
    pub fn registra_storico(&mut self) {
        self.undo.push(self.current.clone());
        if self.total_history_mp() > HISTORY_MAX_MEGAPIXELS || self.undo.len() > HISTORY_MAX_ENTRIES
        {
            while self.total_history_mp() > HISTORY_MAX_MEGAPIXELS && self.undo.len() > 1 {
                self.undo.remove(0);
            }
            if self.undo.len() > HISTORY_MAX_ENTRIES {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
    }

    /// Aggiunge lo snapshot corrente allo storico prima di una nuova modifica;
    /// ogni modifica dati la fusione (la nuova operazione cambia la base).
    pub fn push_history(&mut self) {
        self.registra_storico();
        self.fusione_base = None;
        self.fusione_ai = None;
        self.fusione = 1.0;
        self.fusione_spinta = false;
        self.fusione_p_precedente = 1.0;
    }

    fn mp_of(img: &RgbaImage) -> u64 {
        (img.width() as u64) * (img.height() as u64) / 1_000_000
    }

    fn total_history_mp(&self) -> u64 {
        self.undo.iter().map(Self::mp_of).sum()
    }

    /// Scrive l'anteprima corrente su disco (PNG) e ne restituisce il percorso.
    ///
    /// Per reagire rapidamente l'immagine mostrata viene ridotta a un massimo di
    /// 1600 px sul lato lungo: il webview paga la resa, i bordi si composeno
    /// poi con la risoluzione corrente riportata dallo stato.
    pub fn save_preview(&self) -> std::io::Result<String> {
        write_thumbnail(&self.current, &self.out_dir.join("current.png"))
    }

    /// Scrive l'originale per il confronto prima/dopo, ridotto anch'esso.
    pub fn save_original_preview(&self) -> std::io::Result<String> {
        write_thumbnail(&self.original, &self.out_dir.join("original.png"))
    }
}

/// Massimo lato di un'anteprima visualizzata.
const PREVIEW_MAX_SIDE: u32 = 1600;

/// Salva `img` ridotta al fondo massimo PREVIEW_MAX_SIDE.
fn write_thumbnail(img: &RgbaImage, path: &PathBuf) -> std::io::Result<String> {
    std::fs::create_dir_all(
        path.parent()
            .ok_or_else(|| std::io::Error::other("anteprima senza cartella"))?,
    )?;
    let bytes = thumbnail_png_bytes(img)?;
    std::fs::write(path, &bytes)?;
    Ok(path.to_string_lossy().to_string())
}

/// Codifica una riduzione PNG in memoria (max PREVIEW_MAX_SIDE px sul lato lungo).
pub fn thumbnail_png_bytes(img: &RgbaImage) -> std::io::Result<Vec<u8>> {
    let (w, h) = img.dimensions();
    let long = w.max(h) as f32;
    let to_save: RgbaImage = if long > PREVIEW_MAX_SIDE as f32 {
        let s = PREVIEW_MAX_SIDE as f32 / long;
        image::imageops::resize(
            img,
            ((w as f32 * s).round() as u32).max(1),
            ((h as f32 * s).round() as u32).max(1),
            image::imageops::FilterType::Triangle,
        )
    } else {
        img.to_owned()
    };
    let mut out = std::io::Cursor::new(Vec::new());
    to_save
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(out.into_inner())
}

/// Cartella predefinita per i file temporanei della sessione.
fn default_out_dir() -> PathBuf {
    let base = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let user = std::env::var("SESSIONNAME").unwrap_or_else(|_| "pr".into());
    base.join("photo-restore")
        .join(format!("sess-{}-{}", user.replace(" ", "_"), stamp))
}

/// Stato condiviso: opzionale finche' l'utente non importa nulla.
pub struct AppState {
    pub session: std::sync::Mutex<Option<Session>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            session: std::sync::Mutex::new(None),
        }
    }
}

/// Informazioni da restituire all'interfaccia dopo ogni comando.
#[derive(Debug, Serialize)]
pub struct SessionState {
    pub imported: bool,
    pub original_width: u32,
    pub original_height: u32,
    pub current_width: u32,
    pub current_height: u32,
    pub original_path: Option<String>,
    pub original_file_size: Option<u64>,
    pub preview_path: Option<String>,
    pub original_preview_path: Option<String>,
    pub method: String,
    pub can_undo: bool,
    pub can_redo: bool,
    /// Quota di correzione attiva (0..1) se il pannello fusione e' disponibile.
    pub fusione: f32,
    /// Pannello a cui appartengono la barra di visibilita': "volto" o "ai".
    pub fusione_kind: String,
}

pub static NO_SESSION_MSG: &str = "Nessuna immagine caricata. Aprire prima una fotografia.";

/// Converte lo stato in una struttura per il frontend.
pub fn to_state(s: &Option<Session>) -> SessionState {
    match s {
        Some(sess) => {
            let preview_path = sess.save_preview().ok();
            let original_preview_path = sess.save_original_preview().ok();
            SessionState {
                imported: true,
                original_width: sess.original.width(),
                original_height: sess.original.height(),
                current_width: sess.current.width(),
                current_height: sess.current.height(),
                original_path: sess
                    .original_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string()),
                original_file_size: sess.original_file_size,
                preview_path,
                original_preview_path,
                method: sess.method.clone(),
                can_undo: !sess.undo.is_empty(),
                can_redo: !sess.redo.is_empty(),
                fusione: sess.fusione,
                fusione_kind: if sess.fusione_base.is_some() && sess.fusione_ai.is_some() {
                    fusione_kind_of(&sess.method)
                } else {
                    String::new()
                },
            }
        }
        None => SessionState {
            imported: false,
            original_width: 0,
            original_height: 0,
            current_width: 0,
            current_height: 0,
            original_path: None,
            original_file_size: None,
            preview_path: None,
            original_preview_path: None,
            method: String::new(),
            can_undo: false,
            can_redo: false,
            fusione: 1.0,
            fusione_kind: String::new(),
        },
    }
}

/// Deriva il pannello della barra di visibilita' dal metodo dell'ultima
/// operazione AI (che ha anche impostato la fusione).
pub fn fusione_kind_of(method: &str) -> String {
    if method.starts_with("Ing. AI") {
        "ai".into()
    } else if method.starts_with("Volti") || method.starts_with("Visibilita' recupero") {
        "volto".into()
    } else if method.starts_with("Visibilita' AI") {
        "ai".into()
    } else {
        String::new()
    }
}
