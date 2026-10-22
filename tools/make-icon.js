// Genera src-tauri/icons/icon.ico: quadrato con gradiente e lettera "P".
// Nessuna dipendenza esterna; scrive ICO con bitmap DIB 32bpp.
const fs = require('fs');
const path = require('path');

function lerp(a, b, t) { return a + (b - a) * t; }

function drawLogo(size) {
  // buffer RGBA top-down per semplicita': qui costruiamo BGRA bottom-up per la DIB
  const xor = Buffer.alloc(size * size * 4);
  const and = Buffer.alloc(size * ((size + 31) >> 5) * 4); // 1bpp rows padded a 4 byte
  const cx = size / 2;
  for (let y = 0; y < size; y++) {
    const srcY = size - 1 - y; // bottom-up
    for (let x = 0; x < size; x++) {
      const i = (y * size + x) * 4;
      let r, g, b, a = 255;
      // sfondo: gradiente diagonale blu -> indaco
      const t = (x + y) / (2 * size);
      r = Math.round(lerp(30, 70, t));
      g = Math.round(lerp(90, 60, t));
      b = Math.round(lerp(200, 160, t)) + 10;
      // angoli arrotondati
      const rad = size * 0.22;
      const dx = x < rad ? rad - x : x > size - rad ? x - (size - rad) : 0;
      const dy = y < rad ? rad - y : y > size - rad ? y - (size - rad) : 0;
      const d = Math.sqrt(dx * dx + dy * dy);
      a = Math.round(Math.max(0, Math.min(1, rad - d + 0.5)) * 255);
      // lettera P stilizzata (blocchi semplici)
      const px = x / size, py = y / size;
      const bar = px > 0.30 && px < 0.46 && py > 0.18 && py < 0.82;
      const bowl = ((px >= 0.46 && px < 0.72 && py > 0.18 && py < 0.30) ||
                    (px >= 0.46 && px < 0.72 && py > 0.44 && py < 0.56) ||
                    (px >= 0.60 && px < 0.72 && py >= 0.30 && py < 0.44));
      const white = bar || (bowl && true);
      const shade = (py >= 0.30 && py < 0.44 && px >= 0.46 && px < 0.60);
      if (a > 200 && white) { r = 255; g = 255; b = 255; }
      else if (a > 200 && shade) { r = 220; g = 230; b = 255; }
      const off = (srcY * size + x) * 4;
      xor[off] = b; xor[off + 1] = g; xor[off + 2] = r; xor[off + 3] = a;
      // AND mask: 1 = trasparente
      if (a === 0) {
        const andOff = srcY * (((size + 31) >> 5) * 4) + (x >> 3);
        and[andOff] |= 0x80 >> (x & 7);
      }
    }
  }
  return { xor, and };
}

function bitmapHeader(size) {
  const head = Buffer.alloc(40);
  head.writeUInt32LE(40, 0);
  head.writeInt32LE(size, 4);
  head.writeInt32LE(size * 2, 8); // XOR + AND
  head.writeUInt16LE(1, 12);
  head.writeUInt16LE(32, 14);
  head.writeUInt32LE(0, 16);
  head.writeUInt32LE(size * size * 4 + andLen(size), 20);
  return head;
}

function andLen(size) {
  return ((size + 31) >> 5) * 4 * size;
}

function buildIco() {
  const sizes = [16, 32, 48, 256];
  const imgs = [];
  for (const s of sizes) {
    const { xor, and } = drawLogo(s);
    const bi = bitmapHeader(s);
    imgs.push({ size: s, bi, xor, and, bytes: 40 + xor.length + and.length });
  }
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2); // icon
  header.writeUInt16LE(imgs.length, 4);
  let dirSize = 6 + imgs.length * 16;
  const dir = Buffer.alloc(imgs.length * 16);
  let off = dirSize;
  imgs.forEach((im, i) => {
    const e = i * 16;
    dir[e] = im.size === 256 ? 0 : im.size;
    dir[e + 1] = im.size === 256 ? 0 : im.size;
    dir[e + 2] = 0;
    dir[e + 3] = 0;
    dir.writeUInt16LE(1, e + 4);
    dir.writeUInt16LE(32, e + 6);
    dir.writeUInt32LE(im.bytes, e + 8);
    dir.writeUInt32LE(off, e + 12);
    off += im.bytes;
  });
  const parts = [header, dir];
  for (const im of imgs) parts.push(im.bi, im.xor, im.and);
  const out = path.join(__dirname, '..', 'src-tauri', 'icons', 'icon.ico');
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, Buffer.concat(parts));
  console.log('scritto', out, Buffer.concat(parts).length, 'byte');
}

buildIco();
