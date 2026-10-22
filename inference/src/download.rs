//! Download dei modelli con pausa/riprendi e verifica dell'integrità.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{InferenceError, Result};

pub struct DownloadResult {
    pub path: PathBuf,
    pub sha256: String,
}

/// Scarica `url` su `dest` mostrando il progresso (byte) tramite `on_progress`.
///
/// Riprende da eventuali file parziali preesistenti (`.download`).
pub fn download<F>(
    url: &str,
    dest: &Path,
    expected_sha256: Option<&str>,
    user_agent: &str,
    mut on_progress: F,
) -> Result<DownloadResult>
where
    F: FnMut(u64, u64),
{
    let agent = format!("photo-restore ({}); ureq/rustls", user_agent);
    let head = ureq::AgentBuilder::new()
        .build()
        .head(url)
        .set("User-Agent", &agent)
        .call()
        .map_err(|e| InferenceError::Model(format!("richiesta dimensioni: {e}")))?;
    let total = head
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    let partial = dest.with_extension("download");
    let mut file = if partial.exists() {
        OpenOptions::new().append(true).open(&partial)?
    } else {
        File::create(&partial)?
    };
    let mut got = file.metadata()?.len();

    // L'hash deve coprire anche i byte gia' presenti: leggo il prefisso esistente.
    let mut hasher = Sha256::new();
    if got > 0 {
        let mut f = File::open(&partial)?;
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    } else {
        drop(file);
        file = File::create(&partial)?;
    }

    let mut request = ureq::AgentBuilder::new()
        .build()
        .get(url)
        .set("User-Agent", &agent);
    if got > 0 {
        // offset incluso; altrimenti il server risponde con la risposta completa
        let resume = got;
        request = request.set("Range", &format!("bytes={resume}-"));
    }
    let resp = request
        .call()
        .map_err(|e| InferenceError::Model(format!("download: {e}")))?;
    if got > 0 && resp.status() != 206 {
        // il server non supporta la ripresa: riparte dall'inizio
        got = 0;
        hasher = Sha256::new();
        drop(file);
        file = File::create(&partial)?;
    }

    let mut reader = resp.into_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded = got;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        on_progress(downloaded, total);
    }
    file.flush()?;

    // Un download troncato (connessione chiusa) non deve apparire completo.
    if total > 0 && downloaded != total {
        // il file parziale resta su disco per riprendere in un secondo momento
        return Err(InferenceError::Model(format!(
            "download incompleto: attesi {total} byte, ricevuti {downloaded}"
        )));
    }

    let hash = format!("{:x}", hasher.finalize());
    if let Some(expected) = expected_sha256 {
        if !hash.eq_ignore_ascii_case(expected) {
            std::fs::remove_file(&partial).ok();
            return Err(InferenceError::HashMismatch(expected.to_string()));
        }
    }
    std::fs::rename(&partial, dest)?;
    Ok(DownloadResult {
        path: dest.to_path_buf(),
        sha256: hash,
    })
}

/// Calcola il SHA-256 di un file esistente.
pub fn sha256_of(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 128 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
