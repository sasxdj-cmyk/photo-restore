//! Glue del backend per la gestione AI: re-esporta la gestione dei modelli
//! (in `pr_inference::mgmt`) e aggiunge il flag di occupazione per l'app.

use std::sync::atomic::{AtomicBool, Ordering};

pub use pr_inference::mgmt::{
    self as mgmt_inner, all_models, dll_path, download_model, download_runtime,
    extract_runtime_dll, find_model, model_file, model_path, paths, sha256_file, statuses,
    verify_model, ModelInfo, ModelStatus, Paths, RUNTIME_DLL, RUNTIME_ZIP_MB, RUNTIME_ZIP_URL,
};

static AI_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn is_busy() -> bool {
    AI_RUNNING.load(Ordering::SeqCst)
}

/// Riserva il flag di occupazione in modo atomico: true solo se libero.
/// Elimina la corsa check-then-set tra invocazioni ravvicinate.
pub fn acquire_busy() -> bool {
    AI_RUNNING
        .compare_exchange(
            false,
            true,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .is_ok()
}

pub fn set_busy(busy: bool) {
    AI_RUNNING.store(busy, Ordering::SeqCst);
}
