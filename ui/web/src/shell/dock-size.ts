// Sizes of the dock the person drags (wide layout): its width and, in Design mode,
// the height of Layers above the other panels. Kept in `ui.dock_width_px` and
// `ui.dock_layers_pct`; 0 there means the default size of the CSS.

/** Narrowest dock, in pixels. */
export const DOCK_MIN_PX = 260;
/** The dock takes at most this share of the window. */
export const DOCK_MAX_SHARE = 0.5;
/** Layers keeps at least this share of the dock, in percent (and leaves as much to the rest). */
export const LAYERS_MIN_PCT = 15;
export const LAYERS_MAX_PCT = 85;
/** Step of the arrow keys (Shift: ×4). */
export const WIDTH_STEP_PX = 16;
export const LAYERS_STEP_PCT = 2;

/** A width in pixels kept between the minimum and half the window. */
export function clampWidth(px: number, windowPx: number): number {
  const max = Math.max(DOCK_MIN_PX, Math.floor(windowPx * DOCK_MAX_SHARE));
  return Math.round(Math.min(max, Math.max(DOCK_MIN_PX, px)));
}

/** A height of Layers in percent kept between the limits. */
export function clampLayers(pct: number): number {
  return Math.round(Math.min(LAYERS_MAX_PCT, Math.max(LAYERS_MIN_PCT, pct)));
}

/** Width of a dock on the right edge of the window while its left edge is dragged to `x`. */
export function widthAt(x: number, dockRight: number, windowPx: number): number {
  return clampWidth(dockRight - x, windowPx);
}

/** Height of Layers in percent while its separator is dragged to `y` in a dock from `top` to `bottom`. */
export function layersAt(y: number, top: number, bottom: number): number {
  const h = bottom - top;
  return h > 0 ? clampLayers(((y - top) / h) * 100) : LAYERS_MIN_PCT;
}

/**
 * The new size after a key on a separator, or `null` for keys it ignores. `Home`
 * and `End` go to the limits; `current` is the size shown (the default one included).
 * Width separator: ← widens (the dock is on the right), →  narrows.
 * Layers separator: ↓ makes Layers taller, ↑ shorter.
 */
export function keyResize(
  kind: 'width' | 'layers',
  key: string,
  shift: boolean,
  current: number,
  windowPx: number,
): number | null {
  const n = shift ? 4 : 1;
  if (kind === 'width') {
    const step = WIDTH_STEP_PX * n;
    switch (key) {
      case 'ArrowLeft':
        return clampWidth(current + step, windowPx);
      case 'ArrowRight':
        return clampWidth(current - step, windowPx);
      case 'Home':
        return clampWidth(DOCK_MIN_PX, windowPx);
      case 'End':
        return clampWidth(Infinity, windowPx);
      default:
        return null;
    }
  }
  const step = LAYERS_STEP_PCT * n;
  switch (key) {
    case 'ArrowDown':
      return clampLayers(current + step);
    case 'ArrowUp':
      return clampLayers(current - step);
    case 'Home':
      return LAYERS_MIN_PCT;
    case 'End':
      return LAYERS_MAX_PCT;
    default:
      return null;
  }
}
