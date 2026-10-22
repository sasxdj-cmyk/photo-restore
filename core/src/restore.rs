//! Recupero dei dettagli di fotografie compresse con JPEG.
//!
//! Operazione deterministica e annullabile: smorzamento leggero dei blocchi
//! di compressione (artefatti "a quadretti") seguito da un inevartito
//! di nitidezza che ridiede bordi e trama. Non inventa contenuto: sfrutta
//! l'informazione gia' presente ne file.

use image::RgbaImage;

/// Massima intensita' applicabile (0 = nessun effetto, 1 = massimo).
pub const MAX_STRENGTH: f32 = 1.0;

/// Riduzione dei blocchi JPEG + recupero dei bordi.
///
/// `strength` in [0, 1]: 0.4 circa per una foto in vecchio JPEG
/// fortemente compresse, 0.9-1.0 per dimensioni piccole.
pub fn jpeg_restore(img: &RgbaImage, strength: f32) -> RgbaImage {
    let a = strength.clamp(0.0, MAX_STRENGTH);
    if a <= 0.01 {
        return img.clone();
    }

    // 1. Riduzione dei blocchi: mescolo leggermente l'originale con una
    //    sfocatura che spezza i bordi dei blocchi da 8x8.
    let blurred = image::imageops::blur(img, 0.5 + 0.9 * a);
    let softened = mix(img, &blurred, 0.30 * a);

    // 2. Recupero di nitidezza sui bordi reali (unsharp mask).
    let reference = image::imageops::blur(&softened, 1.1);
    sharpen_unsharp(&softened, &reference, 0.85 * a)
}

/// Mescola due immagini: `out = a*(1-w) + b*w` sui canali RGB.
fn mix(a: &RgbaImage, b: &RgbaImage, w: f32) -> RgbaImage {
    let (aw, ah) = a.dimensions();
    let mut out = RgbaImage::new(aw, ah);
    for (x, y, pa) in a.enumerate_pixels() {
        let pb = b.get_pixel(x, y);
        let v = |t: u8, u: u8| {
            let m = (t as f32) * (1.0 - w) + (u as f32) * w;
            m.round().clamp(0.0, 255.0) as u8
        };
        out.put_pixel(
            x,
            y,
            image::Rgba([v(pa[0], pb[0]), v(pa[1], pb[1]), v(pa[2], pb[2]), pa[3]]),
        );
    }
    out
}

/// Confortable non sharp mask: out = img + (img - ref) * amount.
fn sharpen_unsharp(img: &RgbaImage, reference: &RgbaImage, amount: f32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut out = RgbaImage::new(w, h);
    for (x, y, pi) in img.enumerate_pixels() {
        let pr = reference.get_pixel(x, y);
        let v = |t: u8, u: u8| {
            let m = t as f32 + ((t as f32 - u as f32) * amount);
            m.round().clamp(0.0, 255.0) as u8
        };
        out.put_pixel(
            x,
            y,
            image::Rgba([v(pi[0], pr[0]), v(pi[1], pr[1]), v(pi[2], pr[2]), pi[3]]),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensioni_inalterate_e_in_range() {
        let src = image::ImageBuffer::from_fn(24, 18, |x, y| {
            image::Rgba([((x * 10) % 256) as u8, (y * 7) as u8, 128, 255])
        });
        let out = jpeg_restore(&src, 0.8);
        assert_eq!(out.dimensions(), src.dimensions());
        for p in out.pixels() {
            assert_eq!(p[3], 255); // la trasparenza non viene toccata
        }
    }

    #[test]
    fn forza_zero_indifferente() {
        let src = image::ImageBuffer::from_fn(8, 8, |_, _| image::Rgba([120, 130, 140, 255]));
        let out = jpeg_restore(&src, 0.0);
        assert_eq!(out.get_pixel(4, 4).0, src.get_pixel(4, 4).0);
    }
}
