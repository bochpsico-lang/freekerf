// Canvas colours from the `fk-` design tokens (CSS custom properties), so the canvas
// follows the light, dark and high-contrast themes like the rest of the UI.

import type { Rgba, RenderColors } from './webgl2';

/** Parses `#RGB`, `#RRGGBB` or `#RRGGBBAA` (other forms give opaque grey). */
export function parseColor(text: string, alpha = 1): Rgba {
  const t = text.trim();
  const hex = t.startsWith('#') ? t.slice(1) : '';
  const full =
    hex.length === 3 || hex.length === 4
      ? hex
          .split('')
          .map((c) => c + c)
          .join('')
      : hex;
  if (!/^[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(full)) return [0.5, 0.5, 0.5, alpha];
  const n = (i: number) => parseInt(full.slice(i, i + 2), 16) / 255;
  return [n(0), n(2), n(4), (full.length === 8 ? n(6) : 1) * alpha];
}

/** Colours of the overlay (Canvas2D), as CSS strings. */
export interface OverlayColors {
  canvas: string;
  rulerBg: string;
  rulerText: string;
  border: string;
  head: string;
  selection: string;
  accent: string;
  frame: string;
}

/** Everything the canvas paints with. */
export interface CanvasTheme {
  background: Rgba;
  render: RenderColors;
  overlay: OverlayColors;
  monoFont: string;
}

export const LAYER_TOKENS = [
  'c00',
  'c01',
  'c02',
  'c03',
  'c04',
  'c05',
  'c06',
  'c07',
  'c08',
  'c09',
  'c10',
  'c11',
  't1',
];

/** Reads the theme from the computed style of `el` (call again when the theme changes). */
export function readTheme(el: Element): CanvasTheme {
  const css = getComputedStyle(el);
  const v = (name: string, fallback: string) => css.getPropertyValue(`--fk-${name}`).trim() || fallback;
  return {
    background: parseColor(v('color-surface-canvas', '#0B0D10')),
    render: {
      workarea: parseColor(v('color-surface-workarea', '#12161B')),
      gridMinor: parseColor(v('color-grid-minor', '#1A2027')),
      gridMajor: parseColor(v('color-grid-major', '#26303A')),
      travel: parseColor(v('color-text-subtle', '#808B9A'), 0.35),
      burn: parseColor(v('color-accent', '#F4A51C')),
      done: parseColor(v('color-burn', '#8A5200')),
      layers: LAYER_TOKENS.map((t) => parseColor(v(`color-layer-${t}`, '#9AA3AF'))),
    },
    overlay: {
      canvas: v('color-surface-canvas', '#0B0D10'),
      rulerBg: v('color-ruler-bg', '#151A20'),
      rulerText: v('color-ruler-text', '#808B9A'),
      border: v('color-border-strong', '#5B6675'),
      head: v('color-head', '#FFFFFF'),
      selection: v('color-selection', '#F9B23A'),
      accent: v('color-accent', '#F4A51C'),
      frame: v('color-accent-text', '#F9B23A'),
    },
    monoFont: v('font-family-mono', 'monospace'),
  };
}
