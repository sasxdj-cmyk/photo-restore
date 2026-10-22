//! Motore di upscaling AI con tiling e ricomposizione dei blocchi.

use std::path::Path;
use std::sync::OnceLock;

use image::RgbaImage;
use ort::session::Session;
use ort::value::TensorRef;

use crate::error::{InferenceError, Result};

/// Progresso: (blocchi completati, totale).
pub type ProgressFn = dyn Fn(u32, u32) + Send + Sync;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Normalization {
    /// I valori in ingresso sono [0, 1].
    Unit,
    /// I valori in ingresso sono [-1, 1] (media 0, deviazione 0,5).
    Centered,
}

/// Blocco elaborato dal modello: area sorgente in coordinate immagine.
#[derive(Debug, Clone, Copy)]
struct Block {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

static ENV: OnceLock<ort::environment::Environment> = OnceLock::new();

/// Inizializza l'ambiente ONNX Runtime dalla DLL indicata (una sola volta).
///
/// `dll_path` deve puntare alla `onnxruntime.dll` ufficiale Windows x64
/// (1.31.0), copiata accanto all'eseguibile o nella cartella runtime.
pub fn init_runtime(dll_path: &Path) -> Result<&'static ort::environment::Environment> {
    let builder = ort::init_from(dll_path)
        .map_err(|e| InferenceError::Model(format!("DLL runtime non caricabile: {e}")))?;
    if let Some(env) = ENV.get() {
        // ambiente gia' creato: unico per processo
        return Ok(env);
    }
    let env = builder.build().map_err(InferenceError::Ort)?;
    let _ = ENV.set(env);
    Ok(ENV.get().expect("ambiente appena impostato"))
}

/// Environment runtime gia' inizializzato (per altri moduli del crate).
pub fn environment() -> Result<&'static ort::environment::Environment> {
    ENV.get().ok_or_else(|| {
        InferenceError::Model("runtime non inizializzato: chiamare init_runtime prima".into())
    })
}

pub struct Upscaler {
    session: Session,
    input_name: String,
    output_name: String,
    norm: Normalization,
    pub scale: u32,
    tile: u32,
    overlap: u32,
}

impl Upscaler {
    /// Dimensione massima in ingresso; compromesso RAM/velocita'.
    pub const TILE: u32 = 256;

    pub fn new(model_path: &Path, scale: u32) -> Result<Upscaler> {
        if !model_path.exists() {
            return Err(InferenceError::MissingModel(
                model_path.display().to_string(),
            ));
        }
        // l'ambiente deve gia' essere stato inizializzato
        let env = ENV.get().ok_or_else(|| {
            InferenceError::Model("runtime non inizializzato: chiamare init_runtime prima".into())
        })?;
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
        let mut up = Upscaler {
            session,
            input_name,
            output_name,
            scale,
            norm: Normalization::Unit,
            tile: Self::TILE,
            overlap: 16,
        };
        // Tutti gli export ESRGAN/Real-ESRGAN (incluso UltraSharp) vogliono
        // l'ingresso in [0, 1]; l'autorilevamento sulla media sbaglia spesso
        // (un modello [0,1] nutrito con [-1,1] restituisce comunque una media
        // simile ma l'immagine risulta lava e sfocata).
        up.norm = Normalization::Unit;
        Ok(up)
    }

    /// Riduce la dimensione dei blocchi (per RAM limitate).
    pub fn set_tile(&mut self, tile: u32, overlap: u32) {
        self.tile = tile.clamp(32, 512);
        self.overlap = overlap.min(self.tile / 2);
    }

    /// Forza la normalizzazione (uso diagnostico: di norma resta [0, 1]).
    pub fn force_norm(&mut self, unit: bool) {
        self.norm = if unit {
            Normalization::Unit
        } else {
            Normalization::Centered
        };
    }

    /// Normalizzazione scelta dall'autorilevamento (diagnostica).
    #[doc(hidden)]
    pub fn norm_is_unit(&self) -> bool {
        self.norm == Normalization::Unit
    }

    /// Esegue l'ingrandimento; `progress(done, total)` dopo ogni blocco.
    ///
    /// I blocchi sovrapposti vengono fusi con pesi (feathering) per evitare
    /// cuciture visibili nei gradienti. Errore se la sessione non era attiva.
    pub fn upscale(&mut self, img: &RgbaImage, progress: &ProgressFn) -> Result<RgbaImage> {
        let (iw, ih) = img.dimensions();
        let s = self.scale as usize;
        let out_w = iw as usize * s;
        let out_h = ih as usize * s;
        let blocks = self.blocks_of(iw, ih);
        let total = blocks.len() as u32;
        let mut acc = vec![0.0f32; out_w * out_h * 3];
        let mut wsum = vec![0.0f32; out_w * out_h];
        let mut errors: Vec<InferenceError> = Vec::new();
        for (done, b) in blocks.iter().enumerate() {
            if let Err(e) = self.blend_block(img, *b, out_w, out_h, &mut acc, &mut wsum) {
                // zona vuota: sulla call successiva riparta da un riquadro vuoto
                errors.push(e);
            }
            progress(done as u32 + 1, total);
        }
        if !errors.is_empty() {
            return Err(InferenceError::Model(format!(
                "{} di {} blocchi non elaborati: {}",
                errors.len(),
                total,
                errors[errors.len() - 1]
            )));
        }
        let mut out = RgbaImage::new(out_w as u32, out_h as u32);
        for oy in 0..out_h {
            for ox in 0..out_w {
                let i = oy * out_w + ox;
                let w = wsum[i].max(1e-6);
                let px = image::Rgba([
                    (acc[i * 3] / w).round().clamp(0.0, 255.0) as u8,
                    (acc[i * 3 + 1] / w).round().clamp(0.0, 255.0) as u8,
                    (acc[i * 3 + 2] / w).round().clamp(0.0, 255.0) as u8,
                    255,
                ]);
                out.put_pixel(ox as u32, oy as u32, px);
            }
        }
        Ok(out)
    }

    /// Esegue il modello su un blocco e accumula il risultato pesato
    /// (rampa lineare nel margine di sovrapposizione).
    fn blend_block(
        &mut self,
        img: &RgbaImage,
        b: Block,
        out_w: usize,
        out_h: usize,
        acc: &mut [f32],
        wsum: &mut [f32],
    ) -> Result<()> {
        let s = self.scale as usize;
        // margine di fusione in coordinate immagine
        let ov = (self.overlap.max(1)) as f32;
        let input = self.prepare_input(img, b);
        let (oh, ow, data) = self.infer_block(&input)?;
        for by in 0..b.h as usize {
            for bx in 0..b.w as usize {
                // peso del pixel sorgente: 1 al centro, 0 al bordo del blocco
                let wx = ((bx as f32 + 0.5).min(b.w as f32 - bx as f32 - 0.5) / ov).clamp(0.0, 1.0);
                let wy = ((by as f32 + 0.5).min(b.h as f32 - by as f32 - 0.5) / ov).clamp(0.0, 1.0);
                let wpx = wx.min(wy);
                if wpx <= 0.0 {
                    continue;
                }
                for dy in 0..s {
                    for dx in 0..s {
                        let oy = (b.y as usize + by) * s + dy;
                        let ox = (b.x as usize + bx) * s + dx;
                        if ox >= out_w || oy >= out_h {
                            continue;
                        }
                        let oy_t = oy - b.y as usize * s;
                        let ox_t = ox - b.x as usize * s;
                        if oy_t >= oh || ox_t >= ow {
                            continue;
                        }
                        // ordine C dei dati: [1,3,oh,ow]
                        let index = |c: usize| c * oh * ow + oy_t * ow + ox_t;
                        let v = |c: usize| match self.norm {
                            Normalization::Unit => data[index(c)] * 255.0,
                            Normalization::Centered => (data[index(c)] + 1.0) * 127.5,
                        };
                        let i = (oy * out_w + ox) * 3;
                        acc[i] += v(0) * wpx;
                        acc[i + 1] += v(1) * wpx;
                        acc[i + 2] += v(2) * wpx;
                        wsum[oy * out_w + ox] += wpx;
                    }
                }
            }
        }
        Ok(())
    }

    /// Inferenza di un blocco gia' preparato: restituisce (oh, ow, dati [1,3,oh,ow]).
    fn infer_block(&mut self, input: &ndarray::Array4<f32>) -> Result<(usize, usize, Vec<f32>)> {
        let tensor_ref = TensorRef::<f32>::from_array_view(input)?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => tensor_ref])?;
        let (shape, data) = outputs[self.output_name.as_str()].try_extract_tensor::<f32>()?;
        if shape.len() != 4 || shape[1] != 3 {
            return Err(InferenceError::Model(format!(
                "uscita inattesa: forma {shape:?}"
            )));
        }
        Ok((shape[2] as usize, shape[3] as usize, data.to_vec()))
    }

    fn blocks_of(&self, iw: u32, ih: u32) -> Vec<Block> {
        blocks_for(self.tile, self.overlap, iw, ih)
    }

    fn prepare_input(&self, img: &RgbaImage, b: Block) -> ndarray::Array4<f32> {
        let mut input = ndarray::Array4::<f32>::zeros((1, 3, b.h as usize, b.w as usize));
        for ty in 0..b.h as usize {
            for tx in 0..b.w as usize {
                let p = img.get_pixel(b.x + tx as u32, b.y + ty as u32);
                let [r, g, bl]: [f32; 3] = match self.norm {
                    Normalization::Unit => [
                        (p[0] as f32) / 255.0,
                        (p[1] as f32) / 255.0,
                        (p[2] as f32) / 255.0,
                    ],
                    Normalization::Centered => [
                        (p[0] as f32 - 127.5) / 127.5,
                        (p[1] as f32 - 127.5) / 127.5,
                        (p[2] as f32 - 127.5) / 127.5,
                    ],
                };
                input[[0, 0, ty, tx]] = r;
                input[[0, 1, ty, tx]] = g;
                input[[0, 2, ty, tx]] = bl;
            }
        }
        input
    }
}

/// Costruisce la lista dei blocchi (con sovrapposizione) che copre l'intera immagine.
fn blocks_for(tile: u32, overlap: u32, iw: u32, ih: u32) -> Vec<Block> {
    let step = (tile - overlap).max(1);
    let xs = starts_of(iw, tile, step);
    let ys = starts_of(ih, tile, step);
    let mut blocks = Vec::with_capacity(xs.len() * ys.len());
    for &y in &ys {
        for &x in &xs {
            blocks.push(Block {
                x,
                y,
                w: tile.min(iw - x),
                h: tile.min(ih - y),
            });
        }
    }
    blocks
}

fn starts_of(len: u32, tile: u32, step: u32) -> Vec<u32> {
    if len <= tile {
        return vec![0];
    }
    let mut s: Vec<u32> = (0..=(len - tile)).step_by(step as usize).collect();
    let last = *s.last().expect("l'intervallo contiene lo zero");
    if len - last > tile {
        // aggiunge un blocco finale distinto (non sovrascrivere: creerebbe un buco)
        s.push(len - tile);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::{blocks_for, starts_of};

    #[test]
    fn blocchi_coprono_l_intera_immagine() {
        // combinazioni di lunghezza/tile/step al limite (len appena > tile)
        let casi: &[(u32, u32, u32)] = &[
            (48, 32, 24),
            (1000, 256, 240),
            (256, 256, 240),
            (257, 256, 240),
            (513, 256, 128),
            (600, 200, 60),
        ];
        for &(len, tile, step) in casi {
            let mut cov = vec![false; len as usize];
            if len > tile {
                for x in starts_of(len, tile, step) {
                    let end = (x + tile).min(len);
                    for v in &mut cov[x as usize..end as usize] {
                        *v = true;
                    }
                }
            } else {
                cov.iter_mut().for_each(|v| *v = true);
            }
            assert!(
                cov.iter().all(|v| *v),
                "buco di copertura con len={len}, tile={tile}, step={step}"
            );
        }
    }

    #[test]
    fn blocchi_un_solo_riquadro_per_immagine_piccola() {
        let b = blocks_for(256, 16, 200, 100);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].x, 0);
        assert_eq!(b[0].y, 0);
        assert_eq!(b[0].w, 200);
        assert_eq!(b[0].h, 100);
    }

    #[test]
    fn blocchi_multipli_per_immagine_grande() {
        let b = blocks_for(256, 16, 600, 300);
        assert_eq!(b.iter().filter(|k| k.x == 0).count(), 2);
        assert_eq!(b.len(), b.iter().filter(|k| k.y == 0).count() * 2);
        assert!(b.iter().all(|k| k.w <= 256 && k.h <= 256));
    }
}
