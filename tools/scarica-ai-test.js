// Scarica runtime AI + modelli per i test (Node 24, con curl.exe -L).
const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const dest = path.join(process.env.USERPROFILE, '.photo-restore-test');
fs.mkdirSync(dest, { recursive: true });

function dl(url, out) {
  if (fs.existsSync(out)) {
    console.log('esiste', path.basename(out), Math.round(fs.statSync(out).size / 1048576) + 'MB');
    return;
  }
  console.log('scarico', path.basename(out), '…');
  execSync(`curl.exe -L --fail --retry 3 -A "photo-restore-test" -o "${out}" "${url}"`, {
    stdio: 'inherit',
  });
}

function unzip(zip, inner, out) {
  // inner: percorso interno da saltare; estraiamo tutto e usiamo il file mancante
  execSync(
    `powershell -NoProfile -Command "Expand-Archive -Force '${zip}' '${path.join(dest, "runtime")}'"`,
    { stdio: 'inherit' }
  );
  const found = path.join(
    dest,
    'runtime',
    inner.replace(/\\[^\\]+$/, (m, i, s) => '\\' + path.basename(inner))
  );
  if (!fs.existsSync(found)) throw new Error('estratto insufficiente: ' + inner);
}

function copyRuntime() {
  const from = path.join(dest, 'runtime', 'onnxruntime-win-x64-1.31.0', 'lib', 'onnxruntime.dll');
  if (fs.existsSync(from)) fs.copyFileSync(from, path.join(dest, 'onnxruntime.dll'));
}

const RUNTIME = 'https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/onnxruntime-win-x64-1.31.0.zip';
const M2 = 'https://huggingface.co/SceneWorks/real-esrgan-onnx/resolve/main/real_esrgan_x2.onnx';
const M4 = 'https://huggingface.co/SceneWorks/real-esrgan-onnx/resolve/main/real_esrgan_x4.onnx';

const dll = path.join(dest, 'onnxruntime.dll');
if (!fs.existsSync(dll)) {
  const zipPath = path.join(dest, 'onnxruntime.zip');
  dl(RUNTIME, zipPath);
  unzip(zipPath, 'onnxruntime-win-x64-1.31.0\\onnxruntime.dll', dll);
  copyRuntime();
  if (!fs.existsSync(dll)) throw new Error('DLL non estratta');
  fs.unlinkSync(zipPath);
}
console.log('DLL ok:', dll);

const m2 = path.join(dest, 'real_esrgan_x2.onnx');
dl(M2, m2);
const m4 = path.join(dest, 'real_esrgan_x4.onnx');
dl(M4, m4);
console.log('tutto pronto in', dest);
