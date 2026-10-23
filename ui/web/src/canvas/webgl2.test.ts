import { describe, expect, it } from 'vitest';

import { encodeToolpath, decodeToolpath, segmentAt, type SegmentRecord } from './toolpath';
import { bucketSegments, tileRect } from './webgl2';

describe('bucketSegments', () => {
  it('groups segments by cell without losing any', () => {
    const segs: SegmentRecord[] = [];
    for (let i = 0; i < 100; i++) {
      const x = (i * 37) % 100;
      const y = (i * 53) % 100;
      segs.push({ from: [x, y], to: [x + 1, y + 1], power: i, command: i });
    }
    const t = decodeToolpath(encodeToolpath(segs, 100));
    const { bytes, buckets } = bucketSegments(t.segments, t.segmentCount, 4);
    expect(buckets.reduce((s, b) => s + b.count, 0)).toBe(100);
    const out = decodeToolpath(
      (() => {
        const head = encodeToolpath([], 100);
        const all = new Uint8Array(head.byteLength + bytes.length);
        all.set(new Uint8Array(head));
        all.set(bytes, head.byteLength);
        new DataView(all.buffer).setUint32(8, 100, true);
        return all.buffer;
      })(),
    );
    const commands = new Set<number>();
    for (const b of buckets) {
      for (let i = b.start; i < b.start + b.count; i++) {
        const s = segmentAt(out, i);
        commands.add(s.command);
        // every segment lies inside its cell's bounds
        expect(s.from[0]).toBeGreaterThanOrEqual(b.rect.x);
        expect(s.to[0]).toBeLessThanOrEqual(b.rect.x + b.rect.width);
        expect(s.from[1]).toBeGreaterThanOrEqual(b.rect.y);
        expect(s.to[1]).toBeLessThanOrEqual(b.rect.y + b.rect.height);
      }
    }
    expect(commands.size).toBe(100);
    expect(buckets.length).toBeLessThanOrEqual(16);
  });

  it('handles empty and degenerate input', () => {
    expect(bucketSegments(new DataView(new ArrayBuffer(0)), 0).buckets).toEqual([]);
    const one = decodeToolpath(encodeToolpath([{ from: [5, 5], to: [5, 5], power: 1, command: 0 }], 1));
    const { buckets } = bucketSegments(one.segments, 1);
    expect(buckets).toHaveLength(1);
    expect(buckets[0]!.count).toBe(1);
  });
});

describe('gridLines', () => {
  it('covers the visible work area with minor and major lines', async () => {
    const { Camera } = await import('./camera');
    const { gridLines } = await import('./webgl2');
    const cam = Camera.fitted({ x: 0, y: 0, width: 100, height: 100 }, 424, 424, 12); // 4 px/mm
    const g = gridLines(cam, { x: 0, y: 0, width: 100, height: 100 });
    // minor every 5 mm (20 px), major every 20 mm (80 px): 0..100 → 21 positions per axis
    expect((g.minor.length + g.major.length) / 6).toBe(42);
    expect(g.major.length / 6).toBe(12);
    expect(gridLines(Camera.panBy(cam, 10000, 0), { x: 0, y: 0, width: 100, height: 100 })).toEqual({
      minor: [],
      major: [],
    });
  });
});

describe('tileRect', () => {
  it('places a tile of image pixels on the table, rows going down', () => {
    const img = {
      width: 4,
      height: 2,
      pixels: new Uint8Array(8),
      rect: { x: 10, y: 20, width: 8, height: 4 },
      layer: 0,
    };
    // the bottom-right quarter: x 2..4, rows 1..2
    expect(tileRect(img, [2, 1, 2, 1])).toEqual({ x: 14, y: 20, width: 4, height: 2 });
    // moved: the same tile follows the image
    expect(tileRect({ ...img, rect: { ...img.rect, x: 0 } }, [2, 1, 2, 1]).x).toBe(4);
  });
});
