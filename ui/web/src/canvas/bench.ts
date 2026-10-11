// The canvas performance check of PLAN M4a: 20,000 paths and an 8000×6000 image,
// panned "with the mouse" for a few seconds at several zoom levels while frame
// times are recorded. The pan goes through the same pointer handlers as a real
// mouse (synthetic PointerEvents on the canvas). Run it in the desktop app with
// `freekerf-desktop --bench-canvas`, or in a browser at `dev.html#bench-canvas`.

import { CanvasView } from './view';
import { syntheticImage, syntheticPaths } from './synthetic';

/** Numbers of one phase (one zoom level). */
export interface PhaseResult {
  name: string;
  frames: number;
  fps_mean: number;
  /** Frames per second over the slowest 1 % of frames. */
  fps_1pct_low: number;
  worst_frame_ms: number;
  cpu_ms_mean: number;
  /** Frames that redrew the whole scene (the pan left the cached area). */
  scene_redraws: number;
  segments_per_redraw: number;
  tiles_per_redraw: number;
}

/** The report (snake case: it is printed as JSON by the app). */
export interface BenchReport {
  paths: number;
  segments: number;
  image: string;
  viewport: string;
  device_pixel_ratio: number;
  gl_renderer: string;
  user_agent: string;
  build_ms: number;
  upload_ms: number;
  first_frame_ms: number;
  phases: PhaseResult[];
  /** Lowest mean of the phases (the number checked against 50 fps). */
  pan_fps_mean: number;
  worst_frame_ms: number;
  js_heap_mib: number | null;
}

/** Options (smaller numbers for tests). */
export interface BenchOptions {
  paths: number;
  imageWidth: number;
  imageHeight: number;
  /** Duration of each phase, ms. */
  phaseMs: number;
  /** Zoom levels relative to "fit the table". */
  zooms: number[];
}

export const DEFAULT_BENCH: BenchOptions = {
  paths: 20_000,
  imageWidth: 8000,
  imageHeight: 6000,
  phaseMs: 6000,
  zooms: [1, 4],
};

/**
 * Options from URL parameters, for diagnostics (`?paths=0&image=16x16&ms=3000&zooms=1,4`);
 * missing parameters keep the defaults.
 */
export function benchOptions(q: URLSearchParams): BenchOptions {
  const o = { ...DEFAULT_BENCH };
  const n = (k: string) => (q.get(k) !== null && Number.isFinite(Number(q.get(k))) ? Number(q.get(k)) : null);
  o.paths = n('paths') ?? o.paths;
  o.phaseMs = n('ms') ?? o.phaseMs;
  const img = /^(\d+)x(\d+)$/.exec(q.get('image') ?? '');
  if (img) {
    o.imageWidth = Number(img[1]);
    o.imageHeight = Number(img[2]);
  }
  const zooms = (q.get('zooms') ?? '')
    .split(',')
    .map(Number)
    .filter((z) => z > 0);
  if (zooms.length) o.zooms = zooms;
  return o;
}

const frame = () => new Promise<number>((r) => requestAnimationFrame(r));

function glRenderer(canvas: HTMLCanvasElement): string {
  const gl = canvas.getContext('webgl2');
  if (!gl) return 'none';
  const ext = gl.getExtension('WEBGL_debug_renderer_info');
  return String(ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER));
}

function pointer(el: HTMLElement, type: string, x: number, y: number): void {
  const r = el.getBoundingClientRect();
  el.dispatchEvent(
    new PointerEvent(type, {
      pointerId: 1,
      pointerType: 'mouse',
      button: type === 'pointermove' ? -1 : 1,
      buttons: type === 'pointerup' ? 0 : 4,
      clientX: r.left + x,
      clientY: r.top + y,
      bubbles: true,
    }),
  );
}

/** Runs the check inside `host` (which must have a size) and returns the report. */
export async function runCanvasBench(
  host: HTMLElement,
  o: BenchOptions = DEFAULT_BENCH,
): Promise<BenchReport> {
  const table = { x: 0, y: 0, width: 400, height: 400 };
  const t0 = performance.now();
  const paths = syntheticPaths(o.paths, table);
  const image = syntheticImage(o.imageWidth, o.imageHeight, { x: 20, y: 60, width: 360, height: 270 }, 9);
  const t1 = performance.now();
  const canvas = new CanvasView(host);
  canvas.setMode('produce');
  canvas.setWorkArea(table);
  canvas.setPaths(paths);
  canvas.setImages([image]);
  const t2 = performance.now();
  canvas.fit();
  canvas.draw();
  await frame();
  const t3 = performance.now();

  let cpu = 0;
  canvas.onFrame = (f) => {
    cpu += f.cpuMs;
  };
  const phases: PhaseResult[] = [];
  const { width, height } = canvas.camera;
  for (const zoom of o.zooms) {
    canvas.fit();
    canvas.zoom(zoom);
    canvas.draw();
    await frame();
    cpu = 0;
    const times: number[] = [];
    // middle-button drag back and forth over 120 % of the canvas width with some
    // vertical wobble: long enough to leave the cached area (scene redraws included)
    const cx = width / 2;
    const cy = height / 2;
    const amp = width * 0.6;
    pointer(canvas.overlay, 'pointerdown', cx, cy);
    let last = await frame();
    const start = last;
    let redraws = 0;
    let segments = 0;
    let tiles = 0;
    while (last - start < o.phaseMs) {
      const t = (last - start) / 1000;
      pointer(
        canvas.overlay,
        'pointermove',
        cx + amp * Math.sin(t * 1.5),
        cy + (height / 8) * Math.sin(t * 2.3),
      );
      const now = await frame();
      times.push(now - last);
      last = now;
      const st = canvas.stats;
      if (st.cacheRedrawn) {
        redraws++;
        segments = st.segments;
        tiles = st.tiles;
      }
    }
    pointer(canvas.overlay, 'pointerup', cx, cy);
    const sorted = [...times].sort((a, b) => b - a);
    const slow = sorted.slice(0, Math.max(1, Math.floor(sorted.length / 100)));
    const mean = times.reduce((s, t) => s + t, 0) / Math.max(1, times.length);
    phases.push({
      name: `pan at ${zoom}x`,
      frames: times.length,
      fps_mean: round(1000 / mean),
      fps_1pct_low: round(1000 / (slow.reduce((s, t) => s + t, 0) / slow.length)),
      worst_frame_ms: round(sorted[0] ?? 0),
      cpu_ms_mean: round(cpu / Math.max(1, times.length)),
      scene_redraws: redraws,
      segments_per_redraw: segments,
      tiles_per_redraw: tiles,
    });
  }
  const memory = (performance as unknown as { memory?: { usedJSHeapSize: number } }).memory;
  const report: BenchReport = {
    paths: o.paths,
    segments: paths.segmentCount,
    image: `${o.imageWidth}x${o.imageHeight}`,
    viewport: `${Math.round(width)}x${Math.round(height)}`,
    device_pixel_ratio: window.devicePixelRatio || 1,
    gl_renderer: glRenderer(canvas.gl),
    user_agent: navigator.userAgent,
    build_ms: round(t1 - t0),
    upload_ms: round(t2 - t1),
    first_frame_ms: round(t3 - t2),
    phases,
    pan_fps_mean: Math.min(...phases.map((p) => p.fps_mean)),
    worst_frame_ms: Math.max(...phases.map((p) => p.worst_frame_ms)),
    js_heap_mib: memory ? round(memory.usedJSHeapSize / 1048576) : null,
  };
  canvas.dispose();
  return report;
}

function round(v: number): number {
  return Math.round(v * 10) / 10;
}
