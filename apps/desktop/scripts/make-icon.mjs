// Generates the Stora source icon as a PNG with no external dependencies.
//
// The mark is a simple drive/storage glyph: a rounded slab with a capacity bar,
// echoing the horizontal capacity bar used throughout the interface. It is
// intentionally plain so it reads correctly at 16px in the system tray.

import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";

const SIZE = 512;

// Windows accent blue, matching the default the UI falls back to.
const ACCENT = [0x00, 0x78, 0xd4];
const SLAB = [0xf3, 0xf3, 0xf3];
const SLAB_EDGE = [0xd1, 0xd1, 0xd1];

function inRoundedRect(x, y, left, top, right, bottom, radius) {
  if (x < left || x > right || y < top || y > bottom) return false;
  const cx = Math.min(Math.max(x, left + radius), right - radius);
  const cy = Math.min(Math.max(y, top + radius), bottom - radius);
  const dx = x - cx;
  const dy = y - cy;
  return dx * dx + dy * dy <= radius * radius;
}

function pixel(x, y) {
  // Outer app tile.
  if (!inRoundedRect(x, y, 40, 40, 472, 472, 96)) return [0, 0, 0, 0];

  // Capacity bar: the filled portion sits on the accent color.
  const barTop = 300;
  const barBottom = 356;
  if (inRoundedRect(x, y, 112, barTop, 400, barBottom, 28)) {
    return x < 300 ? [...ACCENT, 255] : [...SLAB_EDGE, 255];
  }

  // Drive slab.
  if (inRoundedRect(x, y, 112, 156, 400, 268, 24)) {
    if (!inRoundedRect(x, y, 124, 168, 388, 256, 16)) {
      return [...SLAB_EDGE, 255];
    }
    return [...SLAB, 255];
  }

  return [...ACCENT, 255];
}

function buildPng() {
  const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
  let offset = 0;
  for (let y = 0; y < SIZE; y += 1) {
    raw[offset] = 0; // filter: none
    offset += 1;
    for (let x = 0; x < SIZE; x += 1) {
      const [r, g, b, a] = pixel(x, y);
      raw[offset] = r;
      raw[offset + 1] = g;
      raw[offset + 2] = b;
      raw[offset + 3] = a;
      offset += 4;
    }
  }

  const chunk = (type, data) => {
    const length = Buffer.alloc(4);
    length.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body) >>> 0);
    return Buffer.concat([length, body, crc]);
  };

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(SIZE, 0);
  ihdr.writeUInt32BE(SIZE, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

let crcTable = null;
function crc32(buffer) {
  if (!crcTable) {
    crcTable = new Int32Array(256);
    for (let n = 0; n < 256; n += 1) {
      let c = n;
      for (let k = 0; k < 8; k += 1) {
        c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      }
      crcTable[n] = c;
    }
  }
  let crc = -1;
  for (let i = 0; i < buffer.length; i += 1) {
    crc = (crc >>> 8) ^ crcTable[(crc ^ buffer[i]) & 0xff];
  }
  return crc ^ -1;
}

const target = process.argv[2] ?? "src-tauri/icons/source.png";
mkdirSync(dirname(target), { recursive: true });
writeFileSync(target, buildPng());
console.log(`wrote ${target}`);
