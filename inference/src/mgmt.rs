//! Gestione dei modelli AI: cartelle dati, download DDS, verifica integrita'
//! ed estrazione del runtime ONNX. Indipendente da Tauri per essere testabile.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::models;
pub use crate::models::ModelInfo;

pub const RUNTIME_ZIP_URL: &str =
    "https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/onnxruntime-win-x64-1.31.0.zip";
pub const RUNTIME_ZIP_MB: u64 = 79;
/// SHA-256 atteso dell'archivio ufficiale 1.31.0 (calcolato sul pacchetto
/// distribuito con l'installer): ogni download di ripristino viene verificato.
pub const RUNTIME_ZIP_SHA256: &str =
    "3958d8a44984160692c1b4ffd0de3cb48f8c39f9d34bb31e433db06e30b82195";
pub const RUNTIME_DLL: &str = "onnxruntime.dll";

pub struct Paths {
    pub models_dir: PathBuf,
    pub runtime_dir: PathBuf,
}

/// Cartelle di lavoro dell'app sotto %LOCALAPPDATA%\PhotoRestore.
pub fn paths() -> Paths {
    let mut base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.push("PhotoRestore");
    let models_dir = base.join("modelli");
    let runtime_dir = base.join("runtime");
    Paths {
        models_dir,
        runtime_dir,
    }
}

/// File distribuito con l'installer: cercato nella cartella risorse
/// accanto all'eseguibile (layout NSIS/Windows).
fn bundled(sub: &str, file: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [
        dir.join("resources").join(sub).join(file),
        dir.join("resources").join(file),
        dir.join(sub).join(file),
    ]
    .into_iter()
    .find(|p| p.exists())
}

pub fn dll_path() -> PathBuf {
    let local = paths().runtime_dir.join(RUNTIME_DLL);
    if local.exists() {
        local
    } else {
        bundled("runtime", RUNTIME_DLL).unwrap_or(local)
    }
}

pub fn model_path(m: &ModelInfo) -> PathBuf {
    let local = paths().models_dir.join(m.filename);
    if local.exists() {
        local
    } else {
        bundled("modelli", m.filename).unwrap_or(local)
    }
}

#[derive(Serialize, Clone)]
pub struct ModelStatus {
    pub id: String,
    pub name: String,
    pub scale: u32,
    pub kind: String,
    pub size_mb: u64,
    pub installed: bool,
    pub runtime_ready: bool,
    pub license: String,
    pub license_url: String,
    pub notice: String,
}

/// Stato dei modelli per l'interfaccia.
pub fn statuses() -> Vec<ModelStatus> {
    let dll = dll_path();
    models::list()
        .into_iter()
        .map(|m| ModelStatus {
            id: m.id.to_string(),
            name: m.name.to_string(),
            scale: m.scale,
            kind: m.kind.to_string(),
            size_mb: m.size_mb as u64,
            installed: model_path(m).exists(),
            runtime_ready: dll.exists(),
            license: m.license.to_string(),
            license_url: m.license_url.to_string(),
            notice: m.notice.to_string(),
        })
        .collect()
}

pub fn find_model(id: &str) -> Option<&'static ModelInfo> {
    models::find(id)
}

/// Tutti i modelli registrati, per operazioni su tutta la lista.
pub fn all_models() -> Vec<&'static ModelInfo> {
    models::list()
}
pub fn model_file(id: &str) -> Option<PathBuf> {
    find_model(id).map(model_path)
}

pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 128 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

/// Scarica il modello se assente e ne verifica l'integrita'.
pub fn download_model(
    m: &'static ModelInfo,
    on_progress: impl Fn(u64, u64),
) -> Result<(PathBuf, String), String> {
    let dir = paths().models_dir;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = model_path(m);
    let r = crate::download::download(m.url, &dest, m.sha256, "Photo Restore", on_progress)
        .map_err(|e| e.to_string())?;
    Ok((r.path, r.sha256))
}

/// Estrae la DLL della runtime dal pacchetto ZIP ufficiale.
pub fn extract_runtime_dll(zip_file: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    let f = std::fs::File::open(zip_file).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().map_err(|e| e.to_string())?.into_owned();
        let file = name.rsplit('/').next().unwrap_or(&name).to_string();
        if file.eq_ignore_ascii_case(RUNTIME_DLL) {
            std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
            let out =
                std::fs::File::create(dest_dir.join(RUNTIME_DLL)).map_err(|e| e.to_string())?;
            let mut out = out;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            return Ok(dest_dir.join(RUNTIME_DLL));
        }
    }
    Err(format!("il pacchetto non contiene {RUNTIME_DLL}"))
}

/// Scarica l'archivio della runtime Windows e ne estrae la DLL.
pub fn download_runtime(on_progress: impl Fn(u64, u64)) -> Result<PathBuf, String> {
    let dir = paths().runtime_dir;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let zip_path = dir.join("onnxruntime-win-x64-1.31.0.zip");
    let r = crate::download::download(
        RUNTIME_ZIP_URL,
        &zip_path,
        Some(RUNTIME_ZIP_SHA256),
        "Photo Restore",
        on_progress,
    )
    .map_err(|e| e.to_string())?;
    let dll = extract_runtime_dll(&r.path, &dir)?;
    dll.try_exists().map_err(|e| e.to_string())?;
    Ok(dll)
}

/// Verifica lo SHA-256 di un modello installato; se errato lo rimuove.
pub fn verify_model(m: &'static ModelInfo) -> Result<(), String> {
    let path = model_path(m);
    if !path.exists() {
        return Err("modello non installato".into());
    }
    if let Some(expected) = m.sha256 {
        let actual = sha256_file(&path)?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&path);
            return Err(format!(
                "integrita' errata: atteso {expected}, trovato {actual}"
            ));
        }
    }
    Ok(())
}
