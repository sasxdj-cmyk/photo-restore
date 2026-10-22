//! Diagnostica normalizzazioni ESRGAN: esegue il modello con ingresso [0,1]
//! e [-1,1] e salva due PNG con la varianza del laplaciano (nitidezza).
//!
//! uso: norm_probe <dll> <modello> <scala> <immagine> <prefisso>

use std::path::Path;

use pr_inference::engine::{init_runtime, Upscaler};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 6 {
        eprintln!("uso: norm_probe <dll> <modello> <scala> <immagine> <prefisso>");
        std::process::exit(1);
    }
    init_runtime(Path::new(&args[1])).expect("runtime");
    let scale: u32 = args[3].parse().expect("scala numerica");
    let img = image::open(&args[4]).expect("immagine").to_rgba8();
    let prefix = args[5].clone();
    eprintln!("ingresso: {}x{}", img.width(), img.height());
    {
        let up = Upscaler::new(Path::new(&args[2]), scale).expect("modello");
        eprintln!(
            "autodetect: {}",
            if up.norm_is_unit() { "unit" } else { "centered" }
        );
    }

    for unit in [true, false] {
        let mut up = Upscaler::new(Path::new(&args[2]), scale).expect("modello");
        up.set_tile(256, 16);
        up.force_norm(unit);
        let t0 = std::time::Instant::now();
        match up.upscale(&img, &|_, _| {}) {
            Ok(out) => {
                let lap = varianza_laplaciano(&out);
                eprintln!(
                    "unit={unit}: {:?} in {:?}, var. laplaciano {:.1}",
                    out.dimensions(),
                    t0.elapsed(),
                    lap
                );
                let nome = format!(
                    "{prefix}-{}.png",
                    if unit { "unit" } else { "centered" }
                );
                out.save(&nome).expect("salvataggio");
            }
            Err(e) => eprintln!("unit={unit}: ERRORE {e}"),
        }
    }
}

fn varianza_laplaciano(img: &image::RgbaImage) -> f64 {
    let (w, h) = (img.width(), img.height());
    if w < 3 || h < 3 {
        return 0.0;
    }
    let lum = |x: u32, y: u32| -> f64 {
        let p = img.get_pixel(x, y);
        (p[0] as f64 + p[1] as f64 + p[2] as f64) / 3.0
    };
    let mut somma = 0.0;
    let mut somma_q = 0.0;
    let mut n = 0.0;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let l = 4.0 * lum(x, y) - lum(x - 1, y) - lum(x + 1, y) - lum(x, y - 1) - lum(x, y + 1);
            somma += l;
            somma_q += l * l;
            n += 1.0;
        }
    }
    let media = somma / n;
    somma_q / n - media * media
}
