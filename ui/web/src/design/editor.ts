// The Design mode editor: what the canvas gestures do to the document. Framework
// free: it reads the document summary and the camera, and changes the document only
// through `document.*` actions (doc/features/design-mode.md). Local, visual state
// (the drag in progress and its preview) stays here until the release sends one
// `document.transform` (one undo step).

import { Camera, type CameraState, type Rect } from '../canvas/camera';
import type { DragMods, Handle } from '../canvas/interaction';
import type { OverlayScene } from '../canvas/overlay';
import type { ObjectSummary, Tool, TransformOp } from '../gen/api';
import type { AppService } from '../service';
import type { DocumentSummary } from '../state/document.svelte';
import { applyOp, corners, dragOp, handleAt, handlePositions, selectionBox, type Point } from './transform';

/** Sizes in CSS pixels: drawn handle, hit area, rotation handle offset. */
export const HANDLES = {
  mouse: { size: 8, hit: 14, offset: 24 },
  touch: { size: 14, hit: 36, offset: 36 },
} as const;
/** Pick tolerance around a shape, CSS pixels. */
export const PICK_PX = 4;
/** Defaults of shapes placed with a click, mm. */
export const DEFAULT_SIZE = { rect: [40, 30], ellipse: [30, 30], text: 10 } as const;
/** A drag box smaller than this (mm) is a click. */
const MIN_BOX_MM = 1;

export interface EditorHost {
  doc(): DocumentSummary | null;
  tool(): Tool;
  camera(): CameraState | null;
  /** Coarse pointer (touch): bigger handles. */
  coarse(): boolean;
  service: AppService;
  /** Runs an action and reports a failure (the app's `run`). */
  run(action: () => Promise<unknown>): Promise<void>;
  /** Back to the select tool after placing a shape (`view.set_tool`). */
  selectTool(): void;
  /** A text was placed: show the inspector to type it. */
  textPlaced?(id: number): void;
  /** The preview changed (redraw the overlay). */
  changed(): void;
}

const inside = (b: [number, number, number, number], r: Rect) =>
  b[0] >= r.x && b[2] <= r.x + r.width && b[1] >= r.y && b[3] <= r.y + r.height;
const touches = (b: [number, number, number, number], r: Rect) =>
  b[0] <= r.x + r.width && b[2] >= r.x && b[1] <= r.y + r.height && b[3] >= r.y;

export class DesignEditor {
  private dragging: { handle: Handle; box: Rect; start: Point; op: TransformOp | null } | null = null;

  constructor(private readonly host: EditorHost) {}

  private get handles() {
    return HANDLES[this.host.coarse() ? 'touch' : 'mouse'];
  }

  /** Objects that can be picked (on a visible layer), in drawing order. */
  private pickable(): ObjectSummary[] {
    const d = this.host.doc();
    if (!d) return [];
    const visible = new Set(d.layers.filter((l) => l.visible).map((l) => l.id));
    return d.objects.filter((o) => o.bounds && visible.has(o.layer));
  }

  /** The selection box (world), or `null`. */
  box(): Rect | null {
    const d = this.host.doc();
    if (!d || !d.selection.length) return null;
    const ids = new Set(d.selection);
    return selectionBox(d.objects.filter((o) => ids.has(o.id)).map((o) => o.bounds ?? null));
  }

  /** What the overlay draws for the selection. */
  overlay(): OverlayScene['selection'] {
    const cam = this.host.camera();
    const box = this.box();
    if (!cam || !box) return null;
    const h = this.handles;
    const op = this.dragging?.op;
    return {
      box,
      handles: handlePositions(cam, box, h.offset),
      handleSize: h.size,
      ghost: op ? corners(box).map((p) => applyOp(op, p)) : null,
    };
  }

  /** The topmost object at a world point (bounds plus a few pixels). */
  hitTest = (wx: number, wy: number): string | null => {
    const cam = this.host.camera();
    const tol = cam ? PICK_PX / cam.scale : 0;
    const objects = this.pickable();
    for (let i = objects.length - 1; i >= 0; i--) {
      const o = objects[i]!;
      const [x0, y0, x1, y1] = o.bounds!;
      if (wx >= x0 - tol && wx <= x1 + tol && wy >= y0 - tol && wy <= y1 + tol) return String(o.id);
    }
    return null;
  };

  select = (id: string | null, additive: boolean, keepIfSelected = false): void => {
    const d = this.host.doc();
    if (id === null) {
      if (!additive && d?.selection.length) void this.dispatchSelect([], 'replace');
      return;
    }
    const n = Number(id);
    if (keepIfSelected && d?.selection.includes(n)) return;
    void this.dispatchSelect([n], additive ? 'toggle' : 'replace');
  };

  selectArea = (rect: Rect, crossing: boolean, additive: boolean): void => {
    const ids = this.pickable()
      .filter((o) => (crossing ? touches : inside)(o.bounds!, rect))
      .map((o) => o.id);
    void this.dispatchSelect(ids, additive ? 'add' : 'replace');
  };

  private dispatchSelect(ids: number[], mode: 'replace' | 'add' | 'toggle') {
    return this.host.run(() => this.host.service.dispatch('document.select', { ids, mode }));
  }

  hitHandle = (sx: number, sy: number): Handle | null => {
    const cam = this.host.camera();
    const box = this.box();
    if (!cam || !box) return null;
    const h = this.handles;
    return handleAt(cam, box, sx, sy, h.hit, h.offset);
  };

  drag = (phase: 'start' | 'move' | 'end' | 'cancel', handle: Handle, world: Point, mods: DragMods): void => {
    if (phase === 'start') {
      const box = this.box();
      this.dragging = box ? { handle, box, start: world, op: null } : null;
      return;
    }
    const d = this.dragging;
    if (!d) return;
    if (phase === 'cancel') {
      this.dragging = null;
      this.host.changed();
      return;
    }
    d.op = dragOp(d.handle, d.box, d.start, world, mods);
    if (phase === 'move') {
      this.host.changed();
      return;
    }
    this.dragging = null;
    const op = d.op;
    const ids = this.host.doc()?.selection ?? [];
    this.host.changed();
    if (ids.length && !isIdentity(op)) {
      void this.host.run(() => this.host.service.dispatch('document.transform', { ids, op }));
    }
  };

  /** True while a handle drag is in progress. */
  get busy(): boolean {
    return this.dragging !== null;
  }

  placing = (): boolean => this.host.tool() !== 'select';

  place = (rect: Rect): void => {
    const tool = this.host.tool();
    const s = this.host.service;
    const click = rect.width < MIN_BOX_MM && rect.height < MIN_BOX_MM;
    void this.host.run(async () => {
      if (tool === 'rect' || tool === 'ellipse') {
        const [w, h] = DEFAULT_SIZE[tool];
        const r = click ? { x: rect.x - w / 2, y: rect.y - h / 2, width: w, height: h } : rect;
        await s.dispatch('document.add_shape', { shape: tool, ...r });
      } else if (tool === 'text') {
        const height_mm = click ? DEFAULT_SIZE.text : Math.max(MIN_BOX_MM, rect.height);
        const { id } = await s.dispatch('document.add_text', {
          text: 'Text',
          x: rect.x,
          y: rect.y,
          height_mm,
        });
        this.host.textPlaced?.(id);
      }
    });
    this.host.selectTool();
  };

  /** Moves the selection by a step (arrow keys): 1 mm, Shift 10 mm, Alt 0.1 mm. */
  nudge(dx: number, dy: number, mods: { shift: boolean; alt: boolean }): Promise<void> {
    const ids = this.host.doc()?.selection ?? [];
    if (!ids.length) return Promise.resolve();
    const k = mods.shift ? 10 : mods.alt ? 0.1 : 1;
    return this.host.run(() =>
      this.host.service.dispatch('document.transform', { ids, op: { translate: [dx * k, dy * k] } }),
    );
  }

  /** World point of a screen point (tests and keyboard placing). */
  toWorld(sx: number, sy: number): Point | null {
    const cam = this.host.camera();
    return cam ? Camera.toWorld(cam, sx, sy) : null;
  }
}

function isIdentity(op: TransformOp): boolean {
  if ('translate' in op) return op.translate[0] === 0 && op.translate[1] === 0;
  if ('scale' in op) return op.scale[0] === 1 && op.scale[1] === 1;
  if ('matrix' in op) return op.matrix.every((v, i) => v === [1, 0, 0, 1, 0, 0][i]);
  return op.rotate_deg === 0;
}
