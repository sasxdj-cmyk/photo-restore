# Photo Restore

**Language**: English | [Italiano](README.it.md)

Desktop application for restoring photographs and scanned prints, **100% local**: no uploads, no cloud, no telemetry. Just download the release asset — **everything is included in the package**: all 9 AI models and the ONNX runtime ship inside the installer (and in the portable version), so **no downloads are ever required**, not even on first run.

- **Code license**: MIT (see [LICENSE](LICENSE))
- **Bundled model licenses**: [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)
- **Technical decisions**: [docs/TECHNICAL_DECISIONS.md](docs/TECHNICAL_DECISIONS.md) (Italian)

> The program does **not** rebuild content with inpainting: it crops, straightens, corrects, upscales and restores faces. The original image is never modified — all work happens on a temporary copy.

## Features

| Step | What it does | How |
|---|---|---|
| Crop | Automatically finds photos inside a scan (minimum-area rectangle); offers crop or perspective correction | Contour detection in pure Rust (pr-core) |
| Resize | Conservative resampling | Lanczos3 |
| Colors | Brightness/contrast/saturation and white-balance corrections with preview | Conservative operations |
| Background (AI) | Background removal with a solid mask (binarization + feathered edge + sampled background) | u2net / u2netp |
| Faces (AI) | Detects and restores faces; choose the *Restorer* (CodeFormer / GFPGAN v1.4) and the *Detector* (RetinaFace R50 / Mobile 0.25), with a fusion slider | RetinaFace + codeformer/gfpgan |
| Upscale (AI) | 2x / 4x with model choice (Real-ESRGAN, UltraSharp); native-scale handling (downscale / double pass) is automatic | real-esrgan / ultrasharp |
| Save | PNG/JPEG/WebP with no EXIF metadata | Clean local export |

Italian-first interface with **full i18n** (IT/EN/ES/FR/DE/PT + system language, switchable live from the Settings panel), check marks on completed steps with Undo/Redo, and an optional donation button.

## Published binaries (Windows x64)

| Asset | Description | Size |
|---|---|---|
| `Photo-Restore_0.2.0_x64-setup.exe` | **NSIS installer** — 9 AI models + ONNX runtime bundled, no downloads; adds an entry under "Apps & features" | ~726 MiB |
| `PhotoRestore-portable-win-x64-0.2.0.zip` | **Portable version** — unzip anywhere and run `photo-restore-app.exe`; no installation, nothing written to the registry | ~750 MiB |

Windows requirements: WebView2 Runtime (already present on Windows 11; on Windows 10 the installer provisions it via bootstrapper — the portable version uses it if installed). CPU-only: no GPU required.

Download both from the [v0.2.0 release page](https://github.com/sasxdj-cmyk/photo-restore/releases/tag/v0.2.0).

**Integrity check (SHA-256)**:
- `Photo-Restore_0.2.0_x64-setup.exe`: `AF61375E355C12265D7A1FF75A2C614BB93B5C850DAD7F44F0FE650B4FFFDEB0`
- `PhotoRestore-portable-win-x64-0.2.0.zip`: `CEB72C1D5546F9FCE76838D250EBA6AD6F2D041B7451AD571D609C912B94B098`

### Portable vs installer
- The installer registers the app under "Apps & features" and handles WebView2 updating.
- The portable version carries the **exact same resources** as the installation (`photo-restore-app.exe` + `resources/modelli/*.onnx` + `resources/runtime/onnxruntime.dll`): no extra files, nothing else to fetch.

## Linux and macOS

**No binaries are published for other platforms**: this repository only builds Windows x64 artifacts. The code is portable, but on Linux/macOS you compile from source:

```bash
# Linux prerequisites (Debian/Ubuntu):
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
# macOS prerequisites:
#   xcode-select --install   +  rustup
npm install
npm run build            # builds + packages (Linux: AppImage/deb; macOS: .app/.dmg)
```

Portability notes for the AI runtime:
- The ONNX runtime currently bundled is the **Windows x64 DLL** (`onnxruntime.dll`). On Linux/macOS the code resolves the file in `resources/runtime/` — you need to place the `libonnxruntime.so` / `libonnxruntime.dylib` of the **same version 1.31.0** there.
- The 9 `.onnx` models can be copied into the OS-equivalent of `%LOCALAPPDATA%\PhotoRestore\modelli` or into `resources/modelli/` before building (see `inference/src/models.rs`).
- Non-Windows builds are not battle-tested yet: treat them as experimental.

## Development

```bash
npm install
npm run dev     # development with reload (Windows)
npm run build   # production build (Tauri bundler)
compila.cmd     # build/test/check/format helper (Windows, MinGW toolchain)
```

- Cargo workspace: `src-tauri` (Tauri commands) + `core` (pure image processing, testable without UI) + `inference` (ONNX runtime, tiling, model registry).
- Static UI, no bundler: `ui/index.html`, `ui/app.js`, `ui/i18n.js`, `ui/styles.css`.
- Tests: `cargo test -p pr-inference -p pr-core` (real-model tests are opt-in via environment variables).

### Structure

```
src-tauri/       Tauri shell: commands, progress events, state
core/            crop, perspective, corrections (pure Rust)
inference/       ONNX runtime, models, tiling, resource manager
ui/              static interface (Italian-first) + i18n
docs/            technical decisions (Italian)
```

## Security and privacy

- No remote code execution: the UI is static and bundled, with a strict CSP.
- No telemetry endpoints in the binary (verified with a strings scan): the only embedded URLs are the declared model sources, the optional donation link and the WebView2 support pages.
- Images never leave the process.

## License

Source code: MIT. The bundled model weights remain under their respective owners' licenses (see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)) — in particular CodeFormer (S-Lab 1.0), UltraSharp (CC BY-NC-SA 4.0) and both RetinaFace detectors (WIDER FACE) are **non-commercial use only**; the package merely redistributes them with attribution, as their terms permit.
