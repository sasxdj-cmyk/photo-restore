//! Rimozione dello sfondo con un modello di segmentazione (U-2-Net in ONNX).
//!
//! Il modello individua in un passaggio unico l'oggetto principale e
//! produce una mappa di probabilita' che diventa il canale alpha.
//! Offline: pesi eseguiti con ONNX Runtime, nessun servizio in rete.

use std::path::Path;

use image::RgbaImage;
use ndarray::Array4;
use ort::session::Session;
use ort::value::TensorRef;

use crate::error::{InferenceError, Result};

/// Lato del tensore d'ingresso di U-2-Net / U-2-Netp.
const INPUT_SIDE: u32 = 320;

/// Media e deviazione di ImageNet per la normalizzazione dell'input (RGB 0..255).
const MEAN: [f32; 3] = [123.675, 116.28, 103.53];
const STD: [f32; 3] = [58.395, 57.12, 57.375];

pub struct Segmenter {
    session: Session,
    input_name: String,
    output_name: String,
}

impl Segmenter {
    pub fn new(model_path: &Path) -> Result<Segmenter> {
        if !model_path.exists() {
            return Err(InferenceError::MissingModel(
                model_path.display().to_string(),
            ));
        }
        let env = crate::engine::environment()?;
        let session = Session::builder(env)?.commit_from_file(model_path)?;
        let input_name = session
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .ok_or_else(|| InferenceError::Model("modello senza ingressi".into()))?;
        let output_name = session
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .ok_or_else(|| InferenceError::Model("modello senza uscite".into()))?;
        Ok(Segmenter {
            session,
            input_name,
            output_name,
        })
    }

    /// Svuota lo sfondo: alpha Zero dove la mappa non vede il soggetto.
    ///
    /// Operazione un-passaggio: non richiede progresso, ma resta annullabile
    /// come tutte le modifiche (snapshot nello storico della sessione).
    pub fn rimuovi_sfondo(&mut self, img: &RgbaImage) -> Result<RgbaImage> {
        let (iw, ih) = img.dimensions();
        if iw == 0 || ih == 0 {
            return Ok(img.clone());
        }
        let mask = self.maschera(img, INPUT_SIDE, INPUT_SIDE)?;
        // maschera riportata alle dimensioni originali
        let mask_up = image::imageops::resize(&mask, iw, ih, image::imageops::FilterType::Triangle);

        // 1. soglia: sotto il 30% della mappa normalizzata e' sicuramente
        //    sfondo (niente veli); sopra e' soggetto pieno
        let mut binaria = image::GrayImage::new(iw, ih);
        for (x, y, p) in mask_up.enumerate_pixels() {
            let a = if p[0] as f32 >= 0.30 * 255.0 { 255u8 } else { 0u8 };
            binaria.put_pixel(x, y, image::Luma([a]));
        }
        // 2. piumatura: bordo morbido senza dentelli
        let morbida = image::imageops::blur(&binaria, 1.2);

        // 3. stima del colore di fondo: mediana dei pixel trasparenti che
        //    toccano il soggetto (la frangia esterna)
        let alfa = |x: u32, y: u32| morbida.get_pixel(x, y)[0];
        let mut bordo: Vec<[f32; 3]> = Vec::new();
        for (x, y, p) in morbida.enumerate_pixels() {
            if p[0] != 0 {
                continue;
            }
            let toccato = (x > 0 && alfa(x - 1, y) != 0)
                || (x + 1 < iw && alfa(x + 1, y) != 0)
                || (y > 0 && alfa(x, y - 1) != 0)
                || (y + 1 < ih && alfa(x, y + 1) != 0);
            if toccato {
                let c = img.get_pixel(x, y);
                bordo.push([c[0] as f32, c[1] as f32, c[2] as f32]);
            }
        }
        let fondo = mediana(&bordo);

        // 4. composizione finale: i pixel parzialmente trasparenti perdono il
        //    contributo del colore di fondo, cosi' la frangia sparisce
        let mut out = RgbaImage::new(iw, ih);
        for (x, y, p) in img.enumerate_pixels() {
            let t = alfa(x, y) as f32 / 255.0;
            let mut c = [p[0], p[1], p[2]];
            if let Some(b) = fondo {
                if t > 0.0 && t < 1.0 {
                    for k in 0..3 {
                        c[k] = ((c[k] as f32 - (1.0 - t) * b[k]) / t)
                            .round()
                            .clamp(0.0, 255.0) as u8;
                    }
                }
            }
            out.put_pixel(x, y, image::Rgba([c[0], c[1], c[2], (t * 255.0) as u8]));
        }
        Ok(out)
    }

    /// Maschera di segmentazione in scala di grigio (u8) alle dimensioni richieste.
    fn maschera(&mut self, img: &RgbaImage, side_w: u32, side_h: u32) -> Result<image::GrayImage> {
        let resized =
            image::imageops::resize(img, side_w, side_h, image::imageops::FilterType::Triangle);
        let (w, h) = (side_w as usize, side_h as usize);
        let mut input = Array4::<f32>::zeros((1, 3, h, w));
        for ty in 0..h {
            for tx in 0..w {
                let p = resized.get_pixel(tx as u32, ty as u32);
                input[[0, 0, ty, tx]] = (p[0] as f32 - MEAN[0]) / STD[0];
                input[[0, 1, ty, tx]] = (p[1] as f32 - MEAN[1]) / STD[1];
                input[[0, 2, ty, tx]] = (p[2] as f32 - MEAN[2]) / STD[2];
            }
        }
        let tensor_ref = TensorRef::<f32>::from_array_view(&input)?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => tensor_ref])?;
        let (shape, data) = outputs[self.output_name.as_str()].try_extract_tensor::<f32>()?;
        if shape.len() != 4 || shape[1] != 1 {
            return Err(InferenceError::Model(format!(
                "uscita di segmentazione inattesa: forma {shape:?}"
            )));
        }
        // la mossa del modello puo' non coincidere col lato richiesto:
        // si legge solo il riquadro effettivamente presente nei dati
        let out_h = (shape[2] as usize).min(h);
        let out_w = (shape[3] as usize).min(w);
        let tot = (shape[2] as usize)
            .saturating_mul(shape[3] as usize);
        if out_h == 0 || out_w == 0 || tot > data.len() {
            return Err(InferenceError::Model("uscita corta".into()));
        }
        // normalizzazione min-max come nelle implementazioni di riferimento
        let mut ma = f32::MIN;
        let mut mi = f32::MAX;
        for &v in data.iter().take(tot.min(data.len())) {
            if v.is_finite() {
                ma = ma.max(v);
                mi = mi.min(v);
            }
        }
        if !ma.is_finite() || !mi.is_finite() || (ma - mi).abs() < 1e-6 {
            ma = 1.0;
            mi = 0.0;
        }
        let mut parziale = image::GrayImage::new(out_w as u32, out_h as u32);
        for y in 0..out_h {
            for x in 0..out_w {
                let v = (data[y * shape[3] as usize + x] - mi) / (ma - mi);
                parziale.put_pixel(
                    x as u32,
                    y as u32,
                    image::Luma([(v.clamp(0.0, 1.0) * 255.0).round() as u8]),
                );
            }
        }
        if out_w == w && out_h == h {
            Ok(parziale)
        } else {
            // la maschera del modello e' piu' piccola: si riporta al lato atteso
            Ok(image::imageops::resize(
                &parziale,
                side_w,
                side_h,
                image::imageops::FilterType::Triangle,
            ))
        }
    }
}

/// Mediana per canale dei colori campionati (None se vuota).
fn mediana(campioni: &[[f32; 3]]) -> Option<[f32; 3]> {
    if campioni.is_empty() {
        return None;
    }
    let mut out = [0.0f32; 3];
    for k in 0..3 {
        let mut v: Vec<f32> = campioni.iter().map(|c| c[k]).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        out[k] = v[v.len() / 2];
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::Segmenter;

    // Test manuale con dataset reale: vedi tests/sfondo.rs
    // (scarica il modello con tools/scarica-ai-test.js)

    #[test]
    fn costruttore_rifiuta_percorso_assente() {
        let r = Segmenter::new(std::path::Path::new("nessun-modello.onnx"));
        assert!(r.is_err());
    }
}
