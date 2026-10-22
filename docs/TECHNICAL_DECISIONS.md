# Decisioni tecniche — Photo Restore

Documentazione della Fase 1 (verifica tecnica, 2026-10-17).
Voci marcate **DA TESTARE**: non ancora eseguite; nessuna funzione viene dichiarata
operativa prima dei test reali previsti in Fase 3.

Ambiente di sviluppo verificato: Windows 11 x64, Node 24.20.0, npm 11.19.0.
Installato in Fase 1: Rust 1.99.1. WebView2 runtime 155.0.4283.60 gia presente.

### 0.1 Toolchain di compilazione (decisione dell'ambiente, 2026-10-17)

L'installazione automatica di Visual Studio 2022 Build Tools non e' riuscita in modo
riproducibile in questo ambiente (l'elevazione amministrativa veniva annullata dal
sistema). Decisione riproducibile senza privilegi:

- **Toolchain host e target x86_64-pc-windows-gnu** (`rustup default
  stable-x86_64-pc-windows-gnu`), con MinGW-w64 UCRT GCC 16.2 (WinLibs, rilasci
  brechtsanders/winlibs_mingw, licenza individuabile nel repository) installato in
  `%USERPROFILE%\.mingw64` e aggiunto al PATH utente (nessuna elevazione).
- **Cartella di build fuori dal percorso con spazi** (`CARGO_TARGET_DIR =
  %USERPROFILE%\pr-target`): `windres`/`cc1` di MinGW non accettano percorsi con
  spazi (qui il progetto vive in `Documenti\Default Project`).
- `tauri.conf.json` resta invariato; l'applicazione di produzione in Fase 4 potra'
  essere composta con MSVC rendendolo disponibile, senza cambiamenti di codice.
- Il crate `src-tauri` compila con `crate-type = ["rlib"]` (solo desktop): la
  combinazione `staticlib`/`cdylib` genera un file di esportazione con un numero di
  simboli oltre il limite di `ld` di MinGW ("export ordinal too large").
- Script di supporto locali: `compila.cmd` (build/test/verifica/formattazione) e
  `avvia-dev.cmd` (modalita sviluppo attraverso `npm run dev`).

---

## 1. Architettura generale

- UI desktop in HTML/CSS/JavaScript statico, in italiano, servita come `frontendDist`
  senza bundler (consultata la documentazione Tauri 2, sezione "Develop", 2026-10-17).
- Shell desktop: **Tauri 2.12.4** (stabile; licenza MIT OR Apache-2.0). Tauri 3.0
  scartata perche solo in alpha (3.0.0-alpha.5 su crates.io).
- Workspace Cargo con tre crate:
  - `src-tauri`: comandi async, eventi di progresso, dialoghi nativi, stato.
  - `core`: elaborazione immagini in Rust puro, testabile con `cargo test` senza UI.
  - `inference` (Fase 3): runtime ONNX, tiling, gestore modelli.
- Motivazioni: separazione UI/logica/inferenza richiesta dal piano; core verificabile
  senza finestra; inferenza sostituibile senza toccare il resto.

## 2. Runtime di inferenza (Fase 3)

- Scelta: **crate `ort` 2.0.0-rc.14** (binding Rust per ONNX Runtime, pykeio/ort).
- Licenza: MIT OR Apache-2.0 (crates.io, 2026-10-17). Progetto attivamente mantenuto.
- Execution Provider candidati, ciascuno DA TESTARE nel proprio ambiente:
  - CPU: sempre disponibile, percorso garantito. Il programma NON richiede una GPU
    per essere installato o utilizzato.
  - CUDA (feature `cuda`): GPU NVIDIA; richiede librerie CUDA/cuDNN separate,
    sotto licenza NVIDIA.
  - DirectML (feature `directml`): Windows, qualunque GPU DX12 (NVIDIA, AMD, Intel).
  - CoreML (feature `coreml`): macOS / Apple Silicon.
- Limiti documentati:
  - CUDA, DirectML e CoreML non sono intercambiabili; il codice prova i EP in ordine
    di preferenza e ricade su CPU con messaggio chiaro.
  - Le stime di tempo vengono mostrate solo dopo una prima misura reale sul backend
    attivo; prima di allora nessuna stima inventata.
  - Tiling con sovrapposizione per RealESRGAN (tessera 256-512 px, overlap 16-32 px)
    per controllare la RAM con immagini grandi.
- Alternativa scartata: esecuzione via Python (vietata dal requisito). La conversione
  dei pesi in ONNX resta un procedimento del maintainer: l'utente scarica solo file
  ONNX con checksum dichiarato.
- crate OpenCV valutati ma scartati: richiedono SDK nativi pesanti per ogni
  piattaforma; Canny, contorni, omografia e resize sono copribili con `image`,
  `imageproc` e codice proprio.

## 3. Modelli: fonti, licenze, integrita (verificate il 2026-10-17)

Tutti i pesi restano fuori dall'installer: nessun download senza consenso esplicito,
verifica SHA-256, opzione di riusare modelli gia presenti.

| Modello | Uso | Fonte | Licenza | Dimensione | Stato |
|---|---|---|---|---|---|
| RealESRGAN x2plus (ONNX) | upscaling AI 2x | HF SceneWorks/real-esrgan-onnx, file real_esrgan_x2.onnx, opset 17, input dinamico, SHA-256 7115ba92e8a1bfa63d68558ef006ef3d91273a068d321b1439f8bb1c9179002c; pesi upstream: xinntao/Real-ESRGAN | BSD-3-Clause | circa 64 MB | VERIFICATO RUNTIME (test end-to-end Fase 3, sha ricalcolato = aspettato) |
| RealESRGAN x4plus (ONNX) | upscaling AI 4x | stesso repo, real_esrgan_x4.onnx, SHA-256 5c586662929cbc686c1a5c38d9c060dbdb4ea5863a1f7672b8c0761e6b89c033; alternativa: HF qualcomm/Real-ESRGAN-x4plus (Real-ESRGAN-x4plus.onnx, 67 MB) | BSD-3-Clause | 64-67 MB | VERIFICATO RUNTIME (test end-to-end Fase 3, sha ricalcolato = aspettato) |
| 4x-UltraSharp | alternativa nitidezza | HF Kim2091/UltraSharp (.pth 67 MB, cartella ONNX); OpenModelDB | CC-BY-NC-SA-4.0 NON COMMERCIALE | 64-67 MB | ESCLUSO dai modelli preconfigurati |
| CodeFormer (ONNX) | face restore | HF facefusion/models-3.0.0, codeformer.onnx, SHA-256 21710e7ab61c82683576c428e9c1b6fe1ed419586b7b39e394c3449c294b550f; pipeline confermata da hpc203/Face-restoration-models: input x f32 1x3x512x512 RGB, normalizzazione (x-0.5)/0.5, parametro w f64 in [0,1] | NTU S-Lab License 1.0: uso NON COMMERCIALE; download opzionale con avviso esplicito, nessun peso nell'installer | 377 MB | DA TESTARE |
| SCRFD (ONNX) | rilevamento volti + 5 keypoint per allineamento 512x512 | HF RuteNL/SCRFD-face-detection-ONNX; pesi ufficiali insightface; target 10g_bnkps.onnx oppure 500m.onnx per CPU lente | Apache-2.0 | 1,7-16 MB | DA TESTARE |

### 3.1 Risultati dei test runtime AI (Fase 3, 2026-10-17)

Esecuzione reale confermata su CPU con la DLL ufficiale
`onnxruntime-win-x64-1.31.0.zip` (SHA-256 ricalcolato sul pacchetto scarico).

- Test end-to-end `inference/tests/ai.rs` (variabili `PR_TEST_RUNTIME_DLL`,
  `PR_TEST_MODEL`, `PR_TEST_MODEL_X4`):
  - x2 su immagine 64x48 produce 128x96; x4 su 48x32 produce 192x128.
  - Luminosita media RGB dell'output entro +/-25 percento dell'input in entrambi i casi.
- Durata: ~163 s per i due test insieme su CPU (tessera ridotta 32-64 px nel test;
  l'app usa la tessera di produzione 256 px).
- Normalizzazione: autodetect in `Upscaler::new` (Unit `/255` scelto per i modelli
  RealESRGAN; `Centered` resta disponibile per CodeFormer).
- Bug corretto nel tiling: `starts_of` sovrascriveva l'ultimo blocco invece di
  aggiungerne uno nuovo, lasciando parte dell'immagine nera; corretto con `push`
  e coperto dal test di copertura `blocchi_coprono_l_intera_immagine`.
- Replica: `node tools/scarica-ai-test.js` scarica DLL e modelli in
  `%USERPROFILE%\.photo-restore-test`; eseguire poi
  `cargo test --workspace` con le variabili d'ambiente sopra indicate.
- Restano da completare: verifica visiva con fotografie reali nel pannello AI
  dell'app finita.

Note di licenza:
- RealESRGAN: BSD-3-Clause sui pesi; licenza direttamente compatibile con un app
  distribuita (conferma indiretta anche dalla model card qualcomm).
- CodeFormer: NTU S-Lab 1.0, uso non commerciale; l'app scarica i pesi su richiesta
  dell'utente, mostra l'avviso di licenza e non ridistribuisce nulla.
- 4x-UltraSharp: escluso; eventuale importazione manuale a responsabilita' dell'utente.

## 4. Upscaling tradizionale (Fase 2)

Con `image`/imageops, mappatura dei metodi richiesti:
- Lanczos -> filtro Lanczos3.
- Bicubic -> filtro CatmullRom.
- Bilinear -> filtro Triangle.
- Area -> box-filter (media dei pixel sorgente) implementato nel crate `core`.
- Nearest -> filtro Nearest.

Selezione per fattore o dimensioni finali, proporzioni mantenibili; nitidezza finale
facoltativa (unsharp mask dolce), disattivata per default.

## 5. Librerie Rust (Fase 2)

| Crate | Versione | Licenza | Uso |
|---|---|---|---|
| tauri | 2.12.4 | MIT OR Apache-2.0 | guscio desktop |
| tauri-plugin-dialog | 2.x | MIT OR Apache-2.0 | dialoghi file |
| image | 0.25.x | MIT OR Apache-2.0 | PNG, JPEG, TIFF, WebP, BMP; imageops |
| imageproc | 0.27.x | MIT | Otsu, Canny, contorni, morfologia |
| rayon | 1.x | MIT OR Apache-2.0 | elaborazione pixel parallela |
| serde / serde_json | 1.x | MIT OR Apache-2.0 | IPC |
| ort (Fase 3) | 2.0.0-rc.14 | MIT OR Apache-2.0 | runtime ONNX |

Tutte attive su crates.io, licenze permissive e compatibili con la distribuzione.

## 6. Gestione hardware (Fase 3)

- CPU sempre disponibile (EP predefinito).
- Rilevamento GPU e fallback automatico con avviso all'utente; macOS candidata CoreML,
  non verificabile su questa macchina (Windows): dichiarato non testato.
- Gestione modelli: consenso esplicito con dimensione e licenza, integrita SHA-256,
  pausa e ripresa del download, cartella modelli configurabile, riuso di modelli
  esistenti, cancellazione di modelli inutilizzati.

## 7. Distribuzione (Fase 4)

- Windows 10/11 x64: installer NSIS via bundle Tauri; msi opzionale.
  Verificato il 2026-10-17 (Fase 4): toolchain gnu, bundle NSIS attivo
  (`installatore ~8,6 MB`), installazione silenziosa `/S` riuscita, rilancio
  dell'app installata riuscito (`RUN_OK`), disinstallazione `/S` pulita
  (`UNINSTALL_OK`). Il pacchetto non include i modelli/della runtime AI: download
  e verifica hash restano un'attivita' esplicita dell'utente nel pannello AI.
  L'installante con toolchain MSVC resta opzionale per Fase 4 (non richiesto:
  l'attuale installer e' gia' riproducibile con la toolchain gnu).
- Linux x86_64 (gnu): AppImage, e deb quando ragionevole; build via CI.
  Non testabile su questa macchina (Windows): dichiarato non testato.
- macOS Intel e Apple Silicon: dmg; build via CI. Non testato localmente.
- L'utente finale non installa Rust, Python, ComfyUI o strumenti di sviluppo.
  WebView2 e' preinstallato su Windows 10/11 aggiornati e l'installer NSIS di Tauri
  puo' installarlo se assente.

## 8. Limiti dichiarati

1. I modelli RealESRGAN x2/x4 sono stati eseguiti con successo nei test runtime di
   Fase 3 (vedi 3.1); la verifica visiva nel pannello AI dell'app finita resta in
   programma. CodeFormer e SCRFD restano DA TESTARE.
2. Solo Windows e' verificabile su questa macchina di sviluppo; Linux e macOS vanno
   verificati in ambienti dedicati.
3. 4x-UltraSharp escluso per licenza non commerciale.
4. CodeFormer resta sotto licenza non commerciale; l'app informa l'utente al download.
5. Nessuna funzione di inpainting o ricostruzione generativa e' prevista
   nell'architettura, nell'interfaccia o nelle fasi future.
