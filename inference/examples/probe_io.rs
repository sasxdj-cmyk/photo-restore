//! Sonda temporanea: stampa input/output di un modello ONNX.
//! Uso: cargo run --example probe_io -- <dll> <modello>

use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("uso: probe_io <dll> <modello>");
        std::process::exit(2);
    }
    let dll = Path::new(&args[1]);
    let model = Path::new(&args[2]);

    pr_inference::engine::init_runtime(dll)?;
    let env = pr_inference::engine::environment()?;
    let session = ort::session::Session::builder(env)?.commit_from_file(model)?;

    println!("--- INPUT ---");
    for i in session.inputs() {
        println!("nome={} tipo={:?}", i.name(), i.dtype());
    }
    println!("--- OUTPUT ---");
    for o in session.outputs() {
        println!("nome={} tipo={:?}", o.name(), o.dtype());
    }
    Ok(())
}
