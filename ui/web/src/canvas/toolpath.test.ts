import { describe, expect, it } from 'vitest';

import { decodeHead, decodeToolpath, encodeToolpath, segmentAt, ToolpathFormatError } from './toolpath';

describe('tool path buffers', () => {
  it('round-trips', () => {
    const buf = encodeToolpath(
      [
        { from: [0, 0], to: [10, 0], power: -1, command: 0 },
        { from: [10, 0], to: [10, 5], power: 800, command: 2 },
      ],
      3,
    );
    const t = decodeToolpath(buf);
    expect(t.segmentCount).toBe(2);
    expect(t.commandCount).toBe(3);
    expect(t.bounds).toEqual({ x: 0, y: 0, width: 10, height: 5 });
    expect(t.maxPower).toBe(800);
    expect(segmentAt(t, 1)).toEqual({ from: [10, 0], to: [10, 5], power: 800, command: 2 });
  });

  it('rejects malformed buffers', () => {
    expect(() => decodeToolpath(new ArrayBuffer(8))).toThrow(ToolpathFormatError);
    const bad = encodeToolpath([], 0);
    new DataView(bad).setUint32(0, 1, true);
    expect(() => decodeToolpath(bad)).toThrow('bad magic');
    const v2 = encodeToolpath([], 0);
    new DataView(v2).setUint32(4, 2, true);
    expect(() => decodeToolpath(v2)).toThrow('version 2');
    const cut = encodeToolpath([{ from: [0, 0], to: [1, 1], power: 1, command: 0 }], 1).slice(0, 50);
    expect(() => decodeToolpath(cut)).toThrow('truncated');
  });

  it('decodes head frames', () => {
    const b = new ArrayBuffer(24);
    const v = new DataView(b);
    v.setFloat32(0, 12.5, true);
    v.setFloat32(4, -3, true);
    v.setFloat32(8, 800, true);
    v.setUint32(12, 7, true);
    v.setUint32(16, 9, true);
    v.setUint32(20, 3, true);
    expect(decodeHead(b)).toEqual({
      x: 12.5,
      y: -3,
      power: 800,
      executed: 7,
      target: 9,
      running: true,
      connected: true,
    });
    expect(() => decodeHead(new ArrayBuffer(4))).toThrow(ToolpathFormatError);
  });
});

describe('the Rust encoder (shared fixture)', () => {
  it('decodes freekerf_service::preview output', async () => {
    const { readFileSync } = await import('node:fs');
    const bytes = readFileSync(`${process.cwd()}/tests/fixtures/toolpath-v1.bin`);
    const t = decodeToolpath(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
    // G0 X1 Y1 · M3 S500 · G1 X11 Y1 · G2 X21 Y1 I5 J0 (chords) · M5 · G0 X0 Y0
    expect(t.commandCount).toBe(6);
    expect(t.maxPower).toBe(500);
    expect(segmentAt(t, 0)).toEqual({ from: [0, 0], to: [1, 1], power: -1, command: 0 });
    expect(segmentAt(t, 1)).toEqual({ from: [1, 1], to: [11, 1], power: 500, command: 2 });
    const arc = Array.from({ length: t.segmentCount }, (_, i) => segmentAt(t, i)).filter(
      (s) => s.command === 3,
    );
    expect(arc.length).toBeGreaterThan(10);
    expect(arc[0]!.from).toEqual([11, 1]);
    expect(arc.at(-1)!.to).toEqual([21, 1]);
    expect(segmentAt(t, t.segmentCount - 1)).toMatchObject({ to: [0, 0], power: -1, command: 5 });
    expect(t.bounds.x).toBe(0);
    expect(t.bounds.width).toBe(21);
  });
});
