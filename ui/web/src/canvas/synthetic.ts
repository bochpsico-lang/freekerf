// Synthetic data for the canvas performance check (PLAN M4a): many small closed
// paths, like converted text, and a large dithered image. Deterministic (seeded).

import type { Rect } from './camera';
import { SEGMENT_BYTES } from './toolpath';
import type { PathBatch, RasterImage } from './webgl2';

/** Small deterministic PRNG (mulberry32). */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** `count` closed wobbly paths of 12-48 segments, 2-8 mm wide, spread over `area`. */
export function syntheticPaths(count: number, area: Rect, seed = 1): PathBatch & { pathCount: number } {
  const r = rng(seed);
  const sizes: number[] = [];
  let total = 0;
  for (let i = 0; i < count; i++) {
    const n = 12 + Math.floor(r() * 37);
    sizes.push(n);
    total += n;
  }
  const buf = new ArrayBuffer(total * SEGMENT_BYTES);
  const v = new DataView(buf);
  let o = 0;
  const cols = Math.ceil(Math.sqrt(count * (area.width / area.height)));
  const cell = area.width / cols;
  for (let i = 0; i < count; i++) {
    const n = sizes[i]!;
    const cx = area.x + (i % cols) * cell + cell / 2;
    const cy = area.y + Math.floor(i / cols) * cell + cell / 2;
    const radius = Math.min(cell * 0.45, 1 + r() * 3);
    const wobble = 0.15 + r() * 0.35;
    const layer = i % 12;
    let px = 0;
    let py = 0;
    for (let k = 0; k <= n; k++) {
      const a = (k / n) * Math.PI * 2;
      const rr = radius * (1 + wobble * Math.sin(a * 3 + i));
      const x = cx + rr * Math.cos(a);
      const y = cy + rr * Math.sin(a);
      if (k > 0) {
        v.setFloat32(o, px, true);
        v.setFloat32(o + 4, py, true);
        v.setFloat32(o + 8, x, true);
        v.setFloat32(o + 12, y, true);
        v.setFloat32(o + 16, layer, true);
        v.setUint32(o + 20, i, true);
        o += SEGMENT_BYTES;
      }
      px = x;
      py = y;
    }
  }
  return { segments: new DataView(buf), segmentCount: total, pathCount: count };
}

const BAYER4 = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

/** A `width`×`height` 1-bit-looking image (ordered dither of a landscape-like field). */
export function syntheticImage(width: number, height: number, rect: Rect, layer = 0): RasterImage {
  const pixels = new Uint8Array(width * height);
  for (let y = 0; y < height; y++) {
    const fy = y / height;
    const row = y * width;
    const by = (y & 3) * 4;
    for (let x = 0; x < width; x++) {
      const fx = x / width;
      const g =
        0.5 +
        0.25 * Math.sin(fx * 9 + Math.sin(fy * 5) * 2) * Math.cos(fy * 7) +
        0.25 * Math.sin((fx + fy) * 23) * (1 - fy);
      const threshold = (BAYER4[by + (x & 3)]! + 0.5) / 16;
      pixels[row + x] = g > threshold ? 255 : 0;
    }
  }
  return { width, height, pixels, rect, layer };
}
