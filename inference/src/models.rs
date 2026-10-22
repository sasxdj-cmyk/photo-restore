//! Registro dei modelli: fonti verificate, licenze e integrità attesa.
//!
//! Solo i modelli con verifiche a data certa hanno `sha256`; per gli altri il
//! campo compare dopo la verifica sperimentale prevista in Fase 3.

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub filename: &'static str,
    pub url: &'static str,
    /// SHA-256 atteso (esadecimale) o None se da verificare al download.
    pub sha256: Option<&'static str>,
    pub license: &'static str,
    pub license_url: &'static str,
    /// Fattore di ingrandimento del modello (2 per x2plus, 4 per x4plus, ...).
    pub scale: u32,
    /// Tipo di elaborazione: "upscale" (pannelli AI) o "sfondo" (rimozione).
    pub kind: &'static str,
    /// Dimensione approssimativa in MB (per il consenso di download).
    pub size_mb: u32,
    /// Avviso da mostrare prima del download (es. licenze non commerciali).
    pub notice: &'static str,
}

pub use self::models_table::MODELS;

/// Ricerca un modello per identificatore.
pub fn find(id: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id)
}

pub fn list() -> Vec<&'static ModelInfo> {
    MODELS.iter().collect()
}

/// Modulo interno con i dati attuali.
mod models_table {
    use super::ModelInfo;

    pub const MODELS: &[ModelInfo] = &[
        ModelInfo {
            id: "realesrgan-x2plus",
            name: "RealESRGAN x2plus (ingrandimento 2x)",
            filename: "real_esrgan_x2.onnx",
            url: "https://huggingface.co/SceneWorks/real-esrgan-onnx/resolve/main/real_esrgan_x2.onnx",
            sha256: Some("7115ba92e8a1bfa63d68558ef006ef3d91273a068d321b1439f8bb1c9179002c"),
            license: "BSD-3-Clause (pesi Real-ESRGAN di xinntao)",
            license_url: "https://github.com/xinntao/Real-ESRGAN/blob/master/LICENSE",
            scale: 2,
            kind: "upscale",
            size_mb: 64,
            notice: "Licenza BSD-3-Clause: distribuzione e uso commerciale consentiti.",
        },
        ModelInfo {
            id: "realesrgan-x4plus",
            name: "RealESRGAN x4plus (ingrandimento 4x)",
            filename: "real_esrgan_x4.onnx",
            url: "https://huggingface.co/SceneWorks/real-esrgan-onnx/resolve/main/real_esrgan_x4.onnx",
            sha256: Some("5c586662929cbc686c1a5c38d9c060dbdb4ea5863a1f7672b8c0761e6b89c033"),
            license: "BSD-3-Clause (pesi Real-ESRGAN di xinntao)",
            license_url: "https://github.com/xinntao/Real-ESRGAN/blob/master/LICENSE",
            scale: 4,
            kind: "upscale",
            size_mb: 67,
            notice: "Licenza BSD-3-Clause: distribuzione e uso commerciale consentiti.",
        },
        ModelInfo {
            id: "sfondo-u2net",
            name: "Rimozione sfondo - precisa",
            filename: "u2net.onnx",
            url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2net.onnx",
            sha256: Some("8d10d2f3bb75ae3b6d527c77944fc5e7dcd94b29809d47a739a7a728a912b491"),
            license: "Apache-2.0 (pesi U-2-Net di Qin Liu)",
            license_url: "https://github.com/xuebinqin/U-2-Net/blob/master/LICENSE",
            scale: 1,
            kind: "sfondo",
            size_mb: 168,
            notice: "Licenza Apache-2.0: distribuzione e uso commerciale consentiti.",
        },
        ModelInfo {
            id: "sfondo-u2netp",
            name: "Rimozione sfondo - leggera",
            filename: "u2netp.onnx",
            url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx",
            sha256: Some("309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8"),
            license: "Apache-2.0 (pesi U-2-Net di Qin Liu)",
            license_url: "https://github.com/xuebinqin/U-2-Net/blob/master/LICENSE",
            scale: 1,
            kind: "sfondo",
            size_mb: 5,
            notice: "Licenza Apache-2.0: distribuzione e uso commerciale consentiti.",
        },
        ModelInfo {
            id: "volto-codeformer",
            name: "Ripristino volti - CodeFormer (piu' dettaglio)",
            filename: "codeformer_v01.onnx",
            url: "https://huggingface.co/netrunner-exe/Face-Upscalers-onnx/resolve/main/codeformer.fp16.onnx",
            sha256: Some("5414b2c989eb93feb7b5bccd22a1a9f3842a127411932663d03884af140a48b6"),
            license: "S-Lab License 1.0 (pesi CodeFormer di S-Lab, NTU) - solo uso non commerciale",
            license_url: "https://github.com/sczhou/CodeFormer/blob/master/LICENSE",
            scale: 1,
            kind: "volto",
            size_mb: 180,
            notice: "Licenza S-Lab: solo uso personale o di ricerca (non commerciale).",
        },
        ModelInfo {
            id: "volto-det-retinaface",
            name: "Rilievo volti - RetinaFace R50 (preciso)",
            filename: "retinaface_r50.onnx",
            url: "https://huggingface.co/nakamura196/retinaface-r50-onnx/resolve/main/retinaface_r50.onnx",
            sha256: Some("ea3f7d12894980d52d3fd7f3f5cab463d48e9cfbcc06f40616bdeeab3e3f31d1"),
            license: "MIT (codice biubug6); pesi addestrati su WIDER FACE - solo uso non commerciale",
            license_url: "https://github.com/biubug6/Pytorch_Retinaface",
            scale: 1,
            kind: "volto",
            size_mb: 105,
            notice: "Pesi da WIDER FACE: solo uso personale o di ricerca (non commerciale).",
        },
        ModelInfo {
            id: "volto-det-retinaface25",
            name: "Rilievo volti - RetinaFace Mobile 0.25 (leggero)",
            filename: "retinaface_mv1_0.25.onnx",
            url: "https://github.com/yakhyo/retinaface-pytorch/releases/download/v0.0.1/retinaface_mv1_0.25.onnx",
            sha256: Some("b7a7acab55e104dce6f32cdfff929bd83946da5cd869b9e2e9bdffafd1b7e4a5"),
            license: "MIT (codice yakhyo/retinaface-pytorch); pesi addestrati su WIDER FACE - solo uso non commerciale",
            license_url: "https://github.com/yakhyo/retinaface-pytorch",
            scale: 1,
            kind: "volto",
            size_mb: 2,
            notice: "Pesi da WIDER FACE: solo uso personale o di ricerca (non commerciale). Molto rapido (1,7 MB).",
        },
        ModelInfo {
            id: "volto-gfpgan",
            name: "Ripristino volti - GFPGAN (uso commerciale consentito)",
            filename: "gfpgan_v14.onnx",
            url: "https://huggingface.co/netrunner-exe/Face-Upscalers-onnx/resolve/main/GFPGANv1.4.fp16.onnx",
            sha256: Some("d851fd539987e5efacb04c59cf77e02e5291c3581e09ba9dea54def7571ff181"),
            license: "Apache-2.0 (pesi GFPGAN di TencentARC) - distribuzione e uso commerciale consentiti",
            license_url: "https://github.com/TencentARC/GFPGAN/blob/master/LICENSE",
            scale: 1,
            kind: "volto",
            size_mb: 163,
            notice: "Licenza Apache-2.0: distribuzione e uso commerciale consentiti. Ricostruzione piu' libera del CodeFormer.",
        },
        ModelInfo {
            id: "realesrgan-ultrasharp-4x",
            name: "UltraSharp x4 (nitido su JPEG)",
            filename: "ultrasharp_4x.onnx",
            url: "https://huggingface.co/Kim2091/UltraSharp/resolve/main/ONNX/4x-UltraSharp-fp32-opset14.onnx",
            sha256: Some("50eab9181b555f4ddd8bda2bed59a534545bc4b3aca05c2d78bc232d1d70ec1c"),
            license: "CC BY-NC-SA 4.0 (pesi 4x-UltraSharp di Kim2091) - solo uso non commerciale",
            license_url: "https://huggingface.co/Kim2091/UltraSharp",
            scale: 4,
            kind: "upscale",
            size_mb: 64,
            notice: "Licenza CC BY-NC-SA: solo uso personale o di ricerca (non commerciale).",
        },
    ];
}
