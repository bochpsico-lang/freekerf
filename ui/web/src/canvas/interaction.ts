// Pointer and wheel gestures of the canvas (design/interaction.md §3), as a
// framework-free state machine: DOM events go in, camera changes and intents
// (select, marquee) come out. Pan and zoom are view-only: they never change the
// document. Multi-touch is best effort (ADR 0006).

import { Camera, type CameraState, type Rect } from './camera';

/** The parts of a pointer event the canvas needs. */
export interface PointerInput {
  id: number;
  /** CSS pixels relative to the canvas. */
  x: number;
  y: number;
  /** 0 left, 1 middle, 2 right. */
  button: number;
  type: 'mouse' | 'pen' | 'touch';
  shift: boolean;
  ctrl: boolean;
  alt: boolean;
}

/** A wheel event (pixels). */
export interface WheelInput {
  x: number;
  y: number;
  dx: number;
  dy: number;
  shift: boolean;
  ctrl: boolean;
}

/** What the canvas tells its owner. */
export interface InteractionHost {
  /** `design`: drag on empty = marquee; `produce`: drag on empty = pan. */
  mode(): 'design' | 'produce';
  camera(): CameraState;
  setCamera(c: CameraState): void;
  /** Object under a world point, if any. */
  hitTest(wx: number, wy: number): string | null;
  /** Click on an object (or on nothing: `null`). */
  select(id: string | null, additive: boolean): void;
  /** Marquee finished: a world rectangle; `crossing` selects what it touches. */
  selectArea(rect: Rect, crossing: boolean, additive: boolean): void;
  /** Marquee in screen pixels while dragging (`null` when done). */
  marquee(m: { x0: number; y0: number; x1: number; y1: number; crossing: boolean } | null): void;
  /** Context menu (right click or long press) at a screen point. */
  contextMenu?(x: number, y: number, id: string | null): void;
}

/** Movement before a press becomes a drag: 3 px with a mouse, 8 px with a finger. */
export const DRAG_THRESHOLD = { mouse: 3, pen: 3, touch: 8 } as const;
/** Long press for the context menu, ms. */
export const LONG_PRESS_MS = 500;
/** Zoom per wheel "line" (100 px of delta). */
export const WHEEL_ZOOM = 1.2;

type State =
  | { kind: 'idle' }
  | { kind: 'press'; p: PointerInput; onObject: string | null; at: number }
  | { kind: 'pan'; id: number; x: number; y: number }
  | { kind: 'marquee'; p: PointerInput; x: number; y: number }
  | { kind: 'pinch'; a: number; b: number };

/** The gesture state machine. */
export class CanvasInteraction {
  private state: State = { kind: 'idle' };
  private readonly pointers = new Map<number, { x: number; y: number }>();
  private space = false;
  private longPress: ReturnType<typeof setTimeout> | null = null;

  constructor(private readonly host: InteractionHost) {}

  /** Space held: drags pan. */
  setSpace(down: boolean): void {
    this.space = down;
  }

  /** True while a gesture is in progress (the owner may skip expensive updates). */
  get busy(): boolean {
    return this.state.kind !== 'idle';
  }

  pointerDown(p: PointerInput): void {
    this.pointers.set(p.id, { x: p.x, y: p.y });
    if (this.pointers.size === 2 && p.type === 'touch') {
      // a second finger cancels whatever the first one was doing (interaction.md §3)
      this.cancelLongPress();
      if (this.state.kind === 'marquee') this.host.marquee(null);
      const [a, b] = [...this.pointers.keys()] as [number, number];
      this.state = { kind: 'pinch', a, b };
      return;
    }
    if (this.state.kind !== 'idle') return;
    if (p.button === 1 || this.space) {
      this.state = { kind: 'pan', id: p.id, x: p.x, y: p.y };
      return;
    }
    if (p.button === 2) {
      const [wx, wy] = Camera.toWorld(this.host.camera(), p.x, p.y);
      this.host.contextMenu?.(p.x, p.y, this.host.hitTest(wx, wy));
      return;
    }
    const [wx, wy] = Camera.toWorld(this.host.camera(), p.x, p.y);
    this.state = { kind: 'press', p, onObject: this.host.hitTest(wx, wy), at: 0 };
    if (p.type === 'touch' && this.host.contextMenu) {
      const id = (this.state as { onObject: string | null }).onObject;
      this.longPress = setTimeout(() => {
        if (this.state.kind === 'press' && this.state.p.id === p.id) {
          this.state = { kind: 'idle' };
          this.host.contextMenu?.(p.x, p.y, id);
        }
      }, LONG_PRESS_MS);
    }
  }

  pointerMove(p: PointerInput): void {
    const prev = this.pointers.get(p.id);
    if (!prev) return;
    this.pointers.set(p.id, { x: p.x, y: p.y });
    const s = this.state;
    switch (s.kind) {
      case 'pan': {
        if (s.id !== p.id) return;
        this.host.setCamera(Camera.panBy(this.host.camera(), p.x - s.x, p.y - s.y));
        this.state = { kind: 'pan', id: p.id, x: p.x, y: p.y };
        return;
      }
      case 'pinch': {
        if (p.id !== s.a && p.id !== s.b) return;
        const other = this.pointers.get(p.id === s.a ? s.b : s.a);
        if (!other) return;
        const before = Math.hypot(prev.x - other.x, prev.y - other.y);
        const after = Math.hypot(p.x - other.x, p.y - other.y);
        let c = this.host.camera();
        // pan by half the movement of one finger (the centre moves by half)
        c = Camera.panBy(c, (p.x - prev.x) / 2, (p.y - prev.y) / 2);
        if (before > 0 && after > 0)
          c = Camera.zoomAt(c, after / before, (p.x + other.x) / 2, (p.y + other.y) / 2);
        this.host.setCamera(c);
        return;
      }
      case 'press': {
        if (s.p.id !== p.id) return;
        if (Math.hypot(p.x - s.p.x, p.y - s.p.y) < DRAG_THRESHOLD[s.p.type]) return;
        this.cancelLongPress();
        // moving objects comes with the Design mode (handles); for now a drag on an
        // object behaves like a drag on empty space
        if (this.host.mode() === 'produce' || s.p.type === 'touch') {
          this.state = { kind: 'pan', id: p.id, x: s.p.x, y: s.p.y };
          this.pointerMove(p);
        } else {
          this.state = { kind: 'marquee', p: s.p, x: p.x, y: p.y };
          this.host.marquee({ x0: s.p.x, y0: s.p.y, x1: p.x, y1: p.y, crossing: p.x < s.p.x });
        }
        return;
      }
      case 'marquee': {
        if (s.p.id !== p.id) return;
        this.state = { ...s, x: p.x, y: p.y };
        this.host.marquee({ x0: s.p.x, y0: s.p.y, x1: p.x, y1: p.y, crossing: p.x < s.p.x });
        return;
      }
      case 'idle':
        return;
    }
  }

  pointerUp(p: PointerInput): void {
    this.pointers.delete(p.id);
    this.cancelLongPress();
    const s = this.state;
    switch (s.kind) {
      case 'press':
        if (s.p.id === p.id) {
          this.state = { kind: 'idle' };
          this.host.select(s.onObject, s.p.shift || s.p.ctrl);
        }
        return;
      case 'marquee': {
        if (s.p.id !== p.id) return;
        this.state = { kind: 'idle' };
        this.host.marquee(null);
        const cam = this.host.camera();
        const [ax, ay] = Camera.toWorld(cam, Math.min(s.p.x, s.x), Math.max(s.p.y, s.y));
        const [bx, by] = Camera.toWorld(cam, Math.max(s.p.x, s.x), Math.min(s.p.y, s.y));
        this.host.selectArea(
          { x: ax, y: ay, width: bx - ax, height: by - ay },
          s.x < s.p.x,
          s.p.shift || s.p.ctrl,
        );
        return;
      }
      case 'pan':
        if (s.id === p.id) this.state = { kind: 'idle' };
        return;
      case 'pinch':
        if (this.pointers.size < 2) this.state = { kind: 'idle' };
        return;
      case 'idle':
        return;
    }
  }

  /** The pointer was lost (capture lost, window blurred): drop the gesture. */
  pointerCancel(p: PointerInput): void {
    this.pointers.delete(p.id);
    this.cancelLongPress();
    if (this.state.kind === 'marquee') this.host.marquee(null);
    this.state = { kind: 'idle' };
  }

  wheel(w: WheelInput): void {
    const cam = this.host.camera();
    if (w.shift && !w.ctrl) {
      // shift+wheel pans horizontally
      this.host.setCamera(Camera.panBy(cam, -(w.dy || w.dx), 0));
      return;
    }
    if (w.ctrl || Math.abs(w.dx) <= Math.abs(w.dy) / 4) {
      // the wheel (and the trackpad pinch, which arrives as ctrl+wheel) zooms at the cursor
      if (w.dy === 0) return;
      this.host.setCamera(Camera.zoomAt(cam, WHEEL_ZOOM ** (-w.dy / 100), w.x, w.y));
      return;
    }
    // a two-finger scroll on a trackpad pans
    this.host.setCamera(Camera.panBy(cam, -w.dx, -w.dy));
  }

  private cancelLongPress(): void {
    if (this.longPress) clearTimeout(this.longPress);
    this.longPress = null;
  }
}
