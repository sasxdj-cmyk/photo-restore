//! Probe manuale: carica un modello e stampa statistiche dell'output.
//! uso: cargo run -p pr-inference --example ai_probe -- <dll> <modello> <scala>
use image::RgbaImage;
use pr_inference::engine::{init_runtime, Upscaler};

fn main() {
    let mut args = std::env::args().skip(1);
    let dll = args.next().expect("percorso DLL");
    let model = args.next().expect("percorso modello");
    let scale: u32 = args.next().expect("scala").parse().unwrap();

    init_runtime(std::path::Path::new(&dll)).expect("DLL SessionBuilder...");
    let img = RgbaImage::from_fn(48, 32, |x, y| {
        let n = (x as u16 * 7 + y as u16 * 13) as u8;
        image::Rgba([
            n,
            (n as u16 * 2 % 256) as u8,
            (128 + n as u16 / 2) as u8,
            255,
        ])
    });

    let mut full = Upscaler::new(&std::path::PathBuf::from(&model), scale).expect("modello");
    let out = full.upscale(&img, &|_, _| {}).expect("upscaler");
    println!("full output {}x{}", out.width(), out.height());
    println!("mean_full {}", mean(&out));

    let mut tiled = Upscaler::new(&std::path::PathBuf::from(&model), scale).expect("modello");
    tiled.set_tile(32, 8);
    let out2 = tiled.upscale(&img, &|_, _| {}).expect("upscaler");
    println!("tiled output {}x{}", out2.width(), out2.height());
    println!("mean_tiled {}", mean(&out2));

    // primo quadrante di righe intere per diagnosi seame
    for y in 0..4u32 {
        for x in 0..6u32 {
            let p = out.get_pixel(x, y);
            print!("{:3} ", p[0]);
        }
        println!();
    }
}

fn mean(img: &RgbaImage) -> u32 {
    let (w, h) = img.dimensions();
    let mut total: u64 = 0;
    for p in img.pixels() {
        total += (p[0] as u64 + p[1] as u64 + p[2] as u64) / 3;
    }
    (total / (w as u64 * h as u64)) as u32
}
