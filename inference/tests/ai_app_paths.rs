//! Test reale del percorso AI dell'app: runtime DLL, download modello,
//! verifica SHA-256 e upscaling con le stesse chiamate usate dai comandi.
//!
//! Eseguire con: cargo test --target x86_64-pc-windows-gnu -p photo-restore-app --test ai_paths
//! (usa %LOCALAPPDATA%\PhotoRestore come l'app: richiede rete la prima volta).

use std::path::PathBuf;

use image::RgbaImage;

fn dll() -> PathBuf {
    pr_inference::mgmt::paths()
        .runtime_dir
        .join(pr_inference::mgmt::RUNTIME_DLL)
}

#[test]
fn runtime_modello_e_upscale_real_app_path() {
    // 1. stato iniziale
    let stati = pr_inference::mgmt::statuses();
    for s in &stati {
        eprintln!(
            "-- {} {}: installato={} runtime={}",
            s.id, s.name, s.installed, s.runtime_ready
        );
    }
    assert_eq!(
        stati.len(),
        9,
        "due x2/x4 + due di segmentazione + due retinaface + codeformer + gfpgan + ultrasharp"
    );
    assert_eq!(stati.iter().filter(|s| s.kind == "upscale").count(), 3);
    assert_eq!(stati.iter().filter(|s| s.kind == "volto").count(), 4);

    // 2. runtime DLL (scarica se assente: idem pulsante UI)
    if !dll().exists() {
        let dll_path = pr_inference::mgmt::download_runtime(|done, total| {
            let _ = (done, total);
        })
        .expect("download runtime riuscito");
        assert!(dll_path.exists());
    }
    assert!(dll().exists(), "DLL deve essere presente ora");

    // 3. modello x2: download + verifica SHA (idem pulsante UI)
    let m = pr_inference::mgmt::find_model("realesrgan-x2plus").expect("registro");
    let (path, sha) =
        pr_inference::mgmt::download_model(m, |_, _| {}).expect("download/modellazione");
    eprintln!(
        "-- modello in {} sha={}",
        path.display(),
        &sha[..12.min(sha.len())]
    );
    pr_inference::mgmt::verify_model(m).expect("verifica sha");

    // 4. upscale reale con tiling di produzione (256/16)
    pr_inference::engine::init_runtime(&dll()).expect("runtime onnx");
    let img = make_test_image(96, 64);
    let mut up = pr_inference::engine::Upscaler::new(&path, 2).expect("sessione");
    up.set_tile(256, 16);
    let out = up
        .upscale(&img, &|done, tot| {
            eprintln!("-- blocco {done}/{tot}");
            assert!(done <= tot);
        })
        .expect("upscaling riuscito");
    assert_eq!(out.dimensions(), (192, 128));
}

#[test]
fn stato_senza_sessione_imported_false() {
    // tutti i modelli hanno licenza e tipo dichiarati
    let stati = pr_inference::mgmt::statuses();
    assert!(stati
        .iter()
        .all(|s| !s.license.is_empty() && !s.kind.is_empty()));
    assert!(stati
        .iter()
        .filter(|s| s.kind == "upscale")
        .all(|s| s.license.contains("BSD-3-Clause") || s.license.contains("CC BY-NC-SA")));
    assert!(stati.iter().any(|s| s.kind == "sfondo"));
    assert!(stati.iter().any(|s| s.kind == "volto"));
    assert!(stati
        .iter()
        .filter(|s| s.kind == "volto")
        .any(|s| s.license.contains("MIT")));
    assert!(stati
        .iter()
        .any(|s| s.license.contains("S-Lab")));
    assert!(stati
        .iter()
        .any(|s| s.license.contains("Apache-2.0")));
    assert!(stati
        .iter()
        .any(|s| s.license.contains("CC BY-NC-SA")));
}

fn make_test_image(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let n = (x as u16 * 7 + y as u16 * 13) as u8;
        image::Rgba([
            n,
            (n as u16 * 2 % 256) as u8,
            (128 + n as u16 / 2) as u8,
            255,
        ])
    })
}
