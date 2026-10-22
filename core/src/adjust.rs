//! Regolazioni fotografiche conservative.
//!
//! Tutte le funzioni lavorano sui pixel presenti nell'immagine: non generano
//! contenuto nuovo, non riempiono aree mancanti, non ricostruiscono particolari.
//! Ogni filtro e' reversibile perche' applicato alla fotocopia in memoria troppo
//! (l'undo a livello di app mantiene la fotografia originale).

use crate::error::{PhotoError, Result};
use crate::io::map_pixels;
use image::RgbaImage;
use rayon::prelude::*;

/// Parametri delle correzioni conservative.
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct AdjustParams {
    /// -100..100: -100 = tutto nero, +100 = tutto bianco.
    pub brightness: i32,
    /// -100..100: -100 = contrasto zero, +100 = contrasto massimo.
    pub contrast: i32,
    /// -100..100: 0 = invariata, -100 = bianco e nero, +100 = saturazione massima.
    pub saturation: i32,
    /// Riduzione rumore 0 = disattiva, 1 = debole, 2 = media (mediana 3/5).
    pub noise_reduction: u8,
    /// Correzione automatica di dominanti cromatiche (gray-world).
    pub auto_white_balance: bool,
    /// Conversione in bianco e nero.
    pub grayscale: bool,
}

impl AdjustParams {
    /// Verifica che i parametri siano nell'intervallo ammesso.
    pub fn validate(&self) -> Result<()> {
        let ok = (-100..=100).contains(&self.brightness)
            && (-100..=100).contains(&self.contrast)
            && (-100..=100).contains(&self.saturation)
            && self.noise_reduction <= 2;
        if !ok {
            return Err(PhotoError::Input("regolazioni fuori intervallo".into()));
        }
        Ok(())
    }
}

/// Applica il set di correzioni richiesto.
pub fn apply(img: &RgbaImage, p: &AdjustParams) -> Result<RgbaImage> {
    p.validate()?;
    let mut out = img.clone();

    if p.noise_reduction > 0 {
        out = median_denoise(&out, p.noise_reduction);
    }
    if p.auto_white_balance {
        out = gray_world_wb(&out);
    }
    if p.brightness != 0 || p.contrast != 0 || p.saturation != 0 || p.grayscale {
        out = tone(&out, p.brightness, p.contrast, p.saturation, p.grayscale);
    }
    Ok(out)
}

/// Applica luminosita', contrasto, saturazione e conversione B/N.
pub fn tone(
    img: &RgbaImage,
    brightness: i32,
    contrast: i32,
    saturation: i32,
    grayscale: bool,
) -> RgbaImage {
    // Luminosita': offset da -128 a +128.
    let b_off = (brightness as f32 / 100.0) * 128.0;
    // Contrasto: fattore moltiplicativo attorno al punto medio 128 (mirroring OpenCV).
    let c_f = (100.0 + contrast as f32) / 100.0;
    let c_f = c_f * c_f;
    // Saturazione: fattore 0..2 attorno al valore medio.
    let s_f = (100 + saturation) as f32 / 100.0;

    map_pixels(img, move |_, _, rgb| {
        let luma = if grayscale {
            let y = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
            [y, y, y]
        } else if saturation.abs() <= 2 {
            rgb
        } else {
            let y = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
            [y, y, y].map(|y| y + (rgb[0] - y) * s_f)
        };

        luma.map(|c| 128.0 + (c + b_off - 128.0) * c_f)
    })
}

/// Correzione dominanti: gray-world, scala i canali per avere media uguale.
pub fn gray_world_wb(img: &RgbaImage) -> RgbaImage {
    let mut pixels = 0.0f64;
    let mut sums = [0.0f64; 3];
    for p in img.pixels() {
        if p[3] == 0 {
            continue;
        }
        pixels += 1.0;
        sums[0] += p[0] as f64;
        sums[1] += p[1] as f64;
        sums[2] += p[2] as f64;
    }
    if pixels == 0.0 {
        return img.clone();
    }
    let means = [sums[0] / pixels, sums[1] / pixels, sums[2] / pixels];
    let target = means.iter().cloned().sum::<f64>() / 3.0;
    let gains: [f32; 3] = means.map(|m| if m > 1.0 { (target / m) as f32 } else { 1.0 });

    map_pixels(img, move |_, _, rgb| {
        [rgb[0] * gains[0], rgb[1] * gains[1], rgb[2] * gains[2]]
    })
}

/// Riduzione rumore con filtro mediano (0 disattivato, 1 raggio 1, 2 raggio 2).
pub fn median_denoise(img: &RgbaImage, strength: u8) -> RgbaImage {
    let radius = match strength {
        1 => 1,
        2 => 2,
        _ => return img.clone(),
    };
    // Il filtro mediano non e' noto per raggio > 2: uso due passaggi raggio 1
    // che e' la tecnica piu' semplice e corretta per la mediana "double-smoothing".
    let mut out = apply_median(img, radius);
    if strength >= 2 && radius == 1 {
        out = apply_median(&out, 1);
    }
    out
}

fn apply_median(img: &RgbaImage, radius: u32) -> RgbaImage {
    // Filtro mediano per canale implementato direttamente e in parallelo per righe.
    let (w, h) = img.dimensions();
    let row_len = w as usize * 4;
    let mut buf = crate::io::alloc_raw(w, h);
    let radius_i = radius as i64;
    buf.par_chunks_mut(row_len)
        .enumerate()
        .for_each(|(row_idx, row_slice)| {
            let y = row_idx as i64;
            for x in 0..w as i64 {
                let src = img.get_pixel(x as u32, y as u32).0;
                let idx = x as usize * 4;
                for c in 0..3 {
                    let mut vals = [0u8; 25];
                    let mut n = 0usize;
                    for dy in -radius_i..=radius_i {
                        let sy = (y + dy).clamp(0, h as i64 - 1) as u32;
                        for dx in -radius_i..=radius_i {
                            let sx = (x + dx).clamp(0, w as i64 - 1) as u32;
                            vals[n] = img.get_pixel(sx, sy)[c];
                            n += 1;
                        }
                    }
                    vals[..n].sort_unstable();
                    row_slice[idx + c] = vals[(n - 1) / 2];
                }
                row_slice[idx + 3] = src[3];
            }
        });
    crate::io::into_image(buf, w, h)
}
