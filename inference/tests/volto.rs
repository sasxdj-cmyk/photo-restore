//! Test con ritratto reale per RetinaFace + CodeFormer (path dell'app).
//!
//! Dipende dalle variabili (se assenti il test viene saltato):
//!   PR_TEST_RUNTIME_DLL  -> percorso di onnxruntime.dll
//!   PR_TEST_VOLTO_IMG    -> fotografia con un volto ben visibile
//!   PR_TEST_RETINAFACE   -> percorso di retinaface_r50.onnx
//!   PR_TEST_CODEFORMER   -> percorso di codeformer_v01.onnx

use std::path::PathBuf;

fn env_path(name: &str) -> Option<PathBuf> {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => Some(PathBuf::from(v)),
        _ => None,
    }
}

fn ritratto() -> image::RgbaImage {
    image::open(env_path("PR_TEST_VOLTO_IMG").expect("imposta PR_TEST_VOLTO_IMG"))
        .expect("immagine di prova")
        .to_rgba8()
}

fn punti_coesi(img: &image::RgbaImage, v: &pr_inference::faces::Face) {
    // il riquadro resta dentro l'immagine
    assert!(v.x >= 0.0 && v.y >= 0.0);
    assert!((v.x + v.w) <= img.width() as f32 + 4.0, "fuori a destra: {v:?}");
    assert!(
        (v.y + v.h) <= img.height() as f32 + 4.0,
        "fuori in basso: {v:?}"
    );
    // coerenza dei punti: gli occhi sopra il naso, il naso sopra la bocca
    let [re, le, nt, rcm, lcm] = v.points;
    assert!(re[1] < nt[1] && le[1] < nt[1], "occhi sotto il naso: {v:?}");
    assert!(nt[1] < rcm[1] && nt[1] < lcm[1], "bocca sopra il naso: {v:?}");
    let separazione = (le[0] - re[0]).abs();
    assert!(
        separazione > v.w * 0.25,
        "occhi troppo vicini: {separazione} su larghezza {}",
        v.w
    );
}

fn statistiche_backup(
    img: &image::RgbaImage,
    out: &image::RgbaImage,
    volto: &pr_inference::faces::Face,
) -> (f64, f64) {
    use pr_inference::faces::{apply, fit_similarity, invert};
    let mut dentro = 0.0f64;
    let mut n_dentro = 0usize;
    let mut fuori = 0.0f64;
    let mut n_fuori = 0usize;
    let cx = volto.x + volto.w / 2.0;
    let cy = volto.y + volto.h / 2.0;
    // riquadro effettivo della pasta (proiezione degli angoli del modello)
    let tpl: Vec<[f32; 2]> = pr_inference::faces::TEMPLATE_FFHQ
        .iter()
        .map(|p| [p[0] * 512.0, p[1] * 512.0])
        .collect();
    let to_model = fit_similarity(volto.points.as_ref(), &tpl);
    let to_image = invert(&to_model);
    let side = pr_inference::faces::SIDE_MODEL as f64;
    let corners = [
        apply(&to_image, 0.0, 0.0),
        apply(&to_image, side, 0.0),
        apply(&to_image, side, side),
        apply(&to_image, 0.0, side),
    ];
    let minx = corners.iter().fold(f64::MAX, |m, c| m.min(c.0));
    let maxx = corners.iter().fold(f64::MIN, |m, c| m.max(c.0));
    let miny = corners.iter().fold(f64::MAX, |m, c| m.min(c.1));
    let maxy = corners.iter().fold(f64::MIN, |m, c| m.max(c.1));
    let in_pasta = |x: u32, y: u32| {
        x as f64 >= minx - 2.0
            && x as f64 <= maxx + 2.0
            && y as f64 >= miny - 2.0
            && y as f64 <= maxy + 2.0
    };
    for (x, y, p) in out.enumerate_pixels() {
        let q = img.get_pixel(x, y);
        let d = (p[0] as i32 - q[0] as i32).abs()
            + (p[1] as i32 - q[1] as i32).abs()
            + (p[2] as i32 - q[2] as i32).abs();
        let dentro_volto = ((x as f32 - cx).abs() < volto.w * 0.3)
            && ((y as f32 - cy).abs() < volto.h * 0.3);
        if dentro_volto {
            dentro += d as f64;
            n_dentro += 1;
        } else if !in_pasta(x, y) {
            fuori += d as f64;
            n_fuori += 1;
        }
    }
    (dentro / n_dentro.max(1) as f64, fuori / n_fuori.max(1) as f64)
}

/// RetinaFace R50 + CodeFormer: rilevamento e ripristino equivalenti.
#[test]
fn codeformer_resta_nell_area_del_volto() {
    let Some(dll) = env_path("PR_TEST_RUNTIME_DLL") else {
        eprintln!("saltato: imposta PR_TEST_RUNTIME_DLL ecc. per il test reale");
        return;
    };
    pr_inference::engine::init_runtime(&dll).expect("runtime");
    let img = ritratto();
    let mut det = pr_inference::faces::FaceDetector::new(
        &env_path("PR_TEST_RETINAFACE").expect("imposta PR_TEST_RETINAFACE"),
    )
    .expect("retinaface");
    let volti = det.detect(&img).expect("rilevamento");
    assert!(!volti.is_empty(), "nessun volto trovato con RetinaFace");
    let v = &volti[0];
    assert!(v.score > 0.7, "punteggio basso: {}", v.score);
    punti_coesi(&img, v);
    let mut rs = pr_inference::faces::FaceRestorer::new(
        &env_path("PR_TEST_CODEFORMER").expect("imposta PR_TEST_CODEFORMER"),
    )
    .expect("codeformer");
    assert!(rs.codeformer, "variante codeformer non individuata");
    let out = rs.ripristina(&img, v).expect("ripristino");
    assert_eq!(out.dimensions(), img.dimensions());
    let (dentro, fuori) = statistiche_backup(&img, &out, v);
    assert!(dentro > 1.0, "volto invariato: diff media {dentro}");
    assert!(fuori < 0.01, "sfondo alterato: diff media {fuori}");
}
