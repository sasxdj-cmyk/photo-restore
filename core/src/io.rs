//! Caricamento e salvataggio immagini.
//!
//! L'elaborazione e' non distruttiva: l'originale non viene mai sovrascritto.
//! I metadati (EXIF e simili) non vengono copiati nell'export: il decoder
//! `image` estrae solo i pixel, quindi gli esporti sono gia' ripuliti
//! (barra di opzione «rimuovi metadati» e' il comportamento predefinito,
//! by-design in Fase 2: la copia selettiva dei metadati e' prevista in Fase 4).

use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::{ImageFormat, ImageReader};
use rayon::prelude::*;

use crate::error::{PhotoError, Result};

/// Dimensioni e informazione di caricamento.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct PhotoInfo {
    pub width: u32,
    pub height: u32,
    pub megapixel: f64,
}

/// Formati supportati per l'esportazione.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Png,
    Jpeg,
    Tiff,
}

impl ExportFormat {
    /// Ricava il formato dall'estensione del file.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" => Some(ExportFormat::Png),
            "jpg" | "jpeg" => Some(ExportFormat::Jpeg),
            "tif" | "tiff" => Some(ExportFormat::Tiff),
            _ => None,
        }
    }
}

/// Carica un'immagine da disco e la converte in RGBA.
pub fn load(path: &Path) -> Result<image::RgbaImage> {
    let reader = ImageReader::open(path).map_err(PhotoError::Io)?;
    let reader = reader.with_guessed_format().map_err(PhotoError::Io)?;
    let img = reader.decode().map_err(PhotoError::Image)?;
    Ok(img.to_rgba8())
}

/// Restituisce solo le info di un'immagine senza tenerne i pixel in memoria definitiva.
pub fn info(path: &Path) -> Result<PhotoInfo> {
    let img = load(path)?;
    Ok(photo_info(&img))
}

/// Statistiche dei pixel di un'immagine.
pub fn photo_info(img: &image::RgbaImage) -> PhotoInfo {
    let (w, h) = img.dimensions();
    PhotoInfo {
        width: w,
        height: h,
        megapixel: w as f64 * h as f64 / 1_000_000.0,
    }
}

/// Salva un'immagine in RGB(A) sul percorso richiesto.
///
/// `quality` e' usato solo per JPEG (1-95). PNG e TIFF lo ignorano.
pub fn save(img: &image::RgbaImage, path: &Path, format: ExportFormat, quality: u8) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(PhotoError::Io)?;
    }
    match format {
        ExportFormat::Png => img
            .save_with_format(path, ImageFormat::Png)
            .map_err(PhotoError::Image),
        ExportFormat::Tiff => img
            .save_with_format(path, ImageFormat::Tiff)
            .map_err(PhotoError::Image),
        ExportFormat::Jpeg => {
            let file = std::fs::File::create(path).map_err(PhotoError::Io)?;
            let q = quality.clamp(1, 95);
            let mut encoder = JpegEncoder::new_with_quality(file, q);
            let dynamic = image::DynamicImage::ImageRgba8(img.clone());
            encoder.encode_image(&dynamic).map_err(PhotoError::Image)?;
            Ok(())
        }
    }
}

/// Alloca un buffer di pixel grezzi (RGBA 8 bit, riga principale).
pub fn alloc_raw(w: u32, h: u32) -> Vec<u8> {
    vec![0u8; (w as usize) * (h as usize) * 4]
}

/// Converte un buffer raw in immagine bufferizzata RGBA.
pub fn into_image(buf: Vec<u8>, w: u32, h: u32) -> image::RgbaImage {
    image::ImageBuffer::from_raw(w, h, buf).expect("buffer con dimensioni coerenti")
}

/// Applica in parallelo una funzione a ogni pixel di un clone dell'immagine
/// (usato dalle regolazioni conservative).
pub fn map_pixels<F>(img: &image::RgbaImage, f: F) -> image::RgbaImage
where
    F: Fn(u32, u32, [f32; 3]) -> [f32; 3] + Sync + Send,
{
    let (w, h) = img.dimensions();
    let row_len = w as usize * 4;
    let mut buf = alloc_raw(w, h);
    let f = &f;
    buf.par_chunks_mut(row_len)
        .enumerate()
        .for_each(|(row_idx, row_slice)| {
            let y = row_idx as u32;
            for x in 0..w as usize {
                let src = img.get_pixel(x as u32, y);
                let rgb = [src[0] as f32, src[1] as f32, src[2] as f32];
                let c = f(x as u32, y, rgb);
                let idx = x * 4;
                row_slice[idx] = clamp8(c[0]);
                row_slice[idx + 1] = clamp8(c[1]);
                row_slice[idx + 2] = clamp8(c[2]);
                row_slice[idx + 3] = src[3];
            }
        });
    into_image(buf, w, h)
}

#[inline]
fn clamp8(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}
