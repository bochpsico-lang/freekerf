// Pure maths of the selection handles: from a drag to a transform operation
// (design/interaction.md §3), and where the handles are.

import type { TransformOp } from '../gen/api';
import { Camera, type CameraState, type Rect } from '../canvas/camera';
import type { DragMods, Handle } from '../canvas/interaction';

export type Point = [number, number];

/** Snap of the rotation: 15° with Shift, 5° on touch, free otherwise. */
export function rotationStep(m: DragMods): number {
  return m.shift ? 15 : m.touch ? 5 : 0;
}

const center = (b: Rect): Point => [b.x + b.width / 2, b.y + b.height / 2];

/** The fixed point of a resize: the opposite corner or side (the centre with Alt). */
function pivotFor(h: Handle, b: Rect, alt: boolean): Point {
  if (alt) return center(b);
  const x = h.includes('w') ? b.x + b.width : h.includes('e') ? b.x : b.x + b.width / 2;
  const y = h.includes('s') ? b.y + b.height : h.includes('n') ? b.y : b.y + b.height / 2;
  return [x, y];
}

/** The operation for a drag of `handle` on the selection box `b` from `start` to `cur` (world mm). */
export function dragOp(handle: Handle, b: Rect, start: Point, cur: Point, m: DragMods): TransformOp {
  if (handle === 'move') return { translate: [cur[0] - start[0], cur[1] - start[1]] };
  if (handle === 'rotate') {
    const c = center(b);
    const a0 = Math.atan2(start[1] - c[1], start[0] - c[0]);
    const a1 = Math.atan2(cur[1] - c[1], cur[0] - c[0]);
    let deg = ((a1 - a0) * 180) / Math.PI;
    const step = rotationStep(m);
    if (step) deg = Math.round(deg / step) * step;
    return { rotate_deg: deg, pivot: c };
  }
  const pivot = pivotFor(handle, b, m.alt);
  const ratio = (s: number, c: number, p: number) => (Math.abs(s - p) < 1e-9 ? 1 : (c - p) / (s - p));
  let sx = handle.includes('e') || handle.includes('w') ? ratio(start[0], cur[0], pivot[0]) : 1;
  let sy = handle.includes('n') || handle.includes('s') ? ratio(start[1], cur[1], pivot[1]) : 1;
  const corner = handle.length === 2;
  // corners keep the proportions by default; Shift frees them
  if (corner && !m.shift) {
    const k = Math.abs(sx) > Math.abs(sy) ? sx : sy;
    sx = k;
    sy = k;
  }
  return { scale: [sx, sy], pivot };
}

/** Applies an operation to a point (previews). */
export function applyOp(op: TransformOp, [x, y]: Point): Point {
  if ('translate' in op) return [x + op.translate[0], y + op.translate[1]];
  if ('matrix' in op) {
    const [a, b, c, d, e, f] = op.matrix;
    return [a * x + c * y + e, b * x + d * y + f];
  }
  const [px, py] = op.pivot;
  if ('scale' in op) return [px + (x - px) * op.scale[0], py + (y - py) * op.scale[1]];
  const a = (op.rotate_deg * Math.PI) / 180;
  const dx = x - px;
  const dy = y - py;
  return [px + dx * Math.cos(a) - dy * Math.sin(a), py + dx * Math.sin(a) + dy * Math.cos(a)];
}

/** Corners of a box, counter-clockwise from bottom-left. */
export function corners(b: Rect): Point[] {
  return [
    [b.x, b.y],
    [b.x + b.width, b.y],
    [b.x + b.width, b.y + b.height],
    [b.x, b.y + b.height],
  ];
}

/** Screen positions of the handles of a selection box; `offset` lifts the rotation handle. */
export function handlePositions(cam: CameraState, b: Rect, offset: number): [Handle, number, number][] {
  const [x0, y1] = Camera.toScreen(cam, b.x, b.y);
  const [x1, y0] = Camera.toScreen(cam, b.x + b.width, b.y + b.height);
  const mx = (x0 + x1) / 2;
  const my = (y0 + y1) / 2;
  return [
    ['nw', x0, y0],
    ['n', mx, y0],
    ['ne', x1, y0],
    ['e', x1, my],
    ['se', x1, y1],
    ['s', mx, y1],
    ['sw', x0, y1],
    ['w', x0, my],
    ['rotate', mx, y0 - offset],
  ];
}

/** The handle under a screen point (`move` inside the box), or `null`. */
export function handleAt(
  cam: CameraState,
  b: Rect,
  sx: number,
  sy: number,
  hit: number,
  offset: number,
): Handle | null {
  for (const [h, x, y] of handlePositions(cam, b, offset)) {
    if (Math.abs(sx - x) <= hit / 2 && Math.abs(sy - y) <= hit / 2) return h;
  }
  const [x0, y1] = Camera.toScreen(cam, b.x, b.y);
  const [x1, y0] = Camera.toScreen(cam, b.x + b.width, b.y + b.height);
  return sx >= x0 && sx <= x1 && sy >= y0 && sy <= y1 ? 'move' : null;
}

/** The box around the bounds of the selected objects (world), or `null`. */
export function selectionBox(bounds: ([number, number, number, number] | null)[]): Rect | null {
  let b: [number, number, number, number] | null = null;
  for (const x of bounds) {
    if (!x) continue;
    b = b ? [Math.min(b[0], x[0]), Math.min(b[1], x[1]), Math.max(b[2], x[2]), Math.max(b[3], x[3])] : [...x];
  }
  return b ? { x: b[0], y: b[1], width: b[2] - b[0], height: b[3] - b[1] } : null;
}

/** Rotation of a 2×3 affine, degrees (−180, 180]. */
export function rotationOf(m: [number, number, number, number, number, number]): number {
  const deg = (Math.atan2(m[1], m[0]) * 180) / Math.PI;
  return Math.abs(deg) < 1e-9 ? 0 : deg;
}
