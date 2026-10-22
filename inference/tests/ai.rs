//! Test di integrazione del motore AI.
//!
//! Richiede:
//!   PR_TEST_RUNTIME_DLL = percorso di onnxruntime.dll (ufficiale win-x64 1.31)
//!   PR_TEST_MODEL = percorso del modello real_esrgan_x2.onnx
//! (scaricati con lo script tools/prepar-testi-ai.cmd)

use std::path::PathBuf;

use image::RgbaImage;
use pr_inference::engine::{init_runtime, Upscaler};

fn runtime_dll() -> Option<PathBuf> {
    std::env::var_os("PR_TEST_RUNTIME_DLL").map(PathBuf::from)
}

fn model_path() -> Option<PathBuf> {
    std::env::var_os("PR_TEST_MODEL").map(PathBuf::from)
}

fn make_image(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let n = (x as u16 * 7 + y as u16 * 13) as u8;
        // gradiente morbid per non confondere l'aliasing
        image::Rgba([
            n,
            (n as u16 * 2 % 256) as u8,
            (128 + n as u16 / 2) as u8,
            255,
        ])
    })
}

#[test]
fn upscaler_x2_end_to_end() {
    let Some((dll, model)) = runtime_dll().zip(model_path()) else {
        eprintln!("# variabili PR_TEST_* non impostate: test saltato");
        return;
    };
    init_runtime(&dll).expect("DLL ONNX runtime caricata");
    let img = make_image(64, 48);
    let mut up = Upscaler::new(&model, 2).expect("modello x2 caricato");
    up.set_tile(64, 8);
    let out = up.upscale(&img, &|_, _| {}).expect("upscaler x2");
    assert_eq!(out.dimensions(), (128, 96));
    // verifiche di significato: energia media conservata entro il 25%
    let mean_in = mean_rgb(&img);
    let mean_out = mean_rgb(&out);
    let ratio = mean_out as f32 / mean_in as f32;
    assert!(
        (0.75..=1.25).contains(&ratio),
        "media luminosita' inattesa: ingresso {mean_in}, uscita {mean_out} (rapporto {ratio})"
    );
}

#[test]
fn upscaler_x4_end_to_end() {
    let Some(dll) = runtime_dll() else {
        eprintln!("# variabili PR_TEST_* non impostate: test saltato");
        return;
    };
    let Some(model) = std::env::var_os("PR_TEST_MODEL_X4").map(PathBuf::from) else {
        eprintln!("# variabili PR_TEST_* non impostate: test saltato");
        return;
    };
    init_runtime(&dll).expect("DLL ONNX runtime caricata");
    let img = make_image(48, 32);
    let mut up = Upscaler::new(&model, 4).expect("modello x4 caricato");
    up.set_tile(32, 8);
    let out = up.upscale(&img, &|_, _| {}).expect("upscaler x4");
    assert_eq!(out.dimensions(), (192, 128));
    let mean_in = mean_rgb(&img);
    let mean_out = mean_rgb(&out);
    let ratio = mean_out as f32 / mean_in as f32;
    assert!(
        (0.75..=1.25).contains(&ratio),
        "media luminosita' inattesa: ingresso {mean_in}, uscita {mean_out} (rapporto {ratio})"
    );
}

fn mean_rgb(img: &RgbaImage) -> u32 {
    let (w, h) = img.dimensions();
    let mut total: u64 = 0;
    for p in img.pixels() {
        total += (p[0] as u64 + p[1] as u64 + p[2] as u64) / 3;
    }
    (total / (w as u64 * h as u64)) as u32
}
