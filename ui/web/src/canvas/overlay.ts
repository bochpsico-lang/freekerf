// Canvas2D overlay: rulers, work area outline, the head, the job frame and the
// selection rectangle. Cheap to redraw every frame. (The grid is part of the WebGL
// scene, under the drawing.)

import { Camera, niceStep, type CameraState, type Rect } from './camera';
import type { OverlayColors } from './theme';

/** Ruler thickness, CSS pixels. */
export const RULER = 20;

/** What the overlay shows. */
export interface OverlayScene {
  workArea: Rect;
  /** Head position (mm), when known. */
  head: { x: number; y: number; burning: boolean } | null;
  /** Job frame (dashed), when known. */
  frame: Rect | null;
  /** Selection rectangle in screen pixels, while dragging; `crossing` = right-to-left. */
  marquee: { x0: number; y0: number; x1: number; y1: number; crossing: boolean } | null;
  /** Unit labels (mm or inch). */
  units: 'mm' | 'in';
  /**
   * Design mode selection: its box (world), the handles (screen positions) and,
   * while dragging, the outline the selection will have (world polygon).
   */
  selection: {
    box: Rect;
    handles: [string, number, number][];
    handleSize: number;
    ghost: [number, number][] | null;
  } | null;
}

/** Formats a ruler label (no trailing zeros). */
export function rulerLabel(mm: number, units: 'mm' | 'in'): string {
  const v = units === 'in' ? mm / 25.4 : mm;
  return String(Math.round(v * 1000) / 1000);
}

/** Draws the overlay. */
export function drawOverlay(
  ctx: CanvasRenderingContext2D,
  cam: CameraState,
  scene: OverlayScene,
  colors: OverlayColors,
  font: string,
  pixelRatio: number,
): void {
  const { width, height } = cam;
  ctx.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);
  ctx.clearRect(0, 0, width, height);
  const major = niceStep(cam.scale, 80);
  const wa = scene.workArea;
  // work area outline
  const [ax, ay] = Camera.toScreen(cam, wa.x, wa.y + wa.height);
  ctx.strokeStyle = colors.border;
  ctx.strokeRect(Math.round(ax) + 0.5, Math.round(ay) + 0.5, wa.width * cam.scale, wa.height * cam.scale);
  // job frame
  if (scene.frame) {
    const f = scene.frame;
    const [fx, fy] = Camera.toScreen(cam, f.x, f.y + f.height);
    ctx.save();
    ctx.setLineDash([6, 4]);
    ctx.strokeStyle = colors.frame;
    ctx.strokeRect(fx, fy, f.width * cam.scale, f.height * cam.scale);
    ctx.restore();
  }
  // head: a cross with a ring, filled while burning
  if (scene.head) {
    const [hx, hy] = Camera.toScreen(cam, scene.head.x, scene.head.y);
    ctx.save();
    ctx.strokeStyle = colors.head;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.arc(hx, hy, 6, 0, Math.PI * 2);
    ctx.moveTo(hx - 12, hy);
    ctx.lineTo(hx + 12, hy);
    ctx.moveTo(hx, hy - 12);
    ctx.lineTo(hx, hy + 12);
    ctx.stroke();
    if (scene.head.burning) {
      ctx.fillStyle = colors.accent;
      ctx.beginPath();
      ctx.arc(hx, hy, 3, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.restore();
  }
  if (scene.selection) drawSelection(ctx, cam, scene.selection, colors);
  // marquee: solid = window (left to right), dashed = crossing (right to left)
  if (scene.marquee) {
    const m = scene.marquee;
    ctx.save();
    ctx.strokeStyle = colors.selection;
    if (m.crossing) ctx.setLineDash([4, 3]);
    ctx.strokeRect(Math.min(m.x0, m.x1), Math.min(m.y0, m.y1), Math.abs(m.x1 - m.x0), Math.abs(m.y1 - m.y0));
    ctx.restore();
  }
  drawRulers(ctx, cam, colors, font, major, scene.units);
}

function drawSelection(
  ctx: CanvasRenderingContext2D,
  cam: CameraState,
  sel: NonNullable<OverlayScene['selection']>,
  colors: OverlayColors,
): void {
  ctx.save();
  ctx.strokeStyle = colors.selection;
  ctx.lineWidth = 1;
  const b = sel.box;
  const [x0, y0] = Camera.toScreen(cam, b.x, b.y + b.height);
  ctx.setLineDash([4, 3]);
  ctx.strokeRect(Math.round(x0) + 0.5, Math.round(y0) + 0.5, b.width * cam.scale, b.height * cam.scale);
  ctx.setLineDash([]);
  if (sel.ghost) {
    ctx.beginPath();
    sel.ghost.forEach(([x, y], i) => {
      const [sx, sy] = Camera.toScreen(cam, x, y);
      if (i) ctx.lineTo(sx, sy);
      else ctx.moveTo(sx, sy);
    });
    ctx.closePath();
    ctx.lineWidth = 1.5;
    ctx.stroke();
  } else {
    const h = sel.handleSize;
    ctx.fillStyle = colors.canvas;
    for (const [name, x, y] of sel.handles) {
      ctx.beginPath();
      if (name === 'rotate') ctx.arc(x, y, h / 2, 0, Math.PI * 2);
      else ctx.rect(Math.round(x - h / 2) + 0.5, Math.round(y - h / 2) + 0.5, h, h);
      ctx.fill();
      ctx.stroke();
    }
  }
  ctx.restore();
}

function drawRulers(
  ctx: CanvasRenderingContext2D,
  cam: CameraState,
  colors: OverlayColors,
  font: string,
  major: number,
  units: 'mm' | 'in',
): void {
  const { width, height } = cam;
  ctx.fillStyle = colors.rulerBg;
  ctx.fillRect(0, 0, width, RULER);
  ctx.fillRect(0, 0, RULER, height);
  ctx.strokeStyle = colors.rulerText;
  ctx.fillStyle = colors.rulerText;
  ctx.font = `10px ${font}`;
  ctx.textBaseline = 'top';
  const vis = Camera.visible(cam);
  const sub = major / 5;
  ctx.beginPath();
  for (let x = Math.floor(vis.x / sub) * sub; x <= vis.x + vis.width; x += sub) {
    const [sx] = Camera.toScreen(cam, x, 0);
    if (sx < RULER) continue;
    const isMajor = Math.abs(x / major - Math.round(x / major)) < 1e-6;
    const px = Math.round(sx) + 0.5;
    ctx.moveTo(px, RULER);
    ctx.lineTo(px, isMajor ? 0 : RULER - 5);
    if (isMajor) ctx.fillText(rulerLabel(x, units), px + 2, 2);
  }
  for (let y = Math.floor(vis.y / sub) * sub; y <= vis.y + vis.height; y += sub) {
    const [, sy] = Camera.toScreen(cam, 0, y);
    if (sy < RULER) continue;
    const isMajor = Math.abs(y / major - Math.round(y / major)) < 1e-6;
    const py = Math.round(sy) + 0.5;
    ctx.moveTo(RULER, py);
    ctx.lineTo(isMajor ? 0 : RULER - 5, py);
    if (isMajor) {
      ctx.save();
      ctx.translate(2, py - 2);
      ctx.rotate(-Math.PI / 2);
      ctx.fillText(rulerLabel(y, units), 0, 0);
      ctx.restore();
    }
  }
  ctx.stroke();
  ctx.fillStyle = colors.rulerBg;
  ctx.fillRect(0, 0, RULER, RULER);
  ctx.fillStyle = colors.rulerText;
  ctx.fillText(units, 3, 5);
}
