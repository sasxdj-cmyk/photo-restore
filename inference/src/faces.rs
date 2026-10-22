//! Rilevamento e ripristino dei volti con modelli ONNX.
//!
//! Fase 1: YuNet (opencv_zoo, MIT) localizza i volti e cinque punti
//! facciali sull'immagine a dimensione intera, riempita fino a un
//! multiplo di 32. Fase 2: GFPGAN v1.4 (Apache-2.0) ripristina una
//! porzione di immagine allineata al volto usando trasformazioni di
//! similarita' col modello FFHQ; il risultato viene reinserito con una
//! sfumatura ai bordi per evitare passaggi netti.
//! Offline: pesi eseguiti con ONNX Runtime, nessun servizio in rete.

use std::path::Path;

use image::RgbaImage;
use ndarray::Array4;
use ort::session::Session;
use ort::value::TensorRef;

use crate::error::{InferenceError, Result};

/// Lato del tensore d'ingresso di GFPGAN v1.4.
pub const SIDE_MODEL: usize = 512;
/// Punti di riferimento FFHQ (in frazione del lato): occhio destro, sinistro,
/// punta del naso, angolo destro della bocca, angolo sinistro della bocca.
pub const TEMPLATE_FFHQ: [[f32; 2]; 5] = [
    [0.37691676, 0.46864664],
    [0.622_857, 0.46912813],
    [0.501_238_6, 0.61331904],
    [0.39308822, 0.725_411],
    [0.61150205, 0.72490465],
];
/// Soglia di confidenza per YuNet.
const SCORE_THRESHOLD: f32 = 0.6;
/// Soglia di sovrapposizione per la suppressione dei duplicati.
const NMS_IOU: f32 = 0.3;

/// Un volto rilevato: riquadro e cinque punti in coordinate immagine.
/// Ordine punti: occhio destro, occhio sinistro, naso, bocca destra, bocca sinistra.
#[derive(Debug, Clone, Copy)]
pub struct Face {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub points: [[f32; 2]; 5],
    pub score: f32,
}

/// Candidato sguardo: riquadro f32, cinque punti, punteggio.
type Candidato = (f32, f32, f32, f32, [[f32; 2]; 5], f32);

/// Rilevatore di volti: YuNet (opencv_zoo, MIT) oppure RetinaFace R50
/// (pesi WIDER FACE, uso non commerciale), a seconda del modello caricato.
pub struct FaceDetector {
    session: Session,
    input_name: String,
    retina: bool,
}

/// Lato massimo per il rilevamento: immagini piu' grandi vengono
/// ridimensionate prima (CPU: ResNet50/YuNet sono lente su alti pixel).
const LATO_MAX_RILEVA: u32 = 1024;

impl FaceDetector {
    pub fn new(model_path: &Path) -> Result<FaceDetector> {
        let env = crate::engine::environment()?;
        let session = Session::builder(env)?.commit_from_file(model_path)?;
        let input_name = session
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .ok_or_else(|| InferenceError::Model("modello senza ingressi".into()))?;
        // RetinaFace ha le uscite loc/conf/landms; YuNet no.
        let retina = session
            .outputs()
            .iter()
            .any(|o| o.name() == "loc" || o.name() == "loc_0");
        Ok(FaceDetector {
            session,
            input_name,
            retina,
        })
    }

    /// Individua i volti sull'immagine in coordinate originali.
    pub fn detect(&mut self, img: &RgbaImage) -> Result<Vec<Face>> {
        let (iw, ih) = img.dimensions();
        if iw == 0 || ih == 0 {
            return Ok(Vec::new());
        }
        // troppo grandi: backsizing, poi riporto le coordinate a scala piena
        let max_side = iw.max(ih);
        if max_side > LATO_MAX_RILEVA {
            let sc = LATO_MAX_RILEVA as f32 / max_side as f32;
            let dw = ((iw as f32 * sc).round() as u32).max(32);
            let dh = ((ih as f32 * sc).round() as u32).max(32);
            let small = image::imageops::resize(
                img, dw, dh, image::imageops::FilterType::Lanczos3,
            );
            let mut found = self.detect(&small)?;
            for f in &mut found {
                f.x /= sc;
                f.y /= sc;
                f.w /= sc;
                f.h /= sc;
                for p in &mut f.points {
                    p[0] /= sc;
                    p[1] /= sc;
                }
                f.score = f.score.min(1.0);
            }
            return Ok(found);
        }
        // riempimento in basso/a destra fino a un multiplo di 32: solo YuNet.
        // RetinaFace lavora direttamente sulle dimensioni native (i modelli
        // esportati recenti aspettano il bordo a zero e la nozione di
        // "fuori immagine" e' senza media).
        let (pw, ph) = if self.retina {
            (iw as usize, ih as usize)
        } else {
            (
                (iw.div_ceil(32) * 32).max(32) as usize,
                (ih.div_ceil(32) * 32).max(32) as usize,
            )
        };
        let mut input = Array4::<f32>::zeros((1, 3, ph, pw));
        for y in 0..ih as usize {
            for x in 0..iw as usize {
                let p = img.get_pixel(x as u32, y as u32);
                // il blob di riferimento usa l'ordine BGR a valori pieni
                input[[0, 0, y, x]] = p[2] as f32;
                input[[0, 1, y, x]] = p[1] as f32;
                input[[0, 2, y, x]] = p[0] as f32;
            }
        }
        let names: Vec<String> = self
            .session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        // RetinaFace: stesso ordine BGR minus la media ImageNet (BGR), solo
        // sui pixel reali: il bordo resta a zero.
        if self.retina {
            let means = [104.0f32, 117.0, 123.0];
            for ch in 0..3 {
                for y in 0..ih as usize {
                    for x in 0..iw as usize {
                        input[[0, ch, y, x]] -= means[ch];
                    }
                }
            }
        }
        let tensor = TensorRef::<f32>::from_array_view(&input)?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => tensor])?;
        let mut candidates: Vec<Candidato> = Vec::new();
        if self.retina {
            // uscite: loc [1,N,4], conf [1,N,2] (softmax in grafo), landms [1,N,10]
            let (n_loc, loc) = extract(&outputs, &names[0], 4)?;
            let (n_conf, conf) = extract(&outputs, &names[1], 2)?;
            let (n_lm, lms) = extract(&outputs, &names[2], 10)?;
            let priors = ancoraggi_retina(pw, ph);
            if n_loc != priors.len() || n_conf != priors.len() || n_lm != priors.len() {
                return Err(InferenceError::Model(format!(
                    "RetinaFace: ancoraggi loc={n_loc} conf={n_conf} lm={n_lm} != {}",
                    priors.len()
                )));
            }
            for (idx, pr) in priors.iter().enumerate() {
                let score = conf[idx * 2 + 1].clamp(0.0, 1.0);
                if score < SCORE_THRESHOLD {
                    continue;
                }
                let cx = pr[0] + loc[idx * 4] * 0.1 * pr[2];
                let cy = pr[1] + loc[idx * 4 + 1] * 0.1 * pr[3];
                let w = (loc[idx * 4 + 2] * 0.2).exp().min(100.0) * pr[2];
                let h = (loc[idx * 4 + 3] * 0.2).exp().min(100.0) * pr[3];
                if !(w.is_finite() && h.is_finite()) || w < 6.0 || h < 6.0 {
                    continue;
                }
                let mut points = [[0.0f32; 2]; 5];
                for n in 0..5 {
                    points[n][0] = pr[0] + lms[idx * 10 + 2 * n] * 0.1 * pr[2];
                    points[n][1] = pr[1] + lms[idx * 10 + 2 * n + 1] * 0.1 * pr[3];
                }
                candidates.push((cx - w / 2.0, cy - h / 2.0, w, h, points, score));
            }
        } else {
        let strides = [8usize, 16, 32];
        for (si, &s) in strides.iter().enumerate() {
            let (anchors, cls) = extract(&outputs, &names[si], 1)?;
            let (_, obj) = extract(&outputs, &names[si + 3], 1)?;
            let (_, bbox) = extract(&outputs, &names[si + 6], 4)?;
            let (_, kps) = extract(&outputs, &names[si + 9], 10)?;
            let cols = pw / s;
            let rows = ph / s;
            if anchors != rows * cols {
                return Err(InferenceError::Model(format!(
                    "YuNet: ancoraggi {anchors} != {rows}x{cols} con stride {s}"
                )));
            }
            for r in 0..rows {
                for c in 0..cols {
                    let idx = r * cols + c;
                    let score =
                        (cls[idx].clamp(0.0, 1.0) * obj[idx].clamp(0.0, 1.0)).sqrt();
                    if score < SCORE_THRESHOLD {
                        continue;
                    }
                    let cx = (c as f32 + bbox[idx * 4]) * s as f32;
                    let cy = (r as f32 + bbox[idx * 4 + 1]) * s as f32;
                    let w = bbox[idx * 4 + 2].exp() * s as f32;
                    let h = bbox[idx * 4 + 3].exp() * s as f32;
                    let mut points = [[0.0f32; 2]; 5];
                    for n in 0..5 {
                        points[n][0] = (kps[idx * 10 + 2 * n] + c as f32) * s as f32;
                        points[n][1] = (kps[idx * 10 + 2 * n + 1] + r as f32) * s as f32;
                    }
                    candidates.push((cx - w / 2.0, cy - h / 2.0, w, h, points, score));
                }
            }
        }
        }
        Ok(nms(candidates, NMS_IOU))
    }
}

/// Ancoraggi RetinaFace in px (cx, cy, larghezza, altezza), conteggio con
/// arrotondamento in eccesso (come il PriorBox di yakhyo/retinaface-pytorch):
/// passi 8/16/32 con misure 16/32, 64/128, 256/512, ordinati
/// righe x colonne x misure.
fn ancoraggi_retina(pw: usize, ph: usize) -> Vec<[f32; 4]> {
    const PASSI: [usize; 3] = [8, 16, 32];
    const MISURE: [[usize; 2]; 3] = [[16, 32], [64, 128], [256, 512]];
    let mut v = Vec::new();
    for (s, misure) in PASSI.iter().zip(MISURE.iter()) {
        let cols = pw.div_ceil(*s);
        let rows = ph.div_ceil(*s);
        for riga in 0..rows {
            for col in 0..cols {
                for m in misure {
                    let cx = (col * s + m / 2) as f32;
                    let cy = (riga * s + m / 2) as f32;
                    v.push([cx, cy, *m as f32, *m as f32]);
                }
            }
        }
    }
    v
}

#[cfg(test)]
mod prova_ancoraggi {
    use super::ancoraggi_retina;

    #[test]
    fn numero_e_ordine() {
        // blob 64x96: passo 8 => 8x12 celle x2 = 192; 16 => 4x6 x2 = 48;
        // 32 => 2x3 x2 = 12; totale 252
        let a = ancoraggi_retina(96, 64);
        assert_eq!(a.len(), 252);
        // prima ancora: passo 8, cella (0,0), misura 16: centro (8, 8)
        assert_eq!(a[0], [8.0, 8.0, 16.0, 16.0]);
        // seconda misura 32: centro (16, 16)
        assert_eq!(a[1], [16.0, 16.0, 32.0, 32.0]);
        // passo 32, cella (0,0), misura 256: indice 192+48=240, centro 128
        assert_eq!(a[240], [128.0, 128.0, 256.0, 256.0]);
    }
}

/// Estrazione di un canale di YuNet: (numero di ancoraggi, dati).
fn extract(
    outputs: &ort::session::SessionOutputs,
    name: &str,
    expect_last: usize,
) -> Result<(usize, Vec<f32>)> {
    let (shape, data) = outputs[name].try_extract_tensor::<f32>()?;
    if shape.len() != 3 || shape[2] != expect_last as i64 {
        return Err(InferenceError::Model(format!(
            "YuNet: uscita {name} inattesa, forma {shape:?}"
        )));
    }
    Ok((shape[1] as usize, data.to_vec()))
}

/// Soppressione non-massima con soglia di sovrapposizione (IoU).
fn nms(mut cands: Vec<Candidato>, iou_thr: f32) -> Vec<Face> {
    cands.sort_by(|a, b| b.5.partial_cmp(&a.5).unwrap_or(std::cmp::Ordering::Equal));
    let mut keep: Vec<usize> = Vec::new();
    for (i, _) in cands.iter().enumerate() {
        if keep.iter().any(|k| iou(&cands[*k], &cands[i]) > iou_thr) {
            continue;
        }
        keep.push(i);
    }
    keep.into_iter()
        .map(|i| Face {
            x: cands[i].0,
            y: cands[i].1,
            w: cands[i].2,
            h: cands[i].3,
            points: cands[i].4,
            score: cands[i].5,
        })
        .collect()
}

fn iou(a: &Candidato, b: &Candidato) -> f32 {
    let ax2 = a.0 + a.2;
    let ay2 = a.1 + a.3;
    let bx2 = b.0 + b.2;
    let by2 = b.1 + b.3;
    let ix = (ax2.min(bx2) - a.0.max(b.0)).max(0.0);
    let iy = (ay2.min(by2) - a.1.max(b.1)).max(0.0);
    let inter = ix * iy;
    let ua = a.2 * a.3 + b.2 * b.3 - inter;
    if ua <= 0.0 { 0.0 } else { inter / ua }
}

/// Trasformazione di similarita' (scala + rotazione) che porta `src` in `dst`,
/// fittata ai minimi quadrati, come matrice [a, b, tx, c, d, ty] (dst = M * p).
/// Fittata a Procrustes 2D senza riflessione.
pub fn fit_similarity(src: &[[f32; 2]], dst: &[[f32; 2]]) -> [f64; 6] {
    let n = src.len() as f64;
    let (sx, sy) = src
        .iter()
        .fold((0.0f64, 0.0f64), |a, p| (a.0 + p[0] as f64, a.1 + p[1] as f64));
    let (dx, dy) = dst
        .iter()
        .fold((0.0f64, 0.0f64), |a, p| (a.0 + p[0] as f64, a.1 + p[1] as f64));
    let (cs, cy) = (sx / n, sy / n);
    let (cdx, cdy) = (dx / n, dy / n);
    let mut a = 0.0f64;
    let mut b = 0.0f64;
    let mut norm_s = 0.0f64;
    for (p, q) in src.iter().zip(dst.iter()) {
        let u = p[0] as f64 - cs;
        let v = p[1] as f64 - cy;
        let x = q[0] as f64 - cdx;
        let y = q[1] as f64 - cdy;
        a += u * x + v * y;
        b += u * y - v * x;
        norm_s += u * u + v * v;
    }
    let r = (a * a + b * b).sqrt();
    if r < 1e-12 || norm_s < 1e-12 {
        return [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    }
    let s = r / norm_s;
    let m00 = s * a / r;
    let m01 = -s * b / r;
    let m10 = s * b / r;
    let m11 = s * a / r;
    let tx = cdx - m00 * cs - m01 * cy;
    let ty = cdy - m10 * cs - m11 * cy;
    [m00, m01, tx, m10, m11, ty]
}

/// Applica la matrice di similarita' a un punto.
pub fn apply(m: &[f64; 6], x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5])
}

/// Inversione della trasformazione affine di similarita'.
pub fn invert(m: &[f64; 6]) -> [f64; 6] {
    let det = m[0] * m[4] - m[1] * m[3];
    if det.abs() < 1e-12 {
        return [1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    }
    let a = m[4] / det;
    let b = -m[1] / det;
    let c = -m[3] / det;
    let d = m[0] / det;
    let tx = -(a * m[2] + b * m[5]);
    let ty = -(c * m[2] + d * m[5]);
    [a, b, tx, c, d, ty]
}

/// Campionamento bilineare (canale ch, bordi memorizzati nel riscontro).
fn bilinear(img: &RgbaImage, x: f32, y: f32, ch: usize) -> f32 {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = (x - x0 as f32).clamp(0.0, 1.0);
    let fy = (y - y0 as f32).clamp(0.0, 1.0);
    let cx = |i: i32| -> usize { i.clamp(0, w - 1) as usize };
    let cy = |i: i32| -> usize { i.clamp(0, h - 1) as usize };
    let p = |ix: i32, iy: i32| -> f32 { img.get_pixel(cx(ix) as u32, cy(iy) as u32)[ch] as f32 };
    let top = p(x0, y0) * (1.0 - fx) + p(x0 + 1, y0) * fx;
    let bottom = p(x0, y0 + 1) * (1.0 - fx) + p(x0 + 1, y0 + 1) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// Ripristinatore di volti: GFPGAN v1.4 (Apache-2.0) o CodeFormer
/// (S-Lab, non commerciale), fp16 con ingresso 512 x 512 in [-1, 1].
pub struct FaceRestorer {
    session: Session,
    input_name: String,
    output_name: String,
    /// Vero se il modello caricato e' CodeFormer (due ingressi).
    pub codeformer: bool,
    /// Fedelta' CodeFormer (0..1): verso 0 predomina la ricostruzione del
    /// modello (piu' dettaglio "inventato"), verso 1 piu' fedelta' ai pixel.
    pub fidelity: f32,
}

impl FaceRestorer {
    pub fn new(model_path: &Path) -> Result<FaceRestorer> {
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
        // CodeFormer ha anche l'ingresso scalare `w` (fedelta')
        let codeformer = session.inputs().len() == 2;
        Ok(FaceRestorer {
            session,
            input_name,
            output_name,
            codeformer,
            fidelity: 0.8,
        })
    }

    /// Ripristina il volto `face` sull'immagine e restituisce un nuovo buffer.
    pub fn ripristina(&mut self, img: &RgbaImage, face: &Face) -> Result<RgbaImage> {
        let side = SIDE_MODEL as f32;
        let template: Vec<[f32; 2]> = TEMPLATE_FFHQ
            .iter()
            .map(|p| [p[0] * side, p[1] * side])
            .collect();
        let pts: Vec<[f32; 2]> = face.points.to_vec();
        // immagine -> modello e inversa
        let to_model = fit_similarity(&pts, &template);
        let to_image = invert(&to_model);

        // 1) porzione allineata: campiona l'immagine nei punti del modello
        let mut input = Array4::from_elem((1, 3, SIDE_MODEL, SIDE_MODEL), half::f16::ZERO);
        for y in 0..SIDE_MODEL {
            for x in 0..SIDE_MODEL {
                let (sx, sy) = apply(&to_image, x as f64, y as f64);
                let (sx, sy) = (sx as f32, sy as f32);
                input[[0, 0, y, x]] =
                    half::f16::from_f32(((bilinear(img, sx, sy, 0) / 255.0) - 0.5) * 2.0);
                input[[0, 1, y, x]] =
                    half::f16::from_f32(((bilinear(img, sx, sy, 1) / 255.0) - 0.5) * 2.0);
                input[[0, 2, y, x]] =
                    half::f16::from_f32(((bilinear(img, sx, sy, 2) / 255.0) - 0.5) * 2.0);
            }
        }
        let tensor = TensorRef::<half::f16>::from_array_view(&input)?;
        let outputs = if self.codeformer {
            // CodeFormer: ingresso scalare w (fedelta') in f64
            let w = ndarray::arr0(self.fidelity.clamp(0.0, 1.0) as f64);
            let w_ref = TensorRef::<f64>::from_array_view(w.view())?;
            self.session.run(ort::inputs![
                self.input_name.as_str() => tensor,
                "w" => w_ref
            ])?
        } else {
            self.session
                .run(ort::inputs![self.input_name.as_str() => tensor])?
        };
        let (shape, data) = outputs[self.output_name.as_str()].try_extract_tensor::<half::f16>()?;
        if shape.len() != 4 || shape[1] != 3 || shape[2] != SIDE_MODEL as i64 || shape[3] != SIDE_MODEL as i64 {
            return Err(InferenceError::Model(format!(
                "uscita GFPGAN inattesa: forma {shape:?}"
            )));
        }

        // 2) riquadro sorgente: proiezione degli angoli del modello
        let side64 = SIDE_MODEL as f64;
        let corners = [
            apply(&to_image, 0.0, 0.0),
            apply(&to_image, side64, 0.0),
            apply(&to_image, side64, side64),
            apply(&to_image, 0.0, side64),
        ];
        let min_x = corners.iter().fold(f64::MAX, |m, c| m.min(c.0));
        let max_x = corners.iter().fold(f64::MIN, |m, c| m.max(c.0));
        let min_y = corners.iter().fold(f64::MAX, |m, c| m.min(c.1));
        let max_y = corners.iter().fold(f64::MIN, |m, c| m.max(c.1));
        let (iw, ih) = img.dimensions();
        let x0 = min_x.floor().max(0.0) as i64;
        let x1 = (max_x.ceil() as i64).min(iw as i64 - 1);
        let y0 = min_y.floor().max(0.0) as i64;
        let y1 = (max_y.ceil() as i64).min(ih as i64 - 1);

        let mut out = img.clone();
        if x1 >= x0 && y1 >= y0 {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let t = apply(&to_model, x as f64, y as f64);
                    if t.0 < -0.5 || t.0 > (side64 - 0.5) || t.1 < -0.5 || t.1 > (side64 - 0.5) {
                        continue;
                    }
                    let w = peso_viso(t.0 as f32, t.1 as f32, side);
                    if w <= 0.0 {
                        continue;
                    }
                    let p = img.get_pixel(x as u32, y as u32);
                    let r = bilinear_rst(data, t.0 as f32, t.1 as f32, 0);
                    let g = bilinear_rst(data, t.0 as f32, t.1 as f32, 1);
                    let b = bilinear_rst(data, t.0 as f32, t.1 as f32, 2);
                    let blend = |o: u8, n: f32| -> u8 {
                        (o as f32 * (1.0 - w) + n * w).round().clamp(0.0, 255.0) as u8
                    };
                    let nr = blend(p[0], ((r + 1.0) * 127.5).clamp(0.0, 255.0));
                    let ng = blend(p[1], ((g + 1.0) * 127.5).clamp(0.0, 255.0));
                    let nb = blend(p[2], ((b + 1.0) * 127.5).clamp(0.0, 255.0));
                    out.put_pixel(x as u32, y as u32, image::Rgba([nr, ng, nb, p[3]]));
                }
            }
        }
        Ok(out)
    }
}

/// Bilinear sul buffer CHW del modello (valori f16 in [-1, 1]).
fn bilinear_rst(data: &[half::f16], x: f32, y: f32, ch: usize) -> f32 {
    let x0 = (x.floor().max(0.0) as usize).min(SIDE_MODEL - 2);
    let y0 = (y.floor().max(0.0) as usize).min(SIDE_MODEL - 2);
    let fx = (x - x0 as f32).clamp(0.0, 1.0);
    let fy = (y - y0 as f32).clamp(0.0, 1.0);
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let p = |ix: usize, iy: usize| -> f32 { data[ch * SIDE_MODEL * SIDE_MODEL + iy * SIDE_MODEL + ix].to_f32() };
    let top = p(x0, y0) * (1.0 - fx) + p(x1, y0) * fx;
    let bottom = p(x0, y1) * (1.0 - fx) + p(x1, y1) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// Peso di fusione: pieno nel nucleo del volto, sfuma oltre, zero nel resto
/// del riquadro allineato. Ellisse in coordinate del modello centrata a
/// (0.50, 0.585) con raggi 0.205/0.265: copre tempie, occhi, naso, bocca e
/// mento, esclude lo sfondo laterale e la massa sotto il mento (dove la
/// pasta lascia una "patina" sfocata sull'ambiente).
fn peso_viso(x: f32, y: f32, side: f32) -> f32 {
    let s = side;
    let dx = (x / s - 0.5) / 0.205;
    let dy = (y / s - 0.585) / 0.265;
    let d = (dx * dx + dy * dy).sqrt();
    if d <= 0.92 {
        return 1.0;
    }
    // pieno fino al 92 percento del raggio, in discesa fino al 108
    let t = ((1.08 - d) / 0.16).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::{apply, fit_similarity, invert};

    #[test]
    fn similarity_identita() {
        let pts: Vec<[f32; 2]> = vec![[10.0, 10.0], [100.0, 10.0], [50.0, 90.0], [20.0, 80.0]];
        let m = fit_similarity(&pts, &pts);
        for p in &pts {
            let t = apply(&m, p[0] as f64, p[1] as f64);
            assert!((t.0 - p[0] as f64).abs() < 1e-6, "x: {:?}", t);
            assert!((t.1 - p[1] as f64).abs() < 1e-6, "y: {:?}", t);
        }
    }

    #[test]
    fn similarity_rotazione_scala() {
        let pts: Vec<[f32; 2]> = vec![[10.0, 10.0], [100.0, 10.0], [50.0, 90.0]];
        let cos = std::f64::consts::FRAC_PI_4.cos() as f32;
        let sin = std::f64::consts::FRAC_PI_4.sin() as f32;
        let s = 2.0f32;
        let f = |p: [f32; 2]| -> [f32; 2] {
            let dx = p[0] - 50.0;
            let dy = p[1] - 50.0;
            [
                50.0 + s * (dx * cos - dy * sin),
                50.0 + s * (dx * sin + dy * cos),
            ]
        };
        let dst: Vec<[f32; 2]> = pts.iter().map(|p| f(*p)).collect();
        let m = fit_similarity(&pts, &dst);
        for p in &pts {
            let t = apply(&m, p[0] as f64, p[1] as f64);
            assert!((t.0 - f(*p)[0] as f64).abs() < 1e-4, "x: {:?}", t);
            assert!((t.1 - f(*p)[1] as f64).abs() < 1e-4, "y: {:?}", t);
        }
    }

    #[test]
    fn similarity_inversione() {
        let pts: Vec<[f32; 2]> = vec![[10.0, 10.0], [100.0, 10.0], [50.0, 90.0]];
        let cos = std::f64::consts::FRAC_PI_6.cos() as f32;
        let sin = std::f64::consts::FRAC_PI_6.sin() as f32;
        let f = |p: [f32; 2]| -> [f32; 2] {
            [
                30.0 + 0.7 * (p[0] * cos - p[1] * sin),
                5.0 + 0.7 * (p[0] * sin + p[1] * cos),
            ]
        };
        let dst: Vec<[f32; 2]> = pts.iter().map(|p| f(*p)).collect();
        let m = fit_similarity(&pts, &dst);
        let inv = invert(&m);
        for p in &pts {
            let t = apply(&m, p[0] as f64, p[1] as f64);
            let back = apply(&inv, t.0, t.1);
            assert!((back.0 - p[0] as f64).abs() < 1e-4, "x: {:?}", back);
            assert!((back.1 - p[1] as f64).abs() < 1e-4, "y: {:?}", back);
        }
    }

    #[test]
    fn pesi_viso_ai_bordi() {
        assert!((super::peso_viso(256.0, 256.0, 512.0) - 1.0).abs() < 1e-6);
        assert_eq!(super::peso_viso(0.0, 256.0, 512.0), 0.0);
        assert_eq!(super::peso_viso(256.0, 511.0, 512.0), 0.0);
        // sopra la fronte e ai lati del volto: fuori dall'ellisse
        assert_eq!(super::peso_viso(30.0, 30.0, 512.0), 0.0);
        assert_eq!(super::peso_viso(480.0, 480.0, 512.0), 0.0);
    }
}
