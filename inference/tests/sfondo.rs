//! Test con dataset reale della rimozione dello sfondo via U-2-Netp.
//!
//! Dipende dalle variabili:
//!   PR_TEST_RUNTIME_DLL  -> percorso di onnxruntime.dll
//!   PR_TEST_BG_MODEL     -> percorso di u2netp.onnx

use std::path::PathBuf;

fn env_path(name: &str) -> Option<PathBuf> {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => Some(PathBuf::from(v)),
        _ => None,
    }
}

/// Scena sintetica: parete con leggera trama + fotografia attaccata al centro.
fn scena() -> image::RgbaImage {
    image::ImageBuffer::from_fn(320, 320, |x, y| {
        let dentro_x = (60..260).contains(&x);
        let dentro_y = (50..270).contains(&y);
        if dentro_x && dentro_y {
            image::Rgba([
                (30 + (x * 3) % 80).clamp(0, 255) as u8,
                (60 + (y * 2) % 120).clamp(0, 255) as u8,
                (120 + (x * y) % 90).clamp(0, 255) as u8,
                255,
            ])
        } else {
            // parete grigia chiarissima con una tenue granulosita'
            let n = (x as i32 * 13 + y as i32 * 7) % 5;
            image::Rgba([(228 + n) as u8, (226 + n) as u8, (222 + n) as u8, 255])
        }
    })
}

#[test]
fn sfondo_rimosso_ubi_errati_oggetto_restato() {
    let Some(dll) = env_path("PR_TEST_RUNTIME_DLL") else {
        eprintln!("# variabili PR_TEST_* non impostate: test saltato");
        return;
    };
    pr_inference::engine::init_runtime(&dll).expect("runtime");
    let modello = env_path("PR_TEST_BG_MODEL").expect("imposta PR_TEST_BG_MODEL");
    let mut seg = pr_inference::seg::Segmenter::new(&modello).expect("segmenter");
    let img = scena();
    let out = seg.rimuovi_sfondo(&img).expect("elaborazione");

    // soggetto: alpha buona; parete: quasi trasparente
    let a = |x: u32, y: u32| out.get_pixel(x, y)[3];
    let in_soggetto = [a(160, 160), a(100, 150), a(220, 250)];
    let in_parete = [a(10, 10), a(305, 300), a(30, 250)];
    assert!(
        in_soggetto.iter().all(|v| *v > 200),
        "oggetto troppo svuotato: {in_soggetto:?}"
    );
    assert!(
        in_parete.iter().all(|v| *v < 80),
        "parete non rimossa del tutto: {in_parete:?}"
    );
}
