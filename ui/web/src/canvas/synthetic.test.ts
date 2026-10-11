import { describe, expect, it } from 'vitest';

import { rng, syntheticImage, syntheticPaths } from './synthetic';
import { SEGMENT_BYTES } from './toolpath';

describe('synthetic data', () => {
  it('is deterministic', () => {
    const a = rng(7);
    const b = rng(7);
    expect([a(), a(), a()]).toEqual([b(), b(), b()]);
  });

  it('builds closed paths inside the area', () => {
    const p = syntheticPaths(100, { x: 0, y: 0, width: 400, height: 400 });
    expect(p.pathCount).toBe(100);
    expect(p.segmentCount).toBeGreaterThanOrEqual(1200);
    expect(p.segmentCount).toBeLessThanOrEqual(4800);
    expect(p.segments.byteLength).toBe(p.segmentCount * SEGMENT_BYTES);
    for (let i = 0; i < p.segmentCount; i++) {
      const x = p.segments.getFloat32(i * SEGMENT_BYTES, true);
      const y = p.segments.getFloat32(i * SEGMENT_BYTES + 4, true);
      expect(x).toBeGreaterThan(-1);
      expect(x).toBeLessThan(401);
      expect(y).toBeGreaterThan(-1);
      expect(y).toBeLessThan(401);
    }
  });

  it('builds a dithered image', () => {
    const img = syntheticImage(64, 32, { x: 0, y: 0, width: 64, height: 32 });
    expect(img.pixels.length).toBe(64 * 32);
    const values = new Set(img.pixels);
    expect([...values].sort()).toEqual([0, 255]);
  });
});
