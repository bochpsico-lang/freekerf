import { describe, expect, it } from 'vitest';

import { Camera, MAX_SCALE, MIN_SCALE, niceStep } from './camera';

const table = { x: 0, y: 0, width: 400, height: 300 };

describe('Camera', () => {
  it('fits a rectangle with a margin', () => {
    const c = Camera.fitted(table, 848, 648, 24);
    expect(c.scale).toBeCloseTo(2);
    expect([c.cx, c.cy]).toEqual([200, 150]);
    expect(Camera.toScreen(c, 0, 0)).toEqual([24, 624]);
    expect(Camera.toScreen(c, 400, 300)).toEqual([824, 24]);
  });

  it('round-trips screen and world', () => {
    const c = Camera.fitted(table, 1000, 700);
    const [wx, wy] = Camera.toWorld(c, 123, 456);
    const [sx, sy] = Camera.toScreen(c, wx, wy);
    expect(sx).toBeCloseTo(123);
    expect(sy).toBeCloseTo(456);
  });

  it('pans with the content under the pointer', () => {
    const c = Camera.fitted(table, 848, 648, 24);
    const p = Camera.panBy(c, 10, 20);
    // the world point that was at (100,100) is now at (110,120)
    const w = Camera.toWorld(c, 100, 100);
    expect(Camera.toScreen(p, w[0], w[1])).toEqual([110, 120]);
  });

  it('zooms around the pointer and clamps', () => {
    const c = Camera.fitted(table, 848, 648, 24);
    const w = Camera.toWorld(c, 300, 200);
    const z = Camera.zoomAt(c, 2, 300, 200);
    expect(z.scale).toBeCloseTo(4);
    const s = Camera.toScreen(z, w[0], w[1]);
    expect(s[0]).toBeCloseTo(300);
    expect(s[1]).toBeCloseTo(200);
    expect(Camera.zoomAt(c, 1e9, 0, 0).scale).toBe(MAX_SCALE);
    expect(Camera.zoomAt(c, 1e-9, 0, 0).scale).toBe(MIN_SCALE);
  });

  it('reports the visible rectangle and keeps the centre on resize', () => {
    const c = Camera.fitted(table, 848, 648, 24);
    expect(Camera.visible(c)).toEqual({ x: -12, y: -12, width: 424, height: 324 });
    const r = Camera.resized(c, 400, 400);
    expect([r.cx, r.cy, r.scale]).toEqual([c.cx, c.cy, c.scale]);
  });

  it('maps world to clip space', () => {
    const c = Camera.fitted(table, 848, 648, 24);
    const m = Camera.clipMatrix(c);
    const clip = (x: number, y: number) => [m[0]! * x + m[6]!, m[4]! * y + m[7]!];
    const [ox, oy] = clip(200, 150);
    expect(ox).toBeCloseTo(0);
    expect(oy).toBeCloseTo(0);
    const [x, y] = clip(400, 300);
    expect(x).toBeCloseTo(800 / 848);
    expect(y).toBeCloseTo(600 / 648);
  });
});

describe('niceStep', () => {
  it('picks 1-2-5 steps of at least minPx', () => {
    expect(niceStep(1, 50)).toBe(50);
    expect(niceStep(2, 50)).toBe(50);
    expect(niceStep(3, 50)).toBe(20);
    expect(niceStep(10, 50)).toBe(5);
    expect(niceStep(100, 50)).toBe(0.5);
    expect(niceStep(0.1, 50)).toBe(500);
  });
});
