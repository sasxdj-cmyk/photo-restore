//! Rilevamento automatico di fotografie dentro una scansione.
//!
//! Strategia tradizionale (nessun AI): scala di lavoro ridotta, sfocatura
//! per uniformare lo sfondo, soglia di Otsu, chiusura morfologica per
//! riunire i confini, ricerca dei contorni esterni, guscio convesso e
//! estrazione dei quattro angoli.
//!
//! Il rilevamento e' una guida, non una verita': l'utente puo' ritoccare i
//! punti a mano, e il ritaglio manuale resta la via definitiva per le
//! scansioni difficili (come richiesto, non si scartano porzioni di foto
//! solo perche' presentano danni).

use image::{GrayImage, ImageBuffer, Luma, RgbaImage};
use rayon::prelude::*;

/// Quadrilatero in coordinate normalizzate 0..1, ordine TL, TR, BR, BL.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct PhotoQuad {
    pub corners: [[f32; 2]; 4],
    /// area del quad in pixel (intesa come bbox ruotata).
    pub score: f32,
}

/// Massimo numero di fotografie rilevate in una singola scansione.
pub const MAX_QUADS: usize = 12;

pub fn detect_photos(img: &RgbaImage) -> Vec<PhotoQuad> {
    let (w, h) = img.dimensions();
    if w < 32 || h < 32 {
        return Vec::new();
    }
    let scale = (1200.0f64 / w.max(h) as f64).min(1.0);
    let lw = ((w as f64 * scale).round() as u32).max(1);
    let lh = ((h as f64 * scale).round() as u32).max(1);
    let small: GrayImage = {
        let resized = image::imageops::resize(img, lw, lh, image::imageops::FilterType::Triangle);
        to_gray(&resized)
    };
    let blurred = double_box_blur(&small);

    let level = imageproc::contrast::otsu_level(&blurred) as i32;
    let border_mean = border_mean(&blurred) as i32;
    let mask: GrayImage = if border_mean >= level {
        // Bordo chiaro: fotografie nella parte scura.
        ImageBuffer::from_fn(lw, lh, |x, y| {
            let v = blurred.get_pixel(x, y)[0] as i32;
            Luma([if v <= level { 255u8 } else { 0u8 }])
        })
    } else {
        // Bordo scuro: fotografie nella parte chiara.
        ImageBuffer::from_fn(lw, lh, |x, y| {
            let v = blurred.get_pixel(x, y)[0] as i32;
            Luma([if v >= level { 255u8 } else { 0u8 }])
        })
    };

    // Chiusura morfologica per unire bordi spezzati.
    let close_r = ((lw.min(lh) / 120).clamp(2, 4) + 2) as u8;
    let dilated =
        imageproc::morphology::dilate(&mask, imageproc::distance_transform::Norm::LInf, close_r);
    let closed =
        imageproc::morphology::erode(&dilated, imageproc::distance_transform::Norm::LInf, close_r);

    let contours = imageproc::contours::find_contours::<i32>(&closed);
    let image_area = (lw * lh) as f32;
    let mut candidates: Vec<PhotoQuad> = contours
        .par_iter()
        .filter(|c| c.border_type == imageproc::contours::BorderType::Outer && c.points.len() >= 8)
        .filter_map(|c| {
            let pts: Vec<(i64, i64)> = c.points.iter().map(|p| (p.x as i64, p.y as i64)).collect();
            photo_quad_from_contour(&pts, lw, lh, image_area)
        })
        .collect();

    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates.truncate(MAX_QUADS);
    candidates
}

fn photo_quad_from_contour(
    points: &[(i64, i64)],
    lw: u32,
    lh: u32,
    image_area: f32,
) -> Option<PhotoQuad> {
    let hull = convex_hull(points.to_vec());
    if hull.len() < 4 {
        return None;
    }
    let area = polygon_area(&hull);
    if (area as f32) < image_area * 0.02 || (area as f32) > image_area * 0.96 {
        return None;
    }
    let (min_x, min_y, max_x, max_y) = hull
        .iter()
        .fold((i64::MAX, i64::MAX, i64::MIN, i64::MIN), |acc, (x, y)| {
            (acc.0.min(*x), acc.1.min(*y), acc.2.max(*x), acc.3.max(*y))
        });
    let bw = (max_x - min_x) as f32;
    let bh = (max_y - min_y) as f32;
    if bw < lw as f32 / 10.0 || bh < lh as f32 / 10.0 {
        return None;
    }
    // Non ritagliare superfici che corrono lungo piu' di un bordo della scansione.
    let mut touches = 0;
    if min_x <= 2 {
        touches += 1;
    }
    if min_y <= 2 {
        touches += 1;
    }
    if (max_x as u32) >= lw.saturating_sub(3) {
        touches += 1;
    }
    if (max_y as u32) >= lh.saturating_sub(3) {
        touches += 1;
    }
    if touches > 1 {
        return None;
    }

    // Rettangolo di area minima via caliper rotanti sul guscio convesso:
    // per lato del guscio si proiettano tutti i punti sull'asse del lato
    // e sulla sua perpendicolare, si tiene il rettangolo con area minima.
    let mut best_area = f64::INFINITY;
    let mut best: Option<[f64; 8]> = None;
    for i in 0..hull.len() {
        let a = hull[i];
        let b = hull[(i + 1) % hull.len()];
        let (dx, dy) = ((b.0 - a.0) as f64, (b.1 - a.1) as f64);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        let (nx, ny) = (-uy, ux);
        let (mut mi_a, mut ma_a, mut mi_b, mut ma_b) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for &(x, y) in &hull {
            let pa = x as f64 * ux + y as f64 * uy;
            let pb = x as f64 * nx + y as f64 * ny;
            mi_a = mi_a.min(pa);
            ma_a = ma_a.max(pa);
            mi_b = mi_b.min(pb);
            ma_b = ma_b.max(pb);
        }
        let ar = (ma_a - mi_a) * (ma_b - mi_b);
        if ar < best_area {
            best_area = ar;
            // memorizza direttrice ed estremi per la retro-proiezione
            best = Some([mi_a, ma_a, mi_b, ma_b, ux, uy, nx, ny]);
        }
    }
    let [mi_a, ma_a, mi_b, ma_b, ux, uy, nx, ny] = best?;
    let back = |pa: f64, pb: f64| -> (f64, f64) { (pa * ux + pb * nx, pa * uy + pb * ny) };
    let rect = [
        back(mi_a, mi_b),
        back(ma_a, mi_b),
        back(ma_a, ma_b),
        back(mi_a, ma_b),
    ];
    let rect_w = (ma_a - mi_a) as f32;
    let rect_h = (ma_b - mi_b) as f32;
    if rect_w < lw as f32 / 10.0 || rect_h < lh as f32 / 10.0 {
        return None;
    }

    // Ordina gli angoli del rettangolo: TL min(x+y), TR max(x-y),
    // BR max(x+y), BL min(x-y).
    let tl = *rect.iter().min_by(|p, q| (p.0 + p.1).partial_cmp(&(q.0 + q.1)).unwrap()).unwrap();
    let br = *rect.iter().max_by(|p, q| (p.0 + p.1).partial_cmp(&(q.0 + q.1)).unwrap()).unwrap();
    let tr = *rect.iter().max_by(|p, q| (p.0 - p.1).partial_cmp(&(q.0 - q.1)).unwrap()).unwrap();
    let bl = *rect.iter().min_by(|p, q| (p.0 - p.1).partial_cmp(&(q.0 - q.1)).unwrap()).unwrap();

    let to_norm = |p: (f64, f64)| -> [f32; 2] {
        [
            (p.0 as f32 / lw as f32).clamp(0.0, 1.0),
            (p.1 as f32 / lh as f32).clamp(0.0, 1.0),
        ]
    };
    let corners = [to_norm(tl), to_norm(tr), to_norm(br), to_norm(bl)];
    let quad_area = rect_w * rect_h;
    if quad_area < image_area * 0.02 {
        return None;
    }
    Some(PhotoQuad {
        corners,
        score: area as f32,
    })
}

/// Guscio convesso (monotone chain su griglia intera).
pub fn convex_hull(mut pts: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    if pts.len() < 4 {
        return pts;
    }
    pts.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    pts.dedup();
    if pts.len() < 4 {
        return pts;
    }
    fn cross(o: (i64, i64), a: (i64, i64), b: (i64, i64)) -> i64 {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    }
    let mut lower: Vec<(i64, i64)> = Vec::new();
    for p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 0 {
            lower.pop();
        }
        lower.push(*p);
    }
    let mut upper: Vec<(i64, i64)> = Vec::new();
    for p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 0 {
            upper.pop();
        }
        upper.push(*p);
    }
    let mut hull = lower;
    hull.pop();
    hull.extend(upper);
    hull.pop();
    hull
}

fn polygon_area(poly: &[(i64, i64)]) -> f64 {
    let mut area = 0.0f64;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        area += a.0 as f64 * b.1 as f64 - b.0 as f64 * a.1 as f64;
    }
    (area / 2.0).abs()
}

fn to_gray(img: &RgbaImage) -> GrayImage {
    ImageBuffer::from_fn(img.width(), img.height(), |x, y| {
        let p = img.get_pixel(x, y);
        let y = (299u32 * p[0] as u32 + 587 * p[1] as u32 + 114 * p[2] as u32) / 1000;
        Luma([y.clamp(0, 255) as u8])
    })
}

/// Box blur 3x3 applicato due volte (stabile, senza dipendere da API variabili).
fn double_box_blur(img: &GrayImage) -> GrayImage {
    box_blur(&box_blur(img))
}

fn box_blur(img: &GrayImage) -> GrayImage {
    let (w, h) = img.dimensions();
    let mut out = GrayImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0u32;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as u32;
                    let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as u32;
                    sum += img.get_pixel(sx, sy)[0] as u32;
                }
            }
            out.get_pixel_mut(x, y).0[0] = (sum / 9) as u8;
        }
    }
    out
}

/// Media della luminanza delle quattro bande perimetrali.
fn border_mean(img: &GrayImage) -> u8 {
    let (w, h) = img.dimensions();
    let band = (w.min(h) / 40).max(1);
    let mut sum = 0u64;
    let mut count = 0u64;
    for y in 0..h {
        for x in 0..w {
            if x < band || y < band || x >= w - band || y >= h - band {
                sum += img.get_pixel(x, y)[0] as u64;
                count += 1;
            }
        }
    }
    if count == 0 {
        return 0;
    }
    (sum / count) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::imageops::FilterType;

    fn synthetic_scan() -> RgbaImage {
        let mut img = RgbaImage::from_fn(600, 400, |_, _| image::Rgba([52, 52, 52, 255]));
        for y in 80..300 {
            for x in 60..280 {
                let g = 120u8.wrapping_add(((x + y) % 60) as u8);
                img.put_pixel(x, y, image::Rgba([g, g, g, 255]));
            }
        }
        for y in 40..200 {
            for x in 380..560 {
                let g = 140u8.wrapping_add(((x * y) % 50) as u8);
                img.put_pixel(x, y, image::Rgba([g, g, g, 255]));
            }
        }
        img
    }

    #[test]
    fn finds_two_photos() {
        let img = synthetic_scan();
        let quads = detect_photos(&img);
        assert!(!quads.is_empty(), "nessuna foto rilevata");
        let total: f32 = quads
            .iter()
            .map(|q| {
                let c = q.corners;
                let qw = (c[1][0] - c[0][0]).abs() * 600.0;
                let qh = (c[2][1] - c[1][1]).abs() * 400.0;
                qw * qh
            })
            .sum();
        // deve rilevare le due aree fotografiche, non l'intera scansione
        assert!(
            quads.len() >= 2,
            "attese almeno 2 foto, rilevate: {}",
            quads.len()
        );
        assert!(
            total < 600.0 * 400.0 * 0.60,
            "area rilevata troppo larga: {total}"
        );
        assert!(quads[0].score > 0.0);
        assert!(quads.len() <= MAX_QUADS);
    }

    #[test]
    fn tiny_image_is_skipped() {
        let img = RgbaImage::new(8, 8);
        assert!(detect_photos(&img).is_empty());
    }

    #[test]
    fn hull_corner_case() {
        let pts = vec![(0, 0), (4, 0), (4, 4), (0, 4), (2, 2)];
        let hull = convex_hull(pts);
        assert_eq!(hull.len(), 4);
        assert!(hull.contains(&(0, 0)));
        assert!(hull.contains(&(4, 4)));
    }

    #[test]
    fn resize_matches_trait_call() {
        let img = RgbaImage::new(20, 20);
        let _res = image::imageops::resize(&img, 10, 10, FilterType::Triangle);
    }
}
