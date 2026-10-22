//! Trasformazioni geometriche: ritaglio, rotazione, raddrizzamento,
//! correzione prospettica.

use image::RgbaImage;
use rayon::prelude::*;

use crate::error::{PhotoError, Result};

/// Ritaglio rettangolare.
pub fn crop(img: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> Result<RgbaImage> {
    let (iw, ih) = img.dimensions();
    if w == 0 || h == 0 {
        return Err(PhotoError::Input(
            "il ritaglio non puo' essere vuoto".into(),
        ));
    }
    if x >= iw || y >= ih {
        return Err(PhotoError::Input(
            "ritaglio fuori dai confini dell'immagine".into(),
        ));
    }
    let w = w.min(iw - x);
    let h = h.min(ih - y);
    Ok(image::imageops::crop_imm(img, x, y, w, h).to_image())
}

/// Rotazione di multipli di 90 gradi in senso orario (`quarter` positivo).
pub fn rotate_quarter(img: &RgbaImage, quarter: i32) -> RgbaImage {
    match quarter.rem_euclid(4) {
        1 => image::imageops::rotate90(img),
        2 => image::imageops::rotate180(img),
        3 => image::imageops::rotate270(img),
        _ => img.clone(),
    }
}

/// Rotazione libera in gradi (positiva = senso antiorario).
///
/// Il riquadro di destinazione e' calcolato automaticamente; gli angoli
/// vuoti restano trasparenti.
pub fn rotate_free(img: &RgbaImage, degrees: f64) -> RgbaImage {
    let angle = (degrees % 360.0).to_radians();
    let sin_abs = angle.sin().abs();
    let cos_abs = angle.cos().abs();
    let (iw, ih) = img.dimensions();
    let out_w = ((iw as f64 * cos_abs + ih as f64 * sin_abs).round() as u32).max(1);
    let out_h = ((iw as f64 * sin_abs + ih as f64 * cos_abs).round() as u32).max(1);

    // Trasformazione destinazione -> sorgente (rotazione di +angle, in
    // coordinate immagine con Y verso il basso).
    let s = angle.sin();
    let c = angle.cos();
    let cx_dst = out_w as f64 / 2.0;
    let cy_dst = out_h as f64 / 2.0;
    let cx_src = iw as f64 / 2.0;
    let cy_src = ih as f64 / 2.0;

    warp_generic_owned(img, out_w, out_h, move |dx, dy| {
        let x = dx as f64 - cx_dst;
        let y = dy as f64 - cy_dst;
        ((x * c - y * s) + cx_src, (x * s + y * c) + cy_src)
    })
}

/// Correzione prospettica: i quattro angoli sorgente [TL, TR, BR, BL]
/// (coordinate pixel nell'immagine originale) vengono proiettati nel
/// rettangolo di uscita di `out_w x out_h`.
pub fn perspective_warp(
    img: &RgbaImage,
    corners: &[[f64; 2]; 4],
    out_w: u32,
    out_h: u32,
) -> Result<RgbaImage> {
    validate_corners(corners)?;
    // Omografia diretta destinazione -> sorgente per non invertire la matrice.
    let dst_corners: [[f64; 2]; 4] = [
        [0.0, 0.0],
        [out_w as f64, 0.0],
        [out_w as f64, out_h as f64],
        [0.0, out_h as f64],
    ];
    let hmap = solve_homography(&dst_corners, corners).ok_or_else(|| {
        PhotoError::Input("proiezione prospettica degenerata (angoli collineari)".into())
    })?;

    Ok(warp_generic_owned(img, out_w, out_h, move |dx, dy| {
        let x = dx as f64;
        let y = dy as f64;
        let d = hmap[6] * x + hmap[7] * y + 1.0;
        if d.abs() < f64::EPSILON {
            return (f64::INFINITY, f64::INFINITY);
        }
        (
            (hmap[0] * x + hmap[1] * y + hmap[2]) / d,
            (hmap[3] * x + hmap[4] * y + hmap[5]) / d,
        )
    }))
}

/// Warp generico parallelizzato per righe.
fn warp_generic_owned<F>(img: &RgbaImage, out_w: u32, out_h: u32, map: F) -> RgbaImage
where
    F: Fn(u32, u32) -> (f64, f64) + Sync + Send,
{
    let (iw, ih) = img.dimensions();
    let map = &map;
    let mut buf = crate::io::alloc_raw(out_w, out_h);
    let row_len = out_w as usize * 4;
    buf.par_chunks_mut(row_len)
        .enumerate()
        .for_each(|(row_idx, row_slice)| {
            let dy = row_idx as u32;
            for dx in 0..out_w {
                let (sx, sy) = map(dx, dy);
                let p = sample_bilinear(img, sx, sy, iw, ih);
                let i = dx as usize * 4;
                row_slice[i] = p[0];
                row_slice[i + 1] = p[1];
                row_slice[i + 2] = p[2];
                row_slice[i + 3] = p[3];
            }
        });
    crate::io::into_image(buf, out_w, out_h)
}

/// Ricampionamento bilineare con alpha; punti fuori dai limiti restano trasparenti.
pub fn sample_bilinear(img: &RgbaImage, fx: f64, fy: f64, iw: u32, ih: u32) -> image::Rgba<u8> {
    if !fx.is_finite() || !fy.is_finite() {
        return image::Rgba([0, 0, 0, 0]);
    }
    if fx < -1.0 || fy < -1.0 || fx > iw as f64 || fy > ih as f64 {
        return image::Rgba([0, 0, 0, 0]);
    }
    let x0 = fx.floor() as i64;
    let y0 = fy.floor() as i64;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = fx - x0 as f64;
    let ty = fy - y0 as f64;
    let cx = |v: i64| (v.clamp(0, iw as i64 - 1)) as i32 as u32;
    let cy = |v: i64| (v.clamp(0, ih as i64 - 1)) as i32 as u32;

    let p00 = img.get_pixel(cx(x0), cy(y0)).0;
    let p10 = img.get_pixel(cx(x1), cy(y0)).0;
    let p01 = img.get_pixel(cx(x0), cy(y1)).0;
    let p11 = img.get_pixel(cx(x1), cy(y1)).0;

    let mut out = [0u8; 4];
    for c in 0..4 {
        let a = p00[c] as f64 * (1.0 - tx) + p10[c] as f64 * tx;
        let b = p01[c] as f64 * (1.0 - tx) + p11[c] as f64 * tx;
        out[c] = ((a * (1.0 - ty) + b * ty).round() as i64).clamp(0, 255) as u8;
    }
    image::Rgba(out)
}

/// Verifica che i quattro angoli siano validi (finiti, area non nulla,
/// ordinati in senso circolare).
pub fn validate_corners(corners: &[[f64; 2]; 4]) -> Result<()> {
    for c in corners {
        if !c[0].is_finite() || !c[1].is_finite() {
            return Err(PhotoError::Input("angolo non finito".into()));
        }
    }
    let area = quad_area(*corners);
    if area.abs() <= f64::EPSILON {
        return Err(PhotoError::Input(
            "area nulla: gli angoli sono collineari o coincidenti".into(),
        ));
    }
    // Convessita'/ordine circolare: le quattro crociatte devono avere lo stesso segno.
    let mut signs = [0.0f64; 4];
    for i in 0..4 {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let c = corners[(i + 2) % 4];
        let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        signs[i] = cross;
    }
    let has_pos = signs.iter().any(|s| *s > 0.0);
    let has_neg = signs.iter().any(|s| *s < 0.0);
    if has_pos && has_neg {
        return Err(PhotoError::Input(
            "angoli non ordinati: usare l'ordine alto-sinistra, alto-destra, basso-destra, basso-sinistra".into(),
        ));
    }
    Ok(())
}

/// Area con la formula del laccio.
pub fn quad_area(quad: [[f64; 2]; 4]) -> f64 {
    let mut area = 0.0;
    for i in 0..4 {
        let a = quad[i];
        let b = quad[(i + 1) % 4];
        area += a[0] * b[1] - b[0] * a[1];
    }
    area / 2.0
}

/// Risoluzione dell'omografia destinazione -> sorgente (DLT, h33 = 1).
fn solve_homography(dst: &[[f64; 2]; 4], src: &[[f64; 2]; 4]) -> Option<[f64; 8]> {
    let mut a = [[0.0f64; 8]; 8];
    let mut b = [0.0f64; 8];
    for i in 0..4 {
        let (x, y) = (dst[i][0], dst[i][1]);
        let (u, v) = (src[i][0], src[i][1]);
        a[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y];
        a[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y];
        b[2 * i] = u;
        b[2 * i + 1] = v;
    }
    for col in 0..8 {
        let mut pivot = col;
        let mut best = a[col][col].abs();
        for (row_idx, row) in a.iter().enumerate().skip(col + 1) {
            let v = row[col].abs();
            if v > best {
                best = v;
                pivot = row_idx;
            }
        }
        if best < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        let pv = a[col][col];
        let pivot_row = a[col];
        for row in col + 1..8 {
            let (arow, brow) = {
                let row_e = a[row];
                let mut new_row = row_e;
                let factor = row_e[col] / pv;
                if factor == 0.0 {
                    continue;
                }
                for c2 in col..8 {
                    new_row[c2] -= factor * pivot_row[c2];
                }
                (new_row, b[row] - factor * b[col])
            };
            a[row] = arow;
            b[row] = brow;
        }
    }
    let mut x = [0.0f64; 8];
    for row in (0..8).rev() {
        let mut acc = b[row];
        for c2 in row + 1..8 {
            acc -= a[row][c2] * x[c2];
        }
        x[row] = acc / a[row][row];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_base() {
        let img = RgbaImage::from_fn(10, 10, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let cropped = crop(&img, 2, 3, 4, 5).unwrap();
        assert_eq!(cropped.dimensions(), (4, 5));
        assert_eq!(cropped.get_pixel(0, 0).0, [2, 3, 0, 255]);
        assert!(crop(&img, 0, 0, 0, 1).is_err());
        assert!(crop(&img, 10, 0, 1, 1).is_err());
    }

    #[test]
    fn rotate_quarters() {
        let img = RgbaImage::from_fn(5, 8, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        assert_eq!(rotate_quarter(&img, 1).dimensions(), (8, 5));
        assert_eq!(rotate_quarter(&img, 2).dimensions(), (5, 8));
        assert_eq!(rotate_quarter(&img, 3).dimensions(), (8, 5));
        assert_eq!(rotate_quarter(&img, 0).dimensions(), (5, 8));
    }

    #[test]
    fn rotate_free_bounding_box() {
        let img = RgbaImage::from_fn(10, 4, |_, _| image::Rgba([10, 20, 30, 255]));
        let out = rotate_free(&img, 90.0);
        // 90 gradi: riquadro 4x10.
        assert_eq!(out.dimensions(), (4, 10));
    }

    #[test]
    fn perspective_identity_keeps_center() {
        let img = RgbaImage::from_fn(16, 12, |x, y| image::Rgba([x as u8, y as u8, 0, 255]));
        let corners = [[0.5, 0.5], [15.5, 0.5], [15.5, 11.5], [0.5, 11.5]];
        let out = perspective_warp(&img, &corners, 16, 12).unwrap();
        assert_eq!(out.dimensions(), (16, 12));
        let expected = img.get_pixel(8, 6).0;
        let got = out.get_pixel(8, 6).0;
        assert!((expected[0] as i32 - got[0] as i32).abs() <= 2);
        assert!((expected[1] as i32 - got[1] as i32).abs() <= 2);
    }

    #[test]
    fn perspective_skewed_quad() {
        let img = RgbaImage::from_fn(20, 20, |_, _| image::Rgba([255, 255, 255, 255]));
        let corners = [[2.0, 2.0], [18.0, 5.0], [16.0, 17.0], [4.0, 14.0]];
        let out = perspective_warp(&img, &corners, 32, 32).unwrap();
        // centro ricoperto dall'immagine di partenza
        assert_eq!(out.get_pixel(16, 16).0[3], 255);
    }

    #[test]
    fn validate_rejects_crossed_corners() {
        let crossed = [[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [10.0, 10.0]];
        assert!(validate_corners(&crossed).is_err());
        let ok = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        assert!(validate_corners(&ok).is_ok());
    }

    #[test]
    fn homography_forward_backward() {
        // Verifica che la DLT mappa esattamente i quattro vertici.
        let dst = [[0.0, 0.0], [100.0, 0.0], [100.0, 50.0], [0.0, 50.0]];
        let src = [[10.0, 5.0], [90.0, 6.0], [88.0, 47.0], [9.0, 45.0]];
        let h = solve_homography(&dst, &src).unwrap(); // dst -> src
        for i in 0..4 {
            let (x, y) = (dst[i][0], dst[i][1]);
            let d = h[6] * x + h[7] * y + 1.0;
            let sx = (h[0] * x + h[1] * y + h[2]) / d;
            let sy = (h[3] * x + h[4] * y + h[5]) / d;
            assert!((sx - src[i][0]).abs() < 1e-6);
            assert!((sy - src[i][1]).abs() < 1e-6);
        }
    }
}
