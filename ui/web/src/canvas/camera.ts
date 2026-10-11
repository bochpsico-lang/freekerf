// The view of the machine table: world coordinates in millimetres (Y up, like the
// machine), screen coordinates in CSS pixels (Y down). Pure and framework-free.

/** A rectangle in world coordinates. */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Zoom limits, in CSS pixels per millimetre. */
export const MIN_SCALE = 0.05;
export const MAX_SCALE = 400;

/** Camera state: what point is at the centre of the view and how big a millimetre is. */
export interface CameraState {
  /** World point at the centre of the view, mm. */
  cx: number;
  cy: number;
  /** CSS pixels per millimetre. */
  scale: number;
  /** View size in CSS pixels. */
  width: number;
  height: number;
}

const clamp = (v: number, lo: number, hi: number): number => Math.min(hi, Math.max(lo, v));

/** Camera operations; every method returns a new state (no mutation). */
export const Camera = {
  /** A camera of `width`×`height` CSS pixels fitted to `rect`. */
  fitted(rect: Rect, width: number, height: number, marginPx = 24): CameraState {
    const w = Math.max(1, width - 2 * marginPx);
    const h = Math.max(1, height - 2 * marginPx);
    const scale = clamp(
      Math.min(w / Math.max(rect.width, 1e-6), h / Math.max(rect.height, 1e-6)),
      MIN_SCALE,
      MAX_SCALE,
    );
    return { cx: rect.x + rect.width / 2, cy: rect.y + rect.height / 2, scale, width, height };
  },

  /** Screen (CSS px) → world (mm). */
  toWorld(c: CameraState, sx: number, sy: number): [number, number] {
    return [c.cx + (sx - c.width / 2) / c.scale, c.cy - (sy - c.height / 2) / c.scale];
  },

  /** World (mm) → screen (CSS px). */
  toScreen(c: CameraState, wx: number, wy: number): [number, number] {
    return [(wx - c.cx) * c.scale + c.width / 2, c.height / 2 - (wy - c.cy) * c.scale];
  },

  /** Moves the view by a screen delta (dragging the content by `dx`,`dy`). */
  panBy(c: CameraState, dx: number, dy: number): CameraState {
    return { ...c, cx: c.cx - dx / c.scale, cy: c.cy + dy / c.scale };
  },

  /** Zooms by `factor` keeping the world point under (`sx`,`sy`) fixed. */
  zoomAt(c: CameraState, factor: number, sx: number, sy: number): CameraState {
    const scale = clamp(c.scale * factor, MIN_SCALE, MAX_SCALE);
    const [wx, wy] = Camera.toWorld(c, sx, sy);
    const z = { ...c, scale };
    const [nx, ny] = Camera.toScreen(z, wx, wy);
    return Camera.panBy(z, sx - nx, sy - ny);
  },

  /** Same centre, new size (window resized). */
  resized(c: CameraState, width: number, height: number): CameraState {
    return { ...c, width, height };
  },

  /** The world rectangle visible on screen. */
  visible(c: CameraState): Rect {
    const w = c.width / c.scale;
    const h = c.height / c.scale;
    return { x: c.cx - w / 2, y: c.cy - h / 2, width: w, height: h };
  },

  /**
   * Column-major 3×3 matrix (as 9 numbers) from world mm to WebGL clip space, for a
   * view of the given size in CSS pixels.
   */
  clipMatrix(c: CameraState): Float32Array {
    const sx = (2 * c.scale) / c.width;
    const sy = (2 * c.scale) / c.height;
    return new Float32Array([sx, 0, 0, 0, sy, 0, -c.cx * sx, -c.cy * sy, 1]);
  },
};

/**
 * A "nice" step (1, 2 or 5 × 10^n mm) so that it is at least `minPx` CSS pixels on
 * screen: used by the grid and the rulers.
 */
export function niceStep(scale: number, minPx: number): number {
  const raw = minPx / scale;
  const pow = 10 ** Math.floor(Math.log10(raw));
  for (const m of [1, 2, 5, 10]) {
    if (m * pow >= raw) return m * pow;
  }
  return 10 * pow;
}
