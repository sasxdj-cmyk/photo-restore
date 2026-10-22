# Notizie di terze parti (Third-Party Notices)

Photo Restore è un'applicazione che **include** i seguenti modelli AI e componenti di terze parti, già verificati con le rispettive licenze. Questo programma è rilasciato con licenza **MIT** (vedi `LICENSE`); i pesi dei modelli restano sotto le licenze dei rispettivi autori, indicate di seguito.

## Runtime

| Componente | Versione | Licenza | Fonte |
|---|---|---|---|
| onnxruntime.dll (win-x64) | 1.31.0 | MIT (Microsoft) | https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/onnxruntime-win-x64-1.31.0.zip |
| Tauri / wry / muda | 2.x | MIT / Apache-2.0 | https://github.com/tauri-apps/tauri |

I modelli seguenti sono distribuiti nell'installer e nella versione portatile **così come sono**, a titolo di comodità, nel rispetto delle condizioni delle rispettive licenze.

## Modelli di ingrandimento (upscale)

| File | Fonte | Licenza dei pesi |
|---|---|---|
| real_esrgan_x2.onnx | https://huggingface.co/SceneWorks/real-esrgan-onnx (pesi Real-ESRGAN di xinntao, https://github.com/xinntao/Real-ESRGAN) | BSD-3-Clause — distribuzione e uso commerciale consentiti |
| real_esrgan_x4.onnx | idem | BSD-3-Clause |
| ultrasharp_4x.onnx | https://huggingface.co/Kim2091/UltraSharp (4x-UltraSharp-fp32-opset14) | CC BY-NC-SA 4.0 — **solo uso non commerciale** |

## Modelli di restauro dei volti (restauratore)

| File | Fonte | Licenza dei pesi |
|---|---|---|
| codeformer_v01.onnx (codeformer.fp16) | https://huggingface.co/netrunner-exe/Face-Upscalers-onnx (modello CodeFormer di S-Lab, NTU, https://github.com/sczhou/CodeFormer) | S-Lab License 1.0 — **solo uso non commerciale** |
| gfpgan_v14.onnx (GFPGAN v1.4 fp16) | https://huggingface.co/netrunner-exe/Face-Upscalers-onnx (TencentARC, https://github.com/TencentARC/GFPGAN) | Apache-2.0 |

## Modelli di rilevamento dei volti (rilevatore)

| File | Fonte | Licenza |
|---|---|---|
| retinaface_r50.onnx | https://huggingface.co/nakamura196/retinaface-r50-onnx (architettura RetinaFace di biubug6, https://github.com/biubug6/Pytorch_Retinaface) | MIT (codice); pesi addestrati su WIDER FACE — **solo uso non commerciale** |
| retinaface_mv1_0.25.onnx | https://github.com/yakhyo/retinaface-pytorch (release v0.0.1) | MIT (codice); pesi addestrati su WIDER FACE — **solo uso non commerciale** |

## Modelli di rimozione dello sfondo

| File | Fonte | Licenza |
|---|---|---|
| u2net.onnx | https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2net.onnx (modello U-2-Net di Qin Liu, https://github.com/xuebinqin/U-2-Net) | Apache-2.0 |
| u2netp.onnx | https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx | Apache-2.0 |

## Note in formato breve

- **Uso commerciale consentito**: modelli Real-ESRGAN (BSD-3), GFPGAN v1.4, U-2-Net / u2netp (Apache-2.0), runtime ONNX (MIT).
- **Solo uso non commerciale**: CodeFormer (S-Lab 1.0), 4x-UltraSharp (CC BY-NC-SA 4.0), entrambi i RetinaFace (WIDER FACE / MIT).
- I checksum SHA-256 di ogni modello sono mostrati dall'applicazione stessa nella schermata di ogni pannello AI e nel codice sorgente (`inference/src/models.rs`).
- Questo elenco è informativo: in caso di dubbi consultare♶ la pagina della licenza di ciascun modello riportata qui sopra.
