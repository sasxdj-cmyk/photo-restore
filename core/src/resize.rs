//! Ridimensionamento tradizionale senza AI.

use image::imageops::FilterType;
use image::RgbaImage;
use rayon::prelude::*;

use crate::error::{PhotoError, Result};

/// Metodi di ridimensionamento richiesti dal progetto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResizeMethod {
    /// Lanczos (Lanczos3).
    Lanczos,
    /// Bicubico (CatmullRom).
    Bicubic,
    /// Bilineare (Triangle).
    Bilinear,
    /// Area (media dei pixel sorgente, box-filter).
    Area,
    /// Blocco vicino.
    Nearest,
}

impl ResizeMethod {
    pub fn filter(self) -> Option<FilterType> {
        match self {
            ResizeMethod::Lanczos => Some(FilterType::Lanczos3),
            ResizeMethod::Bicubic => Some(FilterType::CatmullRom),
            ResizeMethod::Bilinear => Some(FilterType::Triangle),
            ResizeMethod::Area => None, // gestito con box-filter dedicato
            ResizeMethod::Nearest => Some(FilterType::Nearest),
        }
    }

    /// Etichetta in italiano per il pannello di informazioni.
    pub fn label(self) -> &'static str {
        match self {
            ResizeMethod::Lanczos => "Lanczos",
            ResizeMethod::Bicubic => "Bicubico",
            ResizeMethod::Bilinear => "Bilineare",
            ResizeMethod::Area => "Area (media)",
            ResizeMethod::Nearest => "Nearest-neighbor",
        }
    }
}

fn filter_of(m: ResizeMethod) -> FilterType {
    ResizeMethod::filter(m).unwrap_or(FilterType::Lanczos3)
}

/// Ridimensiona al formato esatto `target_w x target_h`.
///
/// `sharpen` e' l'intensita' finale dell'unsharp mask facoltativa (0 = disattivata,
/// valori tipici 0.2-0.6; non viene applicata velocita' sperimentale ma un sigma
/// fisso dolce da 1px). Il valore di default resta 0: nessuna nitidezza automatica.
pub fn resize(
    img: &RgbaImage,
    method: ResizeMethod,
    target_w: u32,
    target_h: u32,
    sharpen: f32,
) -> Result<RgbaImage> {
    if target_w == 0 || target_h == 0 {
        return Err(PhotoError::Input(
            "le dimensioni finali devono essere maggiori di zero".into(),
        ));
    }
    if target_w == img.width() && target_h == img.height() && sharpen <= f32::EPSILON {
        return Ok(img.clone());
    }

    let out = if method == ResizeMethod::Area {
        area_resize(img, target_w, target_h)
    } else {
        image::imageops::resize(img, target_w, target_h, filter_of(method))
    };

    if sharpen > 0.0 {
        Ok(unsharp_mask(
            &out,
            1.0,
            sharpen.clamp(0.05, 1.5) * 255.0,
            2.0,
        ))
    } else {
        Ok(out)
    }
}

/// Calcolo di larghezza/altezza finali mantenendo le proporzioni.
pub fn fitted_size(
    src_w: u32,
    src_h: u32,
    target_w: Option<u32>,
    target_h: Option<u32>,
) -> (u32, u32) {
    let (tw, th) = match (target_w, target_h) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => {
            let h = (w as f64 * src_h as f64 / src_w as f64).round() as u32;
            (w, h.max(1))
        }
        (None, Some(h)) => {
            let w = (h as f64 * src_w as f64 / src_h as f64).round() as u32;
            (w.max(1), h)
        }
        (None, None) => {
            // fattore 1: nessuna variazione
            (src_w, src_h)
        }
    };
    (tw.max(1), th.max(1))
}

/// Box-filter «Area»: media vera dei pixel sorgente coperti.
///
/// Consistente con l'idea di downsampling con media; per l'ingrandimento
/// (destinazione piu' grande della sorgente) ricade sul filtro bilineare,
/// che e' la migliore approssimazione dell'area per raggio < 1 pixel.
pub fn area_resize(img: &RgbaImage, target_w: u32, target_h: u32) -> RgbaImage {
    let (src_w, src_h) = img.dimensions();
    if target_w == 0 || target_h == 0 || src_w == 0 || src_h == 0 {
        return img.clone();
    }
    if target_w > src_w || target_h > src_h {
        return image::imageops::resize(img, target_w, target_h, FilterType::Triangle);
    }

    let scale_x = src_w as f64 / target_w as f64;
    let scale_y = src_h as f64 / target_h as f64;

    let mut buf = crate::io::alloc_raw(target_w, target_h);
    buf.par_chunks_mut(target_w as usize * 4)
        .enumerate()
        .for_each(|(ty, row)| {
            let fy0 = ty as f64 * scale_y;
            let fy1 = fy0 + scale_y;
            for tx in 0..target_w as usize {
                let fx0 = tx as f64 * scale_x;
                let fx1 = fx0 + scale_x;
                let mut acc = [0.0f64; 4];
                let ix0 = fx0.floor() as i64;
                let ix1 = (fx1.ceil() as i64).min(src_w as i64);
                let iy0 = fy0.floor() as i64;
                let iy1 = (fy1.ceil() as i64).min(src_h as i64);
                for iy in iy0..iy1 {
                    let wy = weight(fy0, fy1, iy);
                    if wy <= 0.0 {
                        continue;
                    }
                    for ix in ix0..ix1 {
                        let wx = weight(fx0, fx1, ix);
                        if wx <= 0.0 {
                            continue;
                        }
                        let p = img.get_pixel(ix as u32, iy as u32);
                        let w = wx * wy;
                        acc[0] += w * p[0] as f64;
                        acc[1] += w * p[1] as f64;
                        acc[2] += w * p[2] as f64;
                        acc[3] += w * p[3] as f64;
                    }
                }
                let alpha_area = scale_x * scale_y;
                let idx = tx * 4;
                for c in 0..4 {
                    row[idx + c] = (acc[c] / alpha_area).round().clamp(0.0, 255.0) as u8;
                }
            }
        });
    crate::io::into_image(buf, target_w, target_h)
}

/// Peso del pixel intero `i` dentro la finestra continua `[a, b)`.
#[inline]
fn weight(a: f64, b: f64, i: i64) -> f64 {
    let i0 = i.max(a.floor() as i64) as f64;
    let i1 = (i + 1).min(b.ceil() as i64) as f64;
    let lo = i0.max(a);
    let hi = i1.min(b);
    (hi - lo).max(0.0)
}

/// Unsharp mask: `out = c + amount * (c - blur(c, sigma))`, con soglia per
/// evitare di amplificare micro-rumore.
pub fn unsharp_mask(img: &RgbaImage, sigma: f32, amount: f32, threshold: f32) -> RgbaImage {
    let blurred = image::imageops::blur(img, sigma);
    RgbaImage::from_fn(img.width(), img.height(), |x, y| {
        let a = img.get_pixel(x, y);
        let b = blurred.get_pixel(x, y);
        let mut out = [0u8; 4];
        for c in 0..3 {
            let diff = a[c] as f32 - b[c] as f32;
            let add = if diff.abs() < threshold {
                0.0
            } else {
                diff * amount
            };
            out[c] = (a[c] as f32 + add).round().clamp(0.0, 255.0) as u8;
        }
        out[3] = a[3];
        image::Rgba(out)
    })
}
