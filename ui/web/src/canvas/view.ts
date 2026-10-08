// The canvas as a whole: a WebGL2 layer (heavy geometry), a Canvas2D overlay
// (grid, rulers, head, selection), the camera and the gestures. Framework-free:
// `Canvas.svelte` and the benchmark both drive it. Frames are drawn on demand,
// at most once per animation frame.

import { Camera, type CameraState, type Rect } from './camera';
import { CanvasInteraction, type InteractionHost, type PointerInput } from './interaction';
import { drawOverlay, type OverlayScene } from './overlay';
import { readTheme, type CanvasTheme } from './theme';
import type { Toolpath } from './toolpath';
import { WebGL2Renderer, type PathBatch, type RasterImage } from './webgl2';

/** What the owner gets back from the canvas. */
export interface CanvasViewEvents {
  select?(id: string | null, additive: boolean): void;
  selectArea?(rect: Rect, crossing: boolean, additive: boolean): void;
  contextMenu?(x: number, y: number, id: string | null): void;
  /** The camera changed (pan, zoom, resize). */
  camera?(c: CameraState): void;
  hitTest?(wx: number, wy: number): string | null;
}

/** Error when WebGL2 is not available. */
export class NoWebGL2Error extends Error {
  override name = 'NoWebGL2Error';
}

/** Timing of the frames drawn (benchmark and diagnostics). */
export interface FrameTiming {
  /** CPU time spent issuing the frame, ms. */
  cpuMs: number;
}

export class CanvasView {
  readonly gl: HTMLCanvasElement;
  readonly overlay: HTMLCanvasElement;
  private readonly ctx: CanvasRenderingContext2D;
  private readonly renderer: WebGL2Renderer;
  private readonly interaction: CanvasInteraction;
  private theme: CanvasTheme;
  private cam: CameraState;
  private frame = 0;
  private resizeObserver: ResizeObserver | null = null;
  private readonly detach: (() => void)[] = [];
  private mode: 'design' | 'produce' = 'produce';
  private executed = 0;
  private showTravel = true;
  /** The user moved the view: a resize keeps the camera instead of fitting again. */
  private touched = false;
  /** Overlay contents. */
  readonly scene: OverlayScene = {
    workArea: { x: 0, y: 0, width: 400, height: 400 },
    head: null,
    frame: null,
    marquee: null,
    units: 'mm',
  };
  /** Called after each drawn frame. */
  onFrame: ((t: FrameTiming) => void) | null = null;

  constructor(
    private readonly host: HTMLElement,
    private readonly events: CanvasViewEvents = {},
  ) {
    this.gl = document.createElement('canvas');
    this.overlay = document.createElement('canvas');
    for (const c of [this.gl, this.overlay]) {
      Object.assign(c.style, { position: 'absolute', inset: '0', width: '100%', height: '100%' });
      host.appendChild(c);
    }
    this.overlay.style.touchAction = 'none';
    this.overlay.tabIndex = 0;
    const renderer = WebGL2Renderer.create(this.gl);
    const ctx = this.overlay.getContext('2d');
    if (!renderer || !ctx) {
      this.dispose();
      throw new NoWebGL2Error('WebGL2 is not available in this web view');
    }
    this.renderer = renderer;
    this.ctx = ctx;
    this.theme = readTheme(host);
    const r = host.getBoundingClientRect();
    this.cam = Camera.fitted(this.scene.workArea, Math.max(1, r.width), Math.max(1, r.height));
    const ih: InteractionHost = {
      mode: () => this.mode,
      camera: () => this.cam,
      setCamera: (c) => this.setCamera(c),
      hitTest: (x, y) => this.events.hitTest?.(x, y) ?? null,
      select: (id, add) => this.events.select?.(id, add),
      selectArea: (rect, crossing, add) => this.events.selectArea?.(rect, crossing, add),
      marquee: (m) => {
        this.scene.marquee = m;
        this.invalidate();
      },
      contextMenu: (x, y, id) => this.events.contextMenu?.(x, y, id),
    };
    this.interaction = new CanvasInteraction(ih);
    this.bind();
    this.resize();
  }

  private bind(): void {
    const el = this.overlay;
    const input = (e: PointerEvent): PointerInput => {
      const r = el.getBoundingClientRect();
      return {
        id: e.pointerId,
        x: e.clientX - r.left,
        y: e.clientY - r.top,
        button: e.button,
        type: e.pointerType === 'touch' || e.pointerType === 'pen' ? e.pointerType : 'mouse',
        shift: e.shiftKey,
        ctrl: e.ctrlKey || e.metaKey,
        alt: e.altKey,
      };
    };
    const on = <K extends keyof HTMLElementEventMap>(
      target: HTMLElement | Window,
      type: K,
      f: (e: HTMLElementEventMap[K]) => void,
      opts?: AddEventListenerOptions,
    ) => {
      target.addEventListener(type, f as EventListener, opts);
      this.detach.push(() => target.removeEventListener(type, f as EventListener, opts));
    };
    on(el, 'pointerdown', (e) => {
      el.setPointerCapture?.(e.pointerId);
      if (e.button === 1) e.preventDefault();
      this.interaction.pointerDown(input(e));
    });
    on(el, 'pointermove', (e) => this.interaction.pointerMove(input(e)));
    on(el, 'pointerup', (e) => this.interaction.pointerUp(input(e)));
    on(el, 'pointercancel', (e) => this.interaction.pointerCancel(input(e)));
    on(el, 'lostpointercapture', (e) => this.interaction.pointerCancel(input(e)));
    on(el, 'contextmenu', (e) => e.preventDefault());
    on(
      el,
      'wheel',
      (e) => {
        e.preventDefault();
        const r = el.getBoundingClientRect();
        const k = e.deltaMode === 1 ? 33 : e.deltaMode === 2 ? r.height : 1;
        this.interaction.wheel({
          x: e.clientX - r.left,
          y: e.clientY - r.top,
          dx: e.deltaX * k,
          dy: e.deltaY * k,
          shift: e.shiftKey,
          ctrl: e.ctrlKey || e.metaKey,
        });
      },
      { passive: false },
    );
    on(el, 'keydown', (e) => {
      if (e.code === 'Space') this.interaction.setSpace(true);
    });
    on(el, 'keyup', (e) => {
      if (e.code === 'Space') this.interaction.setSpace(false);
    });
    on(window, 'blur', () => this.interaction.setSpace(false));
    if (typeof ResizeObserver !== 'undefined') {
      this.resizeObserver = new ResizeObserver(() => this.resize());
      this.resizeObserver.observe(this.host);
    }
  }

  /** Reads the size of the host again (also called by the resize observer). */
  resize(): void {
    const r = this.host.getBoundingClientRect();
    const w = Math.max(1, r.width);
    const h = Math.max(1, r.height);
    const ratio = window.devicePixelRatio || 1;
    this.renderer.resize(w, h, ratio);
    this.overlay.width = Math.round(w * ratio);
    this.overlay.height = Math.round(h * ratio);
    this.cam = Camera.resized(this.cam, w, h);
    if (!this.touched) this.cam = Camera.fitted(this.scene.workArea, w, h);
    this.events.camera?.(this.cam);
    this.invalidate();
  }

  /** Reads the theme tokens again (theme or contrast changed). */
  refreshTheme(): void {
    this.theme = readTheme(this.host);
    this.invalidate();
  }

  /** The camera. */
  get camera(): CameraState {
    return this.cam;
  }

  /** Moves the view (the user or the view state moved it: resizes keep it). */
  setCamera(c: CameraState): void {
    this.touched = true;
    this.applyCamera(c);
  }

  private applyCamera(c: CameraState): void {
    this.cam = c;
    this.events.camera?.(c);
    this.invalidate();
  }

  /** Fits `rect` (default: the work area) in the view. */
  fit(rect: Rect = this.scene.workArea): void {
    this.touched = false;
    this.applyCamera(Camera.fitted(rect, this.cam.width, this.cam.height));
  }

  /** Zooms around the centre (buttons, + / − keys). */
  zoom(factor: number): void {
    this.setCamera(Camera.zoomAt(this.cam, factor, this.cam.width / 2, this.cam.height / 2));
  }

  setMode(mode: 'design' | 'produce'): void {
    this.mode = mode;
  }

  setWorkArea(rect: Rect): void {
    this.scene.workArea = rect;
    this.invalidate();
  }

  setToolpath(t: Toolpath | null): void {
    this.renderer.setToolpath(t);
    this.scene.frame = t && t.segmentCount ? t.bounds : null;
    this.invalidate();
  }

  setPaths(p: PathBatch | null): void {
    this.renderer.setPaths(p);
    this.invalidate();
  }

  setImages(images: RasterImage[]): void {
    this.renderer.setImages(images);
    this.invalidate();
  }

  setHead(head: OverlayScene['head']): void {
    this.scene.head = head;
    this.invalidate();
  }

  setProgress(executed: number): void {
    if (executed === this.executed) return;
    this.executed = executed;
    this.invalidate();
  }

  setShowTravel(on: boolean): void {
    this.showTravel = on;
    this.invalidate();
  }

  setUnits(units: 'mm' | 'in'): void {
    this.scene.units = units;
    this.invalidate();
  }

  /** Statistics of the last frame. */
  get stats() {
    return this.renderer.stats;
  }

  /** Asks for a frame (coalesced to one per animation frame). */
  invalidate(): void {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.draw();
    });
  }

  /** Draws now. */
  draw(): void {
    const t0 = performance.now();
    this.renderer.render(
      this.cam,
      {
        colors: this.theme.render,
        workArea: this.scene.workArea,
        executed: this.executed,
        showTravel: this.showTravel,
      },
      this.theme.background,
    );
    drawOverlay(this.ctx, this.cam, this.scene, this.theme.overlay, this.theme.monoFont, this.renderer.ratio);
    this.onFrame?.({ cpuMs: performance.now() - t0 });
  }

  dispose(): void {
    if (this.frame) cancelAnimationFrame(this.frame);
    this.resizeObserver?.disconnect();
    for (const d of this.detach) d();
    this.renderer?.dispose();
    this.gl.remove();
    this.overlay.remove();
  }
}
