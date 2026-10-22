//! Rimozione dello sfondo: zona connessa dal colore simile a quella del pixel cliccato.

use std::collections::VecDeque;

use image::RgbaImage;

/// Rimuove (valore alpha = 0) l'area connessa connessa vicina al colore
/// `seed` a distanza <= `limit`, con BFS partendo da `(sx, sy)`.
///
/// `tolerance` in [0, 100]: `limit` viene calcolato rispetto alla
/// massima distanza RGB possibile tra due pixel.
pub fn remove_background(img: &RgbaImage, seed: (u32, u32), tolerance: f32) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    let mut out = img.clone();
    if iw == 0 || ih == 0 {
        return out;
    }
    let sx = seed.0.min(iw - 1);
    let sy = seed.1.min(ih - 1);
    let anchor = img.get_pixel(sx, sy);
    let tol = tolerance.clamp(0.0, 100.0);
    let max_dist = (tol / 100.0) * (255.0_f32 * 3.0).sqrt();

    // distanza dall'ancore, in binario magari: vedi documentazione con i canali
    let visit = |p: &image::Rgba<u8>| -> bool {
        let dist = ((p[0] as f32 - anchor[0] as f32).powi(2)
            + (p[1] as f32 - anchor[1] as f32).powi(2)
            + (p[2] as f32 - anchor[2] as f32).powi(2))
        .sqrt();
        dist <= max_dist
    };

    let mut visited = vec![false; (iw as usize) * (ih as usize)];
    let mut queue = VecDeque::with_capacity(1024);
    let start = sy as usize * iw as usize + sx as usize;
    if !visit(img.get_pixel(sx, sy)) {
        return out;
    }
    visited[start] = true;
    queue.push_back(start);

    while let Some(idx) = queue.pop_front() {
        let x = (idx % iw as usize) as u32;
        let y = (idx / iw as usize) as u32;
        out.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
        if x > 0 {
            let n = idx - 1;
            if !visited[n] && visit(img.get_pixel(x - 1, y)) {
                visited[n] = true;
                queue.push_back(n);
            }
        }
        if x + 1 < iw {
            let n = idx + 1;
            if !visited[n] && visit(img.get_pixel(x + 1, y)) {
                visited[n] = true;
                queue.push_back(n);
            }
        }
        if y > 0 {
            let n = idx - iw as usize;
            if !visited[n] && visit(img.get_pixel(x, y - 1)) {
                visited[n] = true;
                queue.push_back(n);
            }
        }
        if y + 1 < ih {
            let n = idx + iw as usize;
            if !visited[n] && visit(img.get_pixel(x, y + 1)) {
                visited[n] = true;
                queue.push_back(n);
            }
        }
    }
    out
}

/// Rimozione automatica: individua il colore dello sfondo dal perimetro
/// della foto (mediana dei pixel del bordo) e rimuove la zona ad esso
/// connessa che tocca il bordo. Non serve cliccare niente.
pub fn remove_background_auto(img: &RgbaImage, tolerance: f32) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 {
        return img.clone();
    }

    // Campiona il perimetro e ricava il colore tipico (mediana per canale).
    let mut campioni: Vec<[f32; 3]> = Vec::new();
    let passo_x = (iw / 40).max(1);
    let passo_y = (ih / 40).max(1);
    for x in (0..iw).step_by(passo_x as usize) {
        campioni.push(rgb_di(img.get_pixel(x, 0)));
        campioni.push(rgb_di(img.get_pixel(x, ih - 1)));
    }
    for y in (0..ih).step_by(passo_y as usize) {
        campioni.push(rgb_di(img.get_pixel(0, y)));
        campioni.push(rgb_di(img.get_pixel(iw - 1, y)));
    }
    let canale = |i: usize| -> u8 {
        let mut v: Vec<f32> = campioni.iter().map(|c| c[i]).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = v.len() / 2;
        v[mid].round().clamp(0.0, 255.0) as u8
    };
    let fondo: [f32; 3] = [canale(0) as f32, canale(1) as f32, canale(2) as f32];

    // Rimuove tutti i pixel del perimetro simili al colore di fondo
    // e l'area interna ad esso connessa.
    let tol = tolerance.clamp(0.0, 100.0);
    let max_dist = (tol / 100.0) * (255.0_f32 * 3.0).sqrt();
    let simile = |p: &image::Rgba<u8>| -> bool {
        let c = rgb_di(p);
        let d = ((c[0] - fondo[0]).powi(2) + (c[1] - fondo[1]).powi(2) + (c[2] - fondo[2]).powi(2))
            .sqrt();
        d <= max_dist
    };

    let mut out = img.clone();
    let mut visited = vec![false; (iw as usize) * (ih as usize)];
    let mut queue = VecDeque::with_capacity(1024);
    perimetro(iw, ih, |x, y| {
        let idx = y as usize * iw as usize + x as usize;
        if visited[idx] {
            return;
        }
        if simile(img.get_pixel(x, y)) {
            visited[idx] = true;
            queue.push_back(idx);
        }
    });

    while let Some(idx) = queue.pop_front() {
        let x = (idx % iw as usize) as u32;
        let y = (idx / iw as usize) as u32;
        out.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
        espandi(
            Coda {
                iw,
                ih,
                img,
                visited: &mut visited,
                queue: &mut queue,
                simile: &simile,
            },
            x,
            y,
        );
    }
    out
}

fn rgb_di(p: &image::Rgba<u8>) -> [f32; 3] {
    [p[0] as f32, p[1] as f32, p[2] as f32]
}

fn perimetro(iw: u32, ih: u32, mut on_pixel: impl FnMut(u32, u32)) {
    for x in 0..iw {
        on_pixel(x, 0);
        on_pixel(x, ih - 1);
    }
    for y in 1..ih - 1 {
        on_pixel(0, y);
        on_pixel(iw - 1, y);
    }
}

fn espandi(ctx: Coda, x: u32, y: u32) {
    let Coda {
        iw,
        ih,
        img,
        visited,
        queue,
        simile,
    } = ctx;
    let mut push_match = |nx: u32, ny: u32| {
        let n = ny as usize * iw as usize + nx as usize;
        if !visited[n] && simile(img.get_pixel(nx, ny)) {
            visited[n] = true;
            queue.push_back(n);
        }
    };
    if x > 0 {
        push_match(x - 1, y);
    }
    if x + 1 < iw {
        push_match(x + 1, y);
    }
    if y > 0 {
        push_match(x, y - 1);
    }
    if y + 1 < ih {
        push_match(x, y + 1);
    }
}

/// Dati mutabili condivisi della ricerca in ampiezza della rimozione.
struct Coda<'a> {
    iw: u32,
    ih: u32,
    img: &'a RgbaImage,
    visited: &'a mut [bool],
    queue: &'a mut VecDeque<usize>,
    simile: &'a (dyn Fn(&image::Rgba<u8>) -> bool + 'a),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> RgbaImage {
        image::ImageBuffer::from_fn(10, 10, |_, _| image::Rgba([0, 0, 0, 255]))
    }

    #[test]
    fn zona_adiacente_al_seed_entrata() {
        let mut img = base();
        for x in 0..2 {
            for y in 0..2 {
                img.put_pixel(x, y, image::Rgba([250, 250, 250, 255]));
            }
        }
        let out = remove_background(&img, (0, 0), 20.0);
        assert_eq!(out.get_pixel(0, 0)[3], 0, "chiara adiacente rimossa");
        assert_eq!(out.get_pixel(5, 5)[3], 255, "fondo nero resta opaco");
    }

    #[test]
    fn zona_isolata_non_rimossa() {
        let mut img = base();
        img.put_pixel(0, 0, image::Rgba([250, 250, 250, 255]));
        img.put_pixel(9, 9, image::Rgba([250, 250, 250, 255]));
        let out = remove_background(&img, (0, 0), 20.0);
        assert_eq!(out.get_pixel(0, 0)[3], 0, "seed rimosso");
        assert_eq!(out.get_pixel(9, 9)[3], 255, "l'isolato non e' raggiunto");
    }

    #[test]
    fn automatica_rimuove_fondo_bordo() {
        let mut img = base(); // fondo nero al bordo
        for x in 3..7 {
            for y in 3..7 {
                img.put_pixel(x, y, image::Rgba([250, 250, 250, 255]));
            }
        }
        let out = remove_background_auto(&img, 15.0);
        assert_eq!(out.get_pixel(0, 0)[3], 0, "bordo nero tolto");
        assert_eq!(out.get_pixel(5, 5)[3], 255, "soggetto bianco resta");
    }
}
