//! Comandi esposti all'interfaccia tramite IPC Tauri.
//!
//! Il frontend esegue il controllo d'intervallo di prima battuta; le
//! funzioni del crate `core` verificano comunque in modo difensivo.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::MutexGuard;
use tauri::{Emitter, Manager};

use pr_core::adjust::AdjustParams;
use pr_core::detect::{PhotoQuad, MAX_QUADS};
use pr_core::io::ExportFormat;
use pr_core::resize::ResizeMethod;

use crate::ai;
use crate::state::{to_state, AppState, Session, SessionState, NO_SESSION_MSG};

type Locked<'a> = MutexGuard<'a, Option<Session>>;

fn lock(app: &AppState) -> Result<Locked<'_>, String> {
    app.session
        .lock()
        .map_err(|_| "blocco interno occupato".into())
}

fn with_session<T>(
    app: &AppState,
    f: impl FnOnce(&mut Session) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = lock(app)?;
    match guard.as_mut() {
        Some(s) => f(s),
        None => Err(NO_SESSION_MSG.into()),
    }
}

/// Stato per il frontend dopo una trasformazione riuscita.
fn state_after(app: &AppState) -> Result<SessionState, String> {
    let guard = lock(app)?;
    Ok(to_state(&guard))
}

/// Anteprime in memoria (base64 PNG): nessun protocollo asset, nessuna
/// dipendenza da percorsi canonici: carica sempre.
#[derive(Serialize)]
pub struct PreviewImages {
    /// Fotografia elaborata (data URI).
    pub current: Option<String>,
    /// Fotografia originale per il confronto (data URI).
    pub original: Option<String>,
}

#[tauri::command]
pub fn preview_images(app: tauri::State<AppState>) -> Result<PreviewImages, String> {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    let guard = lock(&app)?;
    let Some(sess) = guard.as_ref() else {
        return Ok(PreviewImages {
            current: None,
            original: None,
        });
    };
    let uri = |img: &image::RgbaImage| -> Option<String> {
        crate::state::thumbnail_png_bytes(img)
            .ok()
            .map(|b| format!("data:image/png;base64,{}", B64.encode(b)))
    };
    Ok(PreviewImages {
        current: uri(&sess.current),
        original: uri(&sess.original),
    })
}

/// Recupero dei dettagli di un JPEG molto compresso.
#[tauri::command]
pub fn apply_jpeg_restore(
    app: tauri::State<AppState>,
    strength: f32,
) -> Result<SessionState, String> {
    with_session(&app, |sess| {
        sess.push_history();
        sess.current = pr_core::restore::jpeg_restore(&sess.current, strength);
        sess.method = format!("Recupero dettagli JPEG ({:.0}%)", strength * 100.0);
        Ok(())
    })?;
    state_after(&app)
}

/// Rimozione dello sfondo per colore cliccato (tolerance 0..100).
#[tauri::command]
pub fn remove_background(
    app: tauri::State<AppState>,
    x: i32,
    y: i32,
    tolerance: f32,
) -> Result<SessionState, String> {
    let (w, h) = with_session(&app, |sess| Ok(sess.current.dimensions()))?;
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        return Err("clicca su un punto della fotografia (dentro i bordi)".into());
    }
    with_session(&app, |sess| {
        sess.push_history();
        sess.current =
            pr_core::bg::remove_background(&sess.current, (x as u32, y as u32), tolerance);
        sess.method = format!("Rimozione sfondo (punto {}, {})", x, y);
        Ok(())
    })?;
    state_after(&app)
}

/// Rimozione automatica dello sfondo: individua il colore dal perimetro.
#[tauri::command]
pub fn auto_remove_background(
    app: tauri::State<AppState>,
    tolerance: f32,
) -> Result<SessionState, String> {
    with_session(&app, |sess| {
        sess.push_history();
        sess.current = pr_core::bg::remove_background_auto(&sess.current, tolerance);
        sess.method = format!("Rimozione automatica dello sfondo ({:.0})", tolerance);
        Ok(())
    })?;
    state_after(&app)
}

/// Snapshot dello stato attuale, richiesto all'avvio.
#[tauri::command]
pub fn get_session_state(app: tauri::State<AppState>) -> Result<SessionState, String> {
    let guard = lock(&app)?;
    Ok(to_state(&guard))
}

/// Import: caricamento esplicito dichiarato dal frontend (drag & drop).
#[tauri::command]
pub fn load_paths(app: tauri::State<AppState>, paths: Vec<String>) -> Result<SessionState, String> {
    let first = paths
        .first()
        .ok_or_else(|| "nessun file indicato".to_string())?;
    let path = PathBuf::from(first);
    let img = pr_core::io::load(&path).map_err(|e| e.to_string())?;
    let sess = Session::new(img, Some(path.to_path_buf()));
    *lock(&app)? = Some(sess);
    state_after(&app)
}

/// Dialogo di apertura file: spazio dedicato, il pulsante non occupa la sessione,
/// cosi' il messaggio dialogo appare davanti alla finestra principale.
#[tauri::command(async)]
pub fn open_dialog(
    app: tauri::State<AppState>,
    window: tauri::WebviewWindow,
) -> Result<SessionState, String> {
    // Tutti i formati leggibili da `image` (caricamento si' via `with_guessed_format`).
    let estensioni = [
        "png", "jpg", "jpeg", "jfif", "tif", "tiff", "webp", "bmp", "gif", "ico", "tga", "dds",
    ];
    let picked = rfd::FileDialog::new()
        .set_title("Apri una fotografia")
        .add_filter("Tutte le immagini", &estensioni)
        .add_filter("JPEG e PNG", &["png", "jpg", "jpeg", "jfif"])
        .add_filter("Scansioni (TIFF)", &["tif", "tiff"])
        .add_filter("Tutti i file", &["*"])
        .set_parent(&window)
        .pick_file();
    let Some(path) = picked else {
        return Err("apertura annullata".into());
    };
    let img = pr_core::io::load(&path).map_err(|e| e.to_string())?;
    let sess = Session::new(img, Some(path.to_path_buf()));
    let mut guard = lock(&app)?;
    *guard = Some(sess);
    drop(guard);
    state_after(&app)
}

/// Apre la pagina PayPal per la donazione nel browser predefinito.
#[tauri::command]
pub fn dona() -> Result<String, String> {
    const URL: &str = "https://paypal.me/SalvaCucca";
    std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", URL])
        .spawn()
        .map_err(|e| format!("impossibile aprire il browser: {e}"))?;
    Ok("pagina di donazione aperta nel browser".into())
}

/// Rilevamento automatico delle fotografie nella scansione.
#[tauri::command]
pub fn detect_photos(app: tauri::State<AppState>) -> Result<Vec<PhotoQuad>, String> {
    with_session(&app, |sess| {
        let quads = pr_core::detect::detect_photos(&sess.current);
        sess.method = "Rilevamento fotografie".into();
        Ok(quads.into_iter().take(MAX_QUADS).collect())
    })
}

/// Ritaglio rettangolare.
#[tauri::command]
pub fn apply_crop(
    app: tauri::State<AppState>,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<SessionState, String> {
    with_session(&app, |sess| {
        if x < 0 || y < 0 || width <= 0 || height <= 0 {
            return Err("riquadro di ritaglio non valido".into());
        }
        sess.push_history();
        let cropped = pr_core::transform::crop(
            &sess.current,
            x as u32,
            y as u32,
            width as u32,
            height as u32,
        )
        .map_err(|e| e.to_string())?;
        sess.current = cropped;
        sess.method = "Ritaglio".into();
        Ok(())
    })?;
    state_after(&app)
}

/// Correzione prospettica con quattro angoli in coordinate normalizzate.
#[tauri::command]
pub fn apply_perspective(
    app: tauri::State<AppState>,
    corners: Vec<[f32; 2]>,
) -> Result<SessionState, String> {
    with_session(&app, |sess| {
        if corners.len() != 4 {
            return Err("servono esattamente quattro angoli".into());
        }
        if corners.iter().any(|c| !(c[0].is_finite() && c[1].is_finite())) {
            return Err("angoli non validi".into());
        }
        let (w, h) = sess.current.dimensions();
        let corners_px: [[f64; 2]; 4] = corners
            .into_iter()
            .map(|c| [c[0] as f64 * w as f64, c[1] as f64 * h as f64])
            .collect::<Vec<_>>()
            .try_into()
            .map_err(|_| "angoli incompleti".to_string())?;
        // Dimensioni di uscita: media dei lati opposti del quadrilatero.
        let dist =
            |a: [f64; 2], b: [f64; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        let top = dist(corners_px[0], corners_px[1]);
        let bottom = dist(corners_px[3], corners_px[2]);
        let left = dist(corners_px[0], corners_px[3]);
        let right = dist(corners_px[1], corners_px[2]);
        let out_w = (((top + bottom) / 2.0).round() as u32).clamp(16, 12000);
        let out_h = (((left + right) / 2.0).round() as u32).clamp(16, 12000);
        sess.push_history();
        let warped = pr_core::transform::perspective_warp(&sess.current, &corners_px, out_w, out_h)
            .map_err(|e| e.to_string())?;
        sess.current = warped;
        sess.method = "Correzione prospettica".into();
        Ok(())
    })?;
    state_after(&app)
}

/// Rotazione: `quarter_turns` per i multipli di 90, `degrees` per il fine.
#[tauri::command]
pub fn apply_rotate(
    app: tauri::State<AppState>,
    degrees: f32,
    quarter_turns: i32,
) -> Result<SessionState, String> {
    with_session(&app, |sess| {
        sess.push_history();
        if quarter_turns != 0 {
            sess.current = pr_core::transform::rotate_quarter(&sess.current, quarter_turns);
            sess.method = "Rotazione di 90 gradi".into();
        } else if degrees.abs() > 0.05 {
            if !degrees.is_finite() || degrees.abs() > 45.0 {
                return Err("angolo di raddrizzamento fuori intervallo (max 45 gradi)".into());
            }
            sess.current = pr_core::transform::rotate_free(&sess.current, degrees as f64);
            sess.method = "Raddrizzamento".into();
        }
        Ok(())
    })?;
    state_after(&app)
}

/// Parametri del ridimensionamento tradizionale.
#[derive(Debug, Deserialize)]
pub struct ResizeOptions {
    pub method: ResizeMethod,
    pub factor: Option<f32>,
    pub keep_aspect: bool,
    pub target_width: Option<u32>,
    pub target_height: Option<u32>,
    pub sharpen: Option<f32>,
}

#[tauri::command]
pub fn apply_resize(
    app: tauri::State<AppState>,
    options: ResizeOptions,
) -> Result<SessionState, String> {
    with_session(&app, |sess| {
        let (w, h) = sess.current.dimensions();
        let o = &options;
        let (tw, th): (u32, u32) = if let Some(f) = o.factor {
            if f <= 0.0 || f > 16.0 {
                return Err("fattore di ingrandimento fuori intervallo (0 .. 16)".into());
            }
            (
                ((w as f64 * f as f64).round() as u32).max(1),
                ((h as f64 * f as f64).round() as u32).max(1),
            )
        } else if o.keep_aspect {
            pr_core::resize::fitted_size(w, h, o.target_width, o.target_height)
        } else {
            match (o.target_width, o.target_height) {
                (Some(w2), Some(h2)) => (w2.max(1), h2.max(1)),
                _ => return Err("specificare larghezza e altezza finali".into()),
            }
        };
        let sharpen = o.sharpen.unwrap_or(0.0);
        if !(0.0..=1.5).contains(&sharpen) {
            return Err("nitidezza fuori intervallo (0 .. 1.5)".into());
        }
        let resized = pr_core::resize::resize(&sess.current, o.method, tw, th, sharpen)
            .map_err(|e| e.to_string())?;
        sess.push_history();
        sess.current = resized;
        sess.method = format!("Ridimensionamento {}", o.method.label());
        if sharpen > 0.0 {
            sess.method.push_str(&format!(" + nitidezza {sharpen:.2}"));
        }
        Ok(())
    })?;
    state_after(&app)
}

#[tauri::command]
pub fn apply_adjust(
    app: tauri::State<AppState>,
    params: AdjustParams,
) -> Result<SessionState, String> {
    with_session(&app, |sess| {
        let adjusted = pr_core::adjust::apply(&sess.current, &params).map_err(|e| e.to_string())?;
        sess.push_history();
        sess.current = adjusted;
        let mut label = String::from("Correzioni");
        if params.grayscale {
            label.push_str(", bianco e nero");
        }
        if params.auto_white_balance {
            label.push_str(", correzione dominanti");
        }
        sess.method = label;
        Ok(())
    })?;
    state_after(&app)
}

#[tauri::command]
pub fn undo(app: tauri::State<AppState>) -> Result<SessionState, String> {
    with_session(&app, |sess| {
        sess.undo
            .pop()
            .map(|prev| (prev, sess.current.clone()))
            .ok_or_else(|| "niente da annullare".to_string())
            .map(|(prev, cur)| {
                sess.redo.push(cur);
                sess.current = prev;
                sess.method = "Annullamento".into();
                sess.fusione_p_precedente = sess.fusione;
                sess.fusione = 1.0;
                sess.fusione_spinta = false;
            })
    })?;
    state_after(&app)
}

#[tauri::command]
pub fn redo(app: tauri::State<AppState>) -> Result<SessionState, String> {
    with_session(&app, |sess| {
        sess.redo
            .pop()
            .map(|next| (next, sess.current.clone()))
            .ok_or_else(|| "niente da ripristinare".to_string())
            .map(|(next, cur)| {
                sess.undo.push(cur);
                sess.current = next;
                sess.method = "Ripristino".into();
                sess.fusione = sess.fusione_p_precedente;
                sess.fusione_spinta = true;
            })
    })?;
    state_after(&app)
}

#[tauri::command]
pub fn reset_to_original(app: tauri::State<AppState>) -> Result<SessionState, String> {
    with_session(&app, |sess| {
        sess.push_history();
        sess.current = sess.original.clone();
        sess.method = "Ritorno all'originale".into();
        Ok(())
    })?;
    state_after(&app)
}

// ---------------------------------------------------------------- visibilita'

/// Unisce una foto di riferimento e una correzione alla quota `p`
/// (0 = solo riferimento, 1 = solo correzione), in fisso a 8 bit.
pub fn fusione_lerp(
    base: &image::RgbaImage,
    ai: &image::RgbaImage,
    p: f32,
) -> Result<image::RgbaImage, String> {
    if base.dimensions() != ai.dimensions() {
        return Err("le due immagini non hanno la stessa dimensione".into());
    }
    let q = ((p.clamp(0.0, 1.0)) * 256.0).round() as u16;
    let mut out = base.clone();
    for (b, a) in out.pixels_mut().zip(ai.pixels()) {
        for c in 0..4 {
            b.0[c] = ((u16::from(b.0[c]) * (256 - q) + u16::from(a.0[c]) * q + 128) / 256) as u8;
        }
    }
    Ok(out)
}

/// Quando lo storico non e' ancora a conoscenza del risultato AI, lo registra
/// (una sola volta per risultato): annullare porta alla correzione piena.
fn fusione_storico(sess: &mut Session) {
    if !sess.fusione_spinta {
        sess.registra_storico();
        sess.fusione_spinta = true;
    }
}

/// Dosa la correzione AI visibile: 0% = foto come prima della correzione
/// (per l'upscaling, ingrandimento normale senza AI), 100% = correzione piena.
/// Aggiorna l'anteprima: la barra reagisce in tempo reale.
#[tauri::command]
pub fn imposta_fusione(
    app: tauri::State<AppState>,
    percento: f32,
) -> Result<SessionState, String> {
    with_session(&app, |sess| {
        if sess.fusione_base.is_none() || sess.fusione_ai.is_none() {
            return Err(format!(
                "nessuna correzione da dosare ({percento}%): esegui prima il restauro \
                 dei volti o l'ingrandimento AI"
            ));
        }
        fusione_storico(sess);
        // `take` per spostare le immagini fuori dalla sessione senza cloni:
        // sono le stesse due immagini che servono ai passi successivi.
        // In caso di errore vengono RESTITUITE, cosi' la barra resta usabile.
        let base = sess.fusione_base.take();
        let ai = sess.fusione_ai.take();
        let p = (percento / 100.0).clamp(0.0, 1.0);
        let out = {
            let composizione = base
                .as_ref()
                .zip(ai.as_ref())
                .ok_or_else(|| "fusione assente".to_string())
                .and_then(|(b, a)| fusione_lerp(b, a, p));
            match composizione {
                Ok(img) => img,
                Err(e) => {
                    sess.fusione_base = base;
                    sess.fusione_ai = ai;
                    return Err(e);
                }
            }
        };
        sess.fusione_base = base;
        sess.fusione_ai = ai;
        sess.current = out;
        sess.fusione = p;
        sess.method = match crate::state::fusione_kind_of(&sess.method) {
            k if k == "ai" => format!("Visibilita' AI ({:.0}%)", p * 100.0),
            k if k == "volto" => format!("Visibilita' recupero ({:.0}%)", p * 100.0),
            _ => format!("Visibilita' ({:.0}%)", p * 100.0),
        };
        Ok(())
    })?;
    state_after(&app)
}

/// Opzioni di esportazione come arrivano dal frontend.
#[derive(Debug, Deserialize)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub quality: u8,
    #[serde(default)]
    pub overwrite_existing: bool,
}

#[tauri::command(async)]
pub fn export_image(
    app: tauri::State<AppState>,
    window: tauri::WebviewWindow,
    options: ExportOptions,
) -> Result<String, String> {
    // Prima: prendo il nome di riferimento (senza tenere la sessione occupata).
    let nome_orig: Option<String> = with_session(&app, |sess| {
        Ok(sess
            .original_path
            .as_ref()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned())))
    })?;
    let ext = match options.format {
        ExportFormat::Png => "png",
        ExportFormat::Jpeg => "jpg",
        ExportFormat::Tiff => "tiff",
    };
    let default_name = match nome_orig {
        Some(n) if !n.is_empty() => format!("{}-restaurata.{}", n, ext),
        _ => format!("fotografia-restaurata.{}", ext),
    };
    let dest = rfd::FileDialog::new()
        .set_title("Salva la fotografia elaborata")
        .add_filter("Immagine", &[ext])
        .set_file_name(default_name)
        .set_parent(&window)
        .save_file()
        .ok_or_else(|| "salvataggio annullato".to_string())?;
    // Non distruttivo: l'originale non viene mai sovrascritto.
    if let Some(orig) = with_session(&app, |sess| {
        Ok(sess.original_path.as_ref().map(|p| p.to_path_buf()))
    })? {
        if orig == dest {
            return Err("non e' possibile sovrascrivere la fotografia originale".into());
        }
    }
    if dest.exists() && !options.overwrite_existing {
        return Err("il file esiste gia'. Scegli un nome diverso.".into());
    }
    let bytes = with_session(&app, |sess| Ok(sess.current.clone()))?;
    pr_core::io::save(&bytes, &dest, options.format, options.quality).map_err(|e| e.to_string())?;
    Ok(dest.to_string_lossy().to_string())
}

// ------------------------------------------------------------------ moduli AI

/// Rimozione dello sfondo con modello di segmentazione; esito via evento
/// "ai-done" (elaborazione in background thread).
#[tauri::command(async)]
pub fn sfondo_ai(
    app: tauri::AppHandle,
    app_state: tauri::State<AppState>,
    model_id: String,
) -> Result<String, String> {
    let m = ai::find_model(&model_id).ok_or_else(|| format!("modello sconosciuto: {model_id}"))?;
    ai::verify_model(m)?;
    let dll = ai::dll_path();
    if !dll.exists() {
        return Err("runtime ONNX non installato".into());
    }
    let model_file = ai::model_file(&model_id).ok_or("file del modello assente")?;
    pr_inference::engine::init_runtime(&dll).map_err(|e| e.to_string())?;
    let img = {
        let guard = lock(&app_state)?;
        let Some(sess) = guard.as_ref() else {
            return Err(crate::state::NO_SESSION_MSG.into());
        };
        sess.current.clone()
    };
    if !ai::acquire_busy() {
        return Err("operazione AI gia' in corso".into());
    }
    let app2 = app.clone();
    std::thread::spawn(move || {
        let inner = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (move || -> Result<image::RgbaImage, String> {
                let mut seg =
                    pr_inference::seg::Segmenter::new(&model_file).map_err(|e| e.to_string())?;
                seg.rimuovi_sfondo(&img).map_err(|e| e.to_string())
            })()
        }));
        ai::set_busy(false);
        let inner = flatten_panic(inner);
        let manager = app2.state::<AppState>();
        match manager.session.lock() {
            Ok(mut guard) => {
                if let Some(sess) = guard.as_mut() {
                    match inner {
                        Ok(out) => {
                            sess.push_history();
                            sess.current = out;
                            sess.method = "Rimozione sfondo (AI)".into();
                            ai_succeed(&app2, "sfondo rimosso".into());
                        }
                        Err(e) => ai_fail(&app2, e),
                    }
                } else {
                    ai_fail(&app2, "nessuna fotografia caricata".into());
                }
            }
            Err(_) => ai_fail(&app2, "blocco interno occupato".into()),
        }
        let _ = &manager;
    });
    Ok("elaborazione avviata in background".into())
}

/// Converte un esito di `catch_unwind` nel `Result` atteso: un panico interno
/// diventa errore leggibile invece di far morire il thread con il flag busy appeso.
fn flatten_panic<T>(r: std::thread::Result<Result<T, String>>) -> Result<T, String> {
    match r {
        Ok(inner) => inner,
        Err(e) => {
            let msg =
                if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = e.downcast_ref::<&str>() {
                    (*s).to_string()
                } else {
                    "errore inatteso".into()
                };
            Err(format!("crash interno contenuto: {msg}"))
        }
    }
}

/// Lista dei modelli disponibili con stato di installazione.
#[tauri::command]
pub fn ai_models() -> Vec<ai::ModelStatus> {
    ai::statuses()
}

#[derive(Serialize, Clone)]
struct AiProgress {
    kind: String,
    model_id: String,
    done: u64,
    total: u64,
    message: String,
}

fn emit_progress(
    app: &tauri::AppHandle,
    kind: &str,
    model_id: &str,
    done: u64,
    total: u64,
    message: &str,
) {
    let _ = app.emit(
        "ai-progress",
        AiProgress {
            kind: kind.into(),
            model_id: model_id.into(),
            done,
            total,
            message: message.into(),
        },
    );
}

#[derive(Serialize, Clone)]
struct AiDone {
    message: String,
    error: Option<String>,
}

fn ai_fail(app: &tauri::AppHandle, message: String) {
    let _ = app.emit(
        "ai-done",
        AiDone {
            message: String::new(),
            error: Some(message),
        },
    );
}

fn ai_succeed(app: &tauri::AppHandle, message: String) {
    let _ = app.emit(
        "ai-done",
        AiDone {
            message,
            error: None,
        },
    );
}


/// Applica l'ingrandimento AI in background; emette eventi di progresso.
#[tauri::command(async)]
pub fn ai_upscale(
    app: tauri::AppHandle,
    app_state: tauri::State<AppState>,
    model_id: String,
    scala: Option<u32>,
) -> Result<String, String> {
    let m = ai::find_model(&model_id).ok_or_else(|| format!("modello sconosciuto: {model_id}"))?;
    ai::verify_model(m)?;
    let dll = ai::dll_path();
    if !dll.exists() {
        return Err("runtime ONNX non installato: reinstalla il programma".into());
    }
    let scala = scala.unwrap_or(m.scale);
    if scala != 2 && scala != 4 {
        return Err("scala non supportata: usa 2 o 4".into());
    }
    if scala > m.scale * 4 {
        return Err("scala troppo grande per questo modello".into());
    }
    let model_file = ai::model_file(&model_id).ok_or("file del modello assente")?;
    pr_inference::engine::init_runtime(&dll).map_err(|e| e.to_string())?;
    let img = {
        let guard = lock(&app_state)?;
        let Some(sess) = guard.as_ref() else {
            return Err(crate::state::NO_SESSION_MSG.into());
        };
        sess.current.clone()
    };
    if !ai::acquire_busy() {
        return Err("operazione AI gia' in corso".into());
    }
    let app2 = app.clone();
    let app_progress = app.clone();
    let model_scale = m.scale;
    let model_id2 = model_id.clone();
    std::thread::spawn(move || {
        let inner = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (move || -> Result<(image::RgbaImage, image::RgbaImage), String> {
            let mut upscaler = pr_inference::engine::Upscaler::new(&model_file, model_scale)
                .map_err(|e| e.to_string())?;
            upscaler.set_tile(256, 16);
            let prog = move |done: u32, tot: u32| {
                emit_progress(
                    &app_progress,
                    "upscaling",
                    &model_id2,
                    done as u64,
                    tot as u64,
                    "elaborazione dei blocchi",
                );
            };
            // scala richiesta maggiore di quella nativa: passaggi in serie
            let out = if model_scale < scala {
                let o = upscaler.upscale(&img, &prog).map_err(|e| e.to_string())?;
                upscaler.upscale(&o, &prog).map_err(|e| e.to_string())?
            } else {
                upscaler.upscale(&img, &prog).map_err(|e| e.to_string())?
            };
            // scala richiesta minore di quella nativa (es. modello 4x, richiesta
            // x2): si riduce con Lanczos (qualita' invariata, meno lavoro utile)
            let out = if model_scale > scala {
                let f = (model_scale / scala) as u32;
                image::imageops::resize(
                    &out,
                    (out.width() / f).max(1),
                    (out.height() / f).max(1),
                    image::imageops::FilterType::Lanczos3,
                )
            } else {
                out
            };
            // base di confronto senza AI: ingrandimento normale (Lanczos),
            // pronto comunque, per la barra di visibilita'
            let base = image::imageops::resize(
                &img,
                out.width(),
                out.height(),
                image::imageops::FilterType::Lanczos3,
            );
            Ok((out, base))
        })()
        }));
        ai::set_busy(false);
        let inner = flatten_panic(inner);
        match inner {
            Ok((out_img, base_img)) => {
                let manager = app2.state::<AppState>();
                match manager.session.lock() {
                    Ok(mut guard) => {
                        if let Some(sess) = guard.as_mut() {
                            sess.push_history();
                            sess.method = format!("Ing. AI x{scala}");
                            sess.current = out_img.clone();
                            sess.fusione_base = Some(base_img);
                            sess.fusione_ai = Some(out_img);
                            sess.fusione = 1.0;
                            sess.fusione_spinta = false;
                        }
                        ai_succeed(&app2, format!("ingrandimento x{scala} completato"));
                    }
                    Err(_) => ai_fail(&app2, "blocco interno occupato".into()),
                }
                let _ = &manager;
            }
            Err(e) => ai_fail(&app2, e),
        }
    });
    Ok("elaborazione avviata in background".into())
}

/// Rileva e ripristina i volti con RetinaFace + CodeFormer/GFPGAN in
/// background (modelli inclusi nell'installer). Esito via
/// eventi ai-progress/ai-done.
#[tauri::command(async)]
pub fn volto_ai(
    app: tauri::AppHandle,
    app_state: tauri::State<AppState>,
    model_id: String,
    detector: Option<String>,
) -> Result<String, String> {
    let detector_id = detector.unwrap_or_else(|| "volto-det-retinaface".into());
    // tutti i controlli precari prima della prenotazione del flag busy
    ai::find_model(&detector_id).ok_or_else(|| format!("modello sconosciuto: {detector_id}"))?;
    let m = ai::find_model(&model_id).ok_or_else(|| format!("modello sconosciuto: {model_id}"))?;
    if ai::model_file(&model_id).map(|p| p.exists()).unwrap_or(false) {
        ai::verify_model(m)?;
    }
    let img = {
        let guard = lock(&app_state)?;
        let Some(sess) = guard.as_ref() else {
            return Err(crate::state::NO_SESSION_MSG.into());
        };
        sess.current.clone()
    };
    if !ai::acquire_busy() {
        return Err("operazione AI gia' in corso".into());
    }
    let app2 = app.clone();
    let app3 = app2.clone();
    std::thread::spawn(move || {
        let inner = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (move || -> Result<Option<(image::RgbaImage, image::RgbaImage)>, String> {
            // 1. runtime ONNX (una sola volta)
            let dll = ai::dll_path();
            if !dll.exists() {
                emit_progress(
                    &app3,
                    "runtime",
                    "",
                    0,
                    ai::RUNTIME_ZIP_MB * 1024 * 1024,
                    "download del runtime ONNX Runtime (una sola volta)",
                );
                ai::download_runtime(|done, total| {
                    emit_progress(&app3, "runtime", "", done, total, "download del runtime");
                })?;
            }
            // 2. detector + ripristinatore (quelli mancanti)
            let det_id2 = detector_id.as_str();
            for id in [det_id2, model_id.as_str()] {
                let mm =
                    ai::find_model(id).ok_or_else(|| format!("modello sconosciuto: {id}"))?;
                if ai::model_file(id).map(|p| !p.exists()).unwrap_or(true) {
                    emit_progress(
                        &app3,
                        "modello",
                        mm.id,
                        0,
                        (mm.size_mb as u64) * 1024 * 1024,
                        &format!("download di {}", mm.name),
                    );
                    ai::download_model(mm, |done, total| {
                        emit_progress(&app3, "modello", mm.id, done, total, "download del modello");
                    })?;
                }
                ai::verify_model(mm)?;
            }
            pr_inference::engine::init_runtime(&dll).map_err(|e| e.to_string())?;
            // 3. rilevamento
            emit_progress(&app3, "volto", "", 0, 1, "individuazione dei volti");
            let det_file = ai::model_file(det_id2).ok_or("file del detector assente")?;
            let res_file = ai::model_file(&model_id).ok_or("file del modello assente")?;
            let mut det = pr_inference::faces::FaceDetector::new(&det_file)
                .map_err(|e| e.to_string())?;
            let volti = det.detect(&img).map_err(|e| e.to_string())?;
            if volti.is_empty() {
                return Ok(None);
            }
            // 4. ripristino volto per volto
            let totale = volti.len() as u64;
            let model_id2 = model_id.clone();
            let mut out = img.clone();
            for (i, f) in volti.iter().enumerate() {
                emit_progress(
                    &app3,
                    "volto",
                    &model_id2,
                    i as u64,
                    totale,
                    &format!("ripristino del volto {} di {}", i + 1, totale),
                );
                let mut rs = pr_inference::faces::FaceRestorer::new(&res_file)
                    .map_err(|e| e.to_string())?;
                out = rs.ripristina(&out, f).map_err(|e| e.to_string())?;
            }
            emit_progress(
                &app3,
                "volto",
                &model_id2,
                totale,
                totale,
                "volti ripristinati",
            );
            Ok(Some((out, img)))
        })()
        }));
        ai::set_busy(false);
        let inner = flatten_panic(inner);
        if let Ok(Some(..)) = inner {
            let manager = app2.state::<AppState>();
            match manager.session.lock() {
                Ok(mut guard) => {
                    if let Some(sess) = guard.as_mut() {
                        let (out_img, base_img) = inner.ok().unwrap().unwrap();
                        sess.push_history();
                        sess.method = "Volti ripristinati (AI)".into();
                        sess.current = out_img.clone();
                        sess.fusione_base = Some(base_img);
                        sess.fusione_ai = Some(out_img);
                        sess.fusione = 1.0;
                        sess.fusione_spinta = false;
                    }
                    ai_succeed(&app2, "volti ripristinati".into());
                }
                Err(_) => ai_fail(&app2, "blocco interno occupato".into()),
            }
            let _ = &manager;
        } else if let Ok(None) = inner {
            ai_succeed(&app2, "nessun volto rilevato".into());
        } else if let Err(e) = inner {
            ai_fail(&app2, e);
        }
    });
    Ok("elaborazione avviata in background".into())
}

#[cfg(test)]
mod fusione_tests {
    use super::*;

    fn img(c: u8) -> image::RgbaImage {
        image::RgbaImage::from_pixel(2, 2, image::Rgba([c, c, c, 255]))
    }

    #[test]
    fn estremi_e_passo_mezzo() {
        let base = img(0);
        let ai = img(200);
        for (p, atteso) in [(0.0, 0u8), (1.0, 200u8), (0.5, 100u8)] {
            let out = fusione_lerp(&base, &ai, p).unwrap();
            assert_eq!(out.get_pixel(0, 0).0[0], atteso, "p={p}");
            assert_eq!(out.dimensions(), ai.dimensions());
        }
    }

    #[test]
    fn dimensioni_diverse_sono_un_errore() {
        let a = img(0);
        let b = image::RgbaImage::new(3, 2);
        assert!(fusione_lerp(&a, &b, 0.5).is_err());
    }
}
