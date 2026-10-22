# Photo Restore

Applicazione desktop per il restauro di fotografie e scansioni, **100% locale**: nessun upload, nessun cloud, nessuna telemetria. I modelli AI e la runtime ONNX sono inclusi nell'installer — nessun download è richiesto al primo avvio.

- **Licenza del codice**: MIT (vedi [LICENSE](LICENSE))
- **Licenze dei modelli inclusi**: [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)
- **Decisioni tecniche**: [docs/TECHNICAL_DECISIONS.md](docs/TECHNICAL_DECISIONS.md)

> Il programma **non** ricostruisce contenuti con inpainting: ritaglia, raddrizza, corregge, ingrandisce e restaura i volti. L'immagine originale non viene mai modificata: si lavora su una copia temporanea.

## Funzioni

| Passo | Cosa fa | Come |
|---|---|---|
| Ritaglia | Individua automaticamente le foto dentro una scansione (rettangolo d'area minima) e offre ritaglio o correzione prospettica | Rilevamento contorni in Rust puro (pr-core) |
| Dimensioni | Ridimensionamento conservativo | Lanczos3 |
| Colori | Correzioni luminosità/contrasto/saturazione e bilanciamento bianchi con anteprima | Operazioni conservative |
| Sfondo (AI) | Rimozione dello sfondo con maschera solida (binarizzazione + bordo sfumato + sfondo campionato) | u2net / u2netp |
| Volti (AI) | Rileva e restaura i volti; scelta di *Restauratore* (CodeFormer / GFPGAN v1.4) e *Rilevatore* (RetinaFace R50 / Mobile 0.25) con barra di fusione | RetinaFace + codeformer/gfpgan |
| Ingrandisci (AI) | x2 / x4 con scelta del modello (Real-ESRGAN, UltraSharp); gestione automatica delle scale native (default/downscale/doppio passaggio) | real-esrgan / ultrasharp |
| Salva | PNG/JPEG/WebP senza metadati EXIF | export pulito in locale |

Interfaccia in italiano con **i18n completo** (IT/EN/ES/FR/DE/PT + lingua di sistema, modificabile al volo da ⚙ Impostazioni), marche ✓ a passi effettuati con Undo/Redo, e pulsante di donazione opzionale.

## Binari disponibili (Windows x64)

| Artefatto | Descrizione | Dimensione |
|---|---|---|
| `Photo-Restore_0.2.0_x64-setup.exe` | **Installer NSIS** — 9 modelli AI + runtime ONNX inclusi, nessun download; crea la voce in "App e funzionalità" | ~726 MiB |
| `PhotoRestore-portable-win-x64-0.2.0.zip` | **Versione portatile** — scompatta ovunque ed esegui `photo-restore-app.exe`; nessuna installazione, il programma non scrive nulla nel registro di sistema | ~750 MiB |

Requisiti Windows: WebView2 Runtime (già presente in Windows 11; su Windows 10 l'installer lo integra da solo tramite bootstrapper — la versione portatile lo usa se già installato). CPU-only: nessuna GPU richiesta.

**Verifica integrità (SHA-256)**:
- `Photo-Restore_0.2.0_x64-setup.exe`: `AF61375E355C12265D7A1FF75A2C614BB93B5C850DAD7F44F0FE650B4FFFDEB0`
- `PhotoRestore-portable-win-x64-0.2.0.zip`: `CEB72C1D5546F9FCE76838D250EBA6AD6F2D041B7451AD571D609C912B94B098`

### Portatile vs installer
- l'installer aggiunge la voce in "App e funzionalità" e gestisce l'aggiornamento di WebView2.
- la versione portatile contiene le **stesse esatte risorse** dell'installazione (photo-restore-app.exe + `resources/modelli/*.onnx` + `resources/runtime/onnxruntime.dll`): i gruppi di file di questa directory sono quelli caricati dall'applicazione, nessun file aggiuntivo.

## Per Linux e macOS

**Non vengono pubblicati binari** per altri sistemi: in questa repo si costruisce solo per Windows x64. Il codice è portabile, ma su Linux/macOS serve compilare da sorgente:

```bash
# prerequisiti Linux (Debian/Ubuntu):
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
# prerequisiti macOS:
#   xcode-select --install   +  rustup
rustup target add x86_64-pc-windows-gnu   # solo per cross-compilare da Linux verso Windows
npm install
npm run build            # costruisce e pacchetta (su Linux: AppImage/deb; su macOS: .app/.dmg)
```

Nota portabilità della runtime AI:
- la runtime ONNX attualmente impacchettata è la **DLL Windows x64** (`onnxruntime.dll`). Su Linux/macOS il codice risolve il file in `resources/runtime/` — occorre depositare lì la `libonnxruntime.so` / `libonnxruntime.dylib` della **stessa versione 1.31.0** (rinominata `onnxruntime.dll` è comunque accettata perché viene passata come path).
- i 9 modelli `.onnx` possono essere copiati nell'equivalente per sistema della cartella `%LOCALAPPDATA%\PhotoRestore\modelli` o in `resources/modelli/` prima della build (vedi `inference/src/models.rs`).
- i progressi di build non-Windows non sono ancora collaudati: considerali sperimentali.

## Sviluppo

```bash
npm install
npm run dev     # sviluppo con ricarica
npm run build   # build di produzione (Tauri bundler)
compila.cmd     # build/test/verifica/format (Windows, MinGW toolchain)
```

- Workspace Cargo: `src-tauri` (comandi Tauri) + `core` (elaborazione immagini pura, testabile senza UI) + `inference` (runtime ONNX, tiling, registrazioni modelli).
- UI statica, nessun bundler; `ui/index.html`, `ui/app.js`, `ui/i18n.js`, `ui/styles.css`.
- Test: `cargo test -p pr-inference -p pr-core` (I modelli reali sono opzionali nei test, abilitati con variabili d'ambiente).

### Struttura

```
src-tauri/       shell Tauri: comandi, eventi di progresso, stato
core/            ritaglio, prospettiva, correzioni (Rust puro)
inference/       ONNX Ort runtime, modelli, tiling, gestore risorse
ui/              interfaccia statica (italiano) + i18n
docs/            decisioni tecniche
```

## Sicurezza e privacy

- Nessun codice remoto eseguito: l'UI è statica e impacchettata, CSP rigido.
- Nessun endpoint di telemetria nel binario (verificato con scansione stringhe): gli unici URL incorporati sono le fonti dichiarate dei modelli, l'URL di donazione volontaria e le pagine di supporto WebView2.
- Nessuna trasmissione delle immagini fuori dal processo.

## Licenza

Codice sorgente: MIT. I pesi dei modelli inclusi restano sotto le licenze dei rispettivi proprietari (vedi [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)) — in particolare CodeFormer (S-Lab 1.0), UltraSharp (CC BY-NC-SA 4.0) e i RetinaFace (WIDER FACE) hanno **clausole di solo uso non commerciale**; il pacchetto si limita a distribuirli con attribuzione, come consentito dalle rispettive condizioni.
