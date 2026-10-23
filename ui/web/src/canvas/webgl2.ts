// WebGL2 renderer of the canvas (ADR 0014). It draws what is heavy: line segments
// (tool path, design paths) as one instanced draw per batch, and large raster
// images as tiled single-channel textures. Light, frequently changing things
// (grid, rulers, head, selection) are drawn by the Canvas2D overlay.
//
// Pan and zoom only change a uniform: the geometry is uploaded once.

import { Camera, niceStep, type CameraState, type Rect } from './camera';
import { SEGMENT_BYTES, type Toolpath } from './toolpath';

/** RGBA colour, components 0..1. */
export type Rgba = [number, number, number, number];

/** Colours used by the renderer (read from the `fk-` tokens, see theme.ts). */
export interface RenderColors {
  workarea: Rgba;
  gridMinor: Rgba;
  gridMajor: Rgba;
  travel: Rgba;
  burn: Rgba;
  done: Rgba;
  /** Palette of design layers (C00..C11, T1). */
  layers: Rgba[];
}

/** What to draw. */
export interface RenderOptions {
  colors: RenderColors;
  /** Work area of the machine (mm). */
  workArea: Rect;
  /** Commands executed so far (segments of earlier commands are drawn as done). */
  executed: number;
  /** Draw moves with the laser off. */
  showTravel: boolean;
}

/** A batch of design paths: segments in the tool path layout, `power` = layer index. */
export interface PathBatch {
  segments: DataView;
  segmentCount: number;
}

/** A single-channel raster image placed on the table (0 = burn, 255 = nothing). */
export interface RasterImage {
  width: number;
  height: number;
  pixels: Uint8Array;
  /** Where it lies on the table (mm). */
  rect: Rect;
  /** Layer index for the colour. */
  layer: number;
}

/** Statistics of the last frame. */
export interface FrameStats {
  drawCalls: number;
  segments: number;
  tiles: number;
  /** The scene was redrawn into the cache this frame (else the cache was reused). */
  cacheRedrawn: boolean;
}

/**
 * Extra area kept around the view in the scene cache, as a fraction of the view on
 * each side: panning within it only moves the cached picture.
 */
export const CACHE_MARGIN = 0.5;

const SEGMENT_VS = `#version 300 es
layout(location = 0) in vec4 a_seg;
layout(location = 1) in float a_power;
layout(location = 2) in uint a_cmd;
uniform mat3 u_clip;
uniform int u_mode;          // 0 tool path, 1 design paths, 2 flat colour (grid)
uniform vec4 u_flat;
uniform float u_maxPower;
uniform uint u_executed;
uniform bool u_showTravel;
uniform vec4 u_travel, u_burn, u_done;
uniform vec4 u_layers[16];
flat out vec4 v_color;
void main() {
  vec2 p = gl_VertexID == 0 ? a_seg.xy : a_seg.zw;
  gl_Position = vec4((u_clip * vec3(p, 1.0)).xy, 0.0, 1.0);
  if (u_mode == 2) {
    v_color = u_flat;
  } else if (u_mode == 1) {
    v_color = u_layers[clamp(int(a_power), 0, 15)];
  } else if (a_power < 0.0) {
    v_color = u_showTravel ? u_travel : vec4(0.0);
  } else {
    float k = u_maxPower > 0.0 ? clamp(a_power / u_maxPower, 0.15, 1.0) : 1.0;
    vec4 c = a_cmd < u_executed ? u_done : u_burn;
    v_color = vec4(c.rgb, c.a * k);
  }
}`;

const SEGMENT_FS = `#version 300 es
precision mediump float;
flat in vec4 v_color;
out vec4 o_color;
void main() {
  if (v_color.a <= 0.0) discard;
  o_color = vec4(v_color.rgb * v_color.a, v_color.a);
}`;

const QUAD_VS = `#version 300 es
layout(location = 0) in vec2 a_unit;
uniform mat3 u_clip;
uniform vec4 u_rect;   // x, y, width, height (mm)
out vec2 v_uv;
void main() {
  vec2 p = u_rect.xy + a_unit * u_rect.zw;
  v_uv = vec2(a_unit.x, 1.0 - a_unit.y);
  gl_Position = vec4((u_clip * vec3(p, 1.0)).xy, 0.0, 1.0);
}`;

const QUAD_FS = `#version 300 es
precision mediump float;
in vec2 v_uv;
uniform int u_textured;   // 0 flat colour, 1 single-channel image (0 = burn), 2 cached RGBA picture
uniform sampler2D u_tex;
uniform vec4 u_color;
out vec4 o_color;
void main() {
  if (u_textured == 2) {
    o_color = texture(u_tex, vec2(v_uv.x, 1.0 - v_uv.y));
    return;
  }
  float a = u_color.a;
  if (u_textured == 1) a *= 1.0 - texture(u_tex, v_uv).r;
  if (a <= 0.0) discard;
  o_color = vec4(u_color.rgb * a, a);
}`;

/** Largest tile edge (textures this size work on every WebGL2 implementation). */
export const TILE = 2048;

interface Bucket {
  /** First segment and count in the (reordered) buffer. */
  start: number;
  count: number;
  /** Bounds of its segments (mm). */
  rect: Rect;
}

interface Batch {
  vao: WebGLVertexArrayObject;
  buffer: WebGLBuffer;
  count: number;
  mode: 0 | 1;
  maxPower: number;
  buckets: Bucket[];
}

/** Cells per side of the spatial grid used to skip segments outside the view. */
export const GRID_CELLS = 16;

/**
 * Reorders segments by grid cell (of their midpoint) so each cell is a contiguous
 * range; returns the new bytes and the cells. Pure (tests).
 */
export function bucketSegments(
  src: DataView,
  count: number,
  cells = GRID_CELLS,
): { bytes: Uint8Array; buckets: Bucket[] } {
  const f = (i: number, k: number) => src.getFloat32(i * SEGMENT_BYTES + 4 * k, true);
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (let i = 0; i < count; i++) {
    x0 = Math.min(x0, f(i, 0), f(i, 2));
    y0 = Math.min(y0, f(i, 1), f(i, 3));
    x1 = Math.max(x1, f(i, 0), f(i, 2));
    y1 = Math.max(y1, f(i, 1), f(i, 3));
  }
  const cw = (x1 - x0) / cells || 1;
  const ch = (y1 - y0) / cells || 1;
  const cellOf = new Uint32Array(count);
  const sizes = new Uint32Array(cells * cells);
  for (let i = 0; i < count; i++) {
    const mx = (f(i, 0) + f(i, 2)) / 2;
    const my = (f(i, 1) + f(i, 3)) / 2;
    const cx = Math.min(cells - 1, Math.max(0, Math.floor((mx - x0) / cw)));
    const cy = Math.min(cells - 1, Math.max(0, Math.floor((my - y0) / ch)));
    const c = cy * cells + cx;
    cellOf[i] = c;
    sizes[c]!++;
  }
  const starts = new Uint32Array(cells * cells);
  for (let c = 1; c < starts.length; c++) starts[c] = starts[c - 1]! + sizes[c - 1]!;
  const fill = starts.slice();
  const bytes = new Uint8Array(count * SEGMENT_BYTES);
  const from = new Uint8Array(src.buffer, src.byteOffset, count * SEGMENT_BYTES);
  const bounds = Array.from({ length: cells * cells }, () => [Infinity, Infinity, -Infinity, -Infinity]);
  for (let i = 0; i < count; i++) {
    const c = cellOf[i]!;
    const at = fill[c]!++;
    bytes.set(from.subarray(i * SEGMENT_BYTES, (i + 1) * SEGMENT_BYTES), at * SEGMENT_BYTES);
    const b = bounds[c]!;
    b[0] = Math.min(b[0]!, f(i, 0), f(i, 2));
    b[1] = Math.min(b[1]!, f(i, 1), f(i, 3));
    b[2] = Math.max(b[2]!, f(i, 0), f(i, 2));
    b[3] = Math.max(b[3]!, f(i, 1), f(i, 3));
  }
  const buckets: Bucket[] = [];
  for (let c = 0; c < sizes.length; c++) {
    if (!sizes[c]) continue;
    const b = bounds[c]!;
    buckets.push({
      start: starts[c]!,
      count: sizes[c]!,
      rect: { x: b[0]!, y: b[1]!, width: b[2]! - b[0]!, height: b[3]! - b[1]! },
    });
  }
  return { bytes, buckets };
}

interface Tile {
  texture: WebGLTexture;
  rect: Rect;
  /** Its pixels in the image: x, y (rows top to bottom), width, height. */
  px: [number, number, number, number];
}

interface ImageEntry {
  /** The image's pixels: an image with the same array keeps its textures. */
  pixels: Uint8Array;
  tiles: Tile[];
  layer: number;
}

/** Where the pixels `px` of `img` lie on the table (image rows go down, Y goes up). */
export function tileRect(img: RasterImage, [x, y, w, h]: Tile['px']): Rect {
  const mmX = img.rect.width / img.width;
  const mmY = img.rect.height / img.height;
  return {
    x: img.rect.x + x * mmX,
    y: img.rect.y + img.rect.height - (y + h) * mmY,
    width: w * mmX,
    height: h * mmY,
  };
}

function compile(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const program = gl.createProgram();
  for (const [type, src] of [
    [gl.VERTEX_SHADER, vs],
    [gl.FRAGMENT_SHADER, fs],
  ] as const) {
    const s = gl.createShader(type);
    if (!s) throw new Error('WebGL2: cannot create a shader');
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
      throw new Error(`WebGL2 shader: ${gl.getShaderInfoLog(s) ?? '?'}`);
    }
    gl.attachShader(program, s);
  }
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(`WebGL2 program: ${gl.getProgramInfoLog(program) ?? '?'}`);
  }
  return program;
}

/** The WebGL2 renderer. Create it with {@link WebGL2Renderer.create}. */
export class WebGL2Renderer {
  private readonly segProgram: WebGLProgram;
  private readonly quadProgram: WebGLProgram;
  private readonly quadVao: WebGLVertexArrayObject;
  private readonly uniforms = new Map<string, WebGLUniformLocation | null>();
  private toolpath: Batch | null = null;
  private paths: Batch | null = null;
  private images: ImageEntry[] = [];
  private pixelRatio = 1;
  /** Statistics of the last frame. */
  stats: FrameStats = { drawCalls: 0, segments: 0, tiles: 0, cacheRedrawn: false };
  private cache: {
    fbo: WebGLFramebuffer;
    texture: WebGLTexture;
    msaa: WebGLFramebuffer | null;
    msaaBuffer: WebGLRenderbuffer | null;
    width: number;
    height: number;
    /** World rectangle the cache shows, and its scale (px per mm, device pixels). */
    rect: Rect;
    scale: number;
    key: string;
  } | null = null;
  private contentVersion = 0;
  private gridVao: WebGLVertexArrayObject | null = null;
  private gridBuffer: WebGLBuffer | null = null;

  private constructor(private readonly gl: WebGL2RenderingContext) {
    this.segProgram = compile(gl, SEGMENT_VS, SEGMENT_FS);
    this.quadProgram = compile(gl, QUAD_VS, QUAD_FS);
    const vao = gl.createVertexArray();
    const buf = gl.createBuffer();
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);
    this.quadVao = vao;
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  }

  /** A renderer on `canvas`, or `null` when WebGL2 is not available. */
  static create(canvas: HTMLCanvasElement): WebGL2Renderer | null {
    const gl = canvas.getContext('webgl2', { antialias: true, alpha: false, premultipliedAlpha: true });
    return gl ? new WebGL2Renderer(gl) : null;
  }

  private u(program: WebGLProgram, name: string): WebGLUniformLocation | null {
    const key = `${program === this.segProgram ? 's' : 'q'}:${name}`;
    if (!this.uniforms.has(key)) this.uniforms.set(key, this.gl.getUniformLocation(program, name));
    return this.uniforms.get(key) ?? null;
  }

  private upload(segments: DataView, count: number, mode: 0 | 1, maxPower: number, old: Batch | null): Batch {
    const gl = this.gl;
    if (old) {
      gl.deleteBuffer(old.buffer);
      gl.deleteVertexArray(old.vao);
    }
    const { bytes, buckets } = bucketSegments(segments, count);
    const vao = gl.createVertexArray();
    const buffer = gl.createBuffer();
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, bytes, gl.STATIC_DRAW);
    for (const loc of [0, 1, 2]) {
      gl.enableVertexAttribArray(loc);
      gl.vertexAttribDivisor(loc, 1);
    }
    this.pointAttributes(0);
    gl.bindVertexArray(null);
    return { vao, buffer, count, mode, maxPower, buckets };
  }

  /** Points the per-instance attributes at segment `first` of the bound buffer. */
  private pointAttributes(first: number): void {
    const gl = this.gl;
    const o = first * SEGMENT_BYTES;
    gl.vertexAttribPointer(0, 4, gl.FLOAT, false, SEGMENT_BYTES, o);
    gl.vertexAttribPointer(1, 1, gl.FLOAT, false, SEGMENT_BYTES, o + 16);
    gl.vertexAttribIPointer(2, 1, gl.UNSIGNED_INT, SEGMENT_BYTES, o + 20);
  }

  /** Replaces the tool path (G-code preview); `null` clears it. */
  setToolpath(t: Toolpath | null): void {
    this.contentVersion++;
    if (!t) {
      if (this.toolpath) this.gl.deleteBuffer(this.toolpath.buffer);
      this.toolpath = null;
      return;
    }
    this.toolpath = this.upload(t.segments, t.segmentCount, 0, t.maxPower, this.toolpath);
  }

  /** Replaces the design paths; `null` clears them. */
  setPaths(p: PathBatch | null): void {
    this.contentVersion++;
    if (!p) {
      if (this.paths) this.gl.deleteBuffer(this.paths.buffer);
      this.paths = null;
      return;
    }
    this.paths = this.upload(p.segments, p.segmentCount, 1, 0, this.paths);
  }

  /**
   * Replaces the raster images (split in tiles of at most {@link TILE} pixels). An
   * image whose pixel array was already shown keeps its textures (moved: only the
   * places change).
   */
  setImages(images: RasterImage[]): void {
    this.contentVersion++;
    const old = this.images;
    this.images = images.map((img) => {
      const i = old.findIndex((e) => e.pixels === img.pixels && e.tiles.length > 0);
      if (i < 0) return { pixels: img.pixels, tiles: this.tiles(img), layer: img.layer };
      const [kept] = old.splice(i, 1);
      const tiles = kept!.tiles.map((t) => ({ ...t, rect: tileRect(img, t.px) }));
      return { pixels: img.pixels, tiles, layer: img.layer };
    });
    for (const e of old) for (const t of e.tiles) this.gl.deleteTexture(t.texture);
  }

  private tiles(img: RasterImage): Tile[] {
    const gl = this.gl;
    const out: Tile[] = [];
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.pixelStorei(gl.UNPACK_ROW_LENGTH, img.width);
    for (let ty = 0; ty < img.height; ty += TILE) {
      for (let tx = 0; tx < img.width; tx += TILE) {
        const w = Math.min(TILE, img.width - tx);
        const h = Math.min(TILE, img.height - ty);
        const texture = gl.createTexture();
        gl.bindTexture(gl.TEXTURE_2D, texture);
        gl.pixelStorei(gl.UNPACK_SKIP_PIXELS, tx);
        gl.pixelStorei(gl.UNPACK_SKIP_ROWS, ty);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.R8, w, h, 0, gl.RED, gl.UNSIGNED_BYTE, img.pixels);
        gl.generateMipmap(gl.TEXTURE_2D);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
        const px: Tile['px'] = [tx, ty, w, h];
        out.push({ texture, px, rect: tileRect(img, px) });
      }
    }
    for (const p of [gl.UNPACK_ROW_LENGTH, gl.UNPACK_SKIP_PIXELS, gl.UNPACK_SKIP_ROWS]) gl.pixelStorei(p, 0);
    return out;
  }

  /** Sizes the drawing buffer for a view of `width`×`height` CSS pixels. */
  resize(width: number, height: number, pixelRatio: number): void {
    const c = this.gl.canvas as HTMLCanvasElement;
    this.pixelRatio = pixelRatio;
    const w = Math.max(1, Math.round(width * pixelRatio));
    const h = Math.max(1, Math.round(height * pixelRatio));
    if (c.width !== w || c.height !== h) {
      c.width = w;
      c.height = h;
    }
  }

  /**
   * Draws one frame. The scene (work area, images, paths, tool path) is drawn into
   * a cache texture bigger than the view; while the camera only pans inside it, a
   * frame is a single textured quad. Any other change redraws the cache.
   */
  render(camera: CameraState, o: RenderOptions, background: Rgba): void {
    const gl = this.gl;
    const key = cacheKey(o, background, this.contentVersion);
    const ratio = this.pixelRatio;
    const visible = Camera.visible(camera);
    const scale = camera.scale * ratio;
    const c = this.cache;
    const reusable =
      c !== null && c.key === key && Math.abs(c.scale - scale) < 1e-9 && contains(c.rect, visible);
    let stats: FrameStats = { drawCalls: 0, segments: 0, tiles: 0, cacheRedrawn: false };
    if (!reusable) {
      stats = this.redrawCache(camera, o, background, key);
    }
    // blit the cache: one quad in world coordinates
    const cache = this.cache;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, gl.drawingBufferWidth, gl.drawingBufferHeight);
    gl.clearColor(...background);
    gl.clear(gl.COLOR_BUFFER_BIT);
    if (cache) {
      gl.useProgram(this.quadProgram);
      gl.bindVertexArray(this.quadVao);
      gl.uniformMatrix3fv(this.u(this.quadProgram, 'u_clip'), false, Camera.clipMatrix(camera));
      gl.uniform1i(this.u(this.quadProgram, 'u_textured'), 2);
      gl.uniform1i(this.u(this.quadProgram, 'u_tex'), 0);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, cache.texture);
      const r = cache.rect;
      gl.uniform4f(this.u(this.quadProgram, 'u_rect'), r.x, r.y, r.width, r.height);
      gl.uniform4f(this.u(this.quadProgram, 'u_color'), 1, 1, 1, 1);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
      gl.bindVertexArray(null);
      stats.drawCalls++;
    }
    this.stats = stats;
  }

  /** Throws the cache away (e.g. after the context was restored). */
  invalidateCache(): void {
    this.contentVersion++;
  }

  private ensureCache(width: number, height: number): NonNullable<WebGL2Renderer['cache']> {
    const gl = this.gl;
    const c = this.cache;
    if (c && c.width === width && c.height === height) return c;
    if (c) this.freeCache();
    const texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    const fbo = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
    // multisampled target for smooth lines, resolved into the texture
    let msaa: WebGLFramebuffer | null = null;
    let msaaBuffer: WebGLRenderbuffer | null = null;
    const samples = Math.min(4, Number(gl.getParameter(gl.MAX_SAMPLES)) || 0);
    if (samples > 1) {
      msaaBuffer = gl.createRenderbuffer();
      gl.bindRenderbuffer(gl.RENDERBUFFER, msaaBuffer);
      gl.renderbufferStorageMultisample(gl.RENDERBUFFER, samples, gl.RGBA8, width, height);
      msaa = gl.createFramebuffer();
      gl.bindFramebuffer(gl.FRAMEBUFFER, msaa);
      gl.framebufferRenderbuffer(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.RENDERBUFFER, msaaBuffer);
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    this.cache = {
      fbo,
      texture,
      msaa,
      msaaBuffer,
      width,
      height,
      rect: { x: 0, y: 0, width: 0, height: 0 },
      scale: 0,
      key: '',
    };
    return this.cache;
  }

  private freeCache(): void {
    const gl = this.gl;
    const c = this.cache;
    if (!c) return;
    gl.deleteFramebuffer(c.fbo);
    gl.deleteTexture(c.texture);
    if (c.msaa) gl.deleteFramebuffer(c.msaa);
    if (c.msaaBuffer) gl.deleteRenderbuffer(c.msaaBuffer);
    this.cache = null;
  }

  private redrawCache(camera: CameraState, o: RenderOptions, background: Rgba, key: string): FrameStats {
    const gl = this.gl;
    const max = Number(gl.getParameter(gl.MAX_TEXTURE_SIZE)) || 4096;
    const ratio = this.pixelRatio;
    const k = 1 + 2 * CACHE_MARGIN;
    const width = Math.min(max, Math.ceil(camera.width * ratio * k));
    const height = Math.min(max, Math.ceil(camera.height * ratio * k));
    const cache = this.ensureCache(width, height);
    // a camera of the cache size (in CSS pixels) centred on the view
    const cacheCam: CameraState = { ...camera, width: width / ratio, height: height / ratio };
    gl.bindFramebuffer(gl.FRAMEBUFFER, cache.msaa ?? cache.fbo);
    gl.viewport(0, 0, width, height);
    const stats = this.drawScene(cacheCam, o, background);
    if (cache.msaa) {
      gl.bindFramebuffer(gl.READ_FRAMEBUFFER, cache.msaa);
      gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, cache.fbo);
      gl.blitFramebuffer(0, 0, width, height, 0, 0, width, height, gl.COLOR_BUFFER_BIT, gl.NEAREST);
      gl.bindFramebuffer(gl.READ_FRAMEBUFFER, null);
      gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, null);
    }
    cache.rect = Camera.visible(cacheCam);
    cache.scale = camera.scale * ratio;
    cache.key = key;
    return { ...stats, cacheRedrawn: true };
  }

  /** Grid of the work area (minor and major lines, 1-2-5 steps); returns draw calls. */
  private drawGrid(camera: CameraState, clip: Float32Array, o: RenderOptions): number {
    const gl = this.gl;
    const lines = gridLines(camera, o.workArea);
    if (!lines.minor.length && !lines.major.length) return 0;
    if (!this.gridVao) {
      this.gridVao = gl.createVertexArray();
      this.gridBuffer = gl.createBuffer();
      gl.bindVertexArray(this.gridVao);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.gridBuffer);
      for (const loc of [0, 1, 2]) {
        gl.enableVertexAttribArray(loc);
        gl.vertexAttribDivisor(loc, 1);
      }
      this.pointAttributes(0);
    }
    const all = new Float32Array([...lines.minor, ...lines.major]);
    gl.bindVertexArray(this.gridVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.gridBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, all, gl.DYNAMIC_DRAW);
    const p = this.segProgram;
    gl.useProgram(p);
    gl.uniformMatrix3fv(this.u(p, 'u_clip'), false, clip);
    gl.uniform1i(this.u(p, 'u_mode'), 2);
    let calls = 0;
    const per = SEGMENT_BYTES / 4;
    for (const [first, count, color] of [
      [0, lines.minor.length / per, o.colors.gridMinor],
      [lines.minor.length / per, lines.major.length / per, o.colors.gridMajor],
    ] as const) {
      if (!count) continue;
      gl.uniform4f(this.u(p, 'u_flat'), ...color);
      this.pointAttributes(first);
      gl.drawArraysInstanced(gl.LINES, 0, 2, count);
      calls++;
    }
    gl.bindVertexArray(null);
    return calls;
  }

  /** Draws the scene for `camera` into the bound framebuffer (viewport already set). */
  private drawScene(camera: CameraState, o: RenderOptions, background: Rgba): FrameStats {
    const gl = this.gl;
    const stats: FrameStats = { drawCalls: 0, segments: 0, tiles: 0, cacheRedrawn: false };
    gl.clearColor(...background);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const clip = Camera.clipMatrix(camera);
    const visible = Camera.visible(camera);

    // work area and images
    gl.useProgram(this.quadProgram);
    gl.bindVertexArray(this.quadVao);
    gl.uniformMatrix3fv(this.u(this.quadProgram, 'u_clip'), false, clip);
    const wa = o.workArea;
    gl.uniform1i(this.u(this.quadProgram, 'u_textured'), 0);
    gl.uniform4f(this.u(this.quadProgram, 'u_rect'), wa.x, wa.y, wa.width, wa.height);
    gl.uniform4f(this.u(this.quadProgram, 'u_color'), ...o.colors.workarea);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    stats.drawCalls++;
    stats.drawCalls += this.drawGrid(camera, clip, o);
    gl.useProgram(this.quadProgram);
    gl.bindVertexArray(this.quadVao);
    gl.uniform1i(this.u(this.quadProgram, 'u_textured'), 1);
    gl.uniform1i(this.u(this.quadProgram, 'u_tex'), 0);
    gl.activeTexture(gl.TEXTURE0);
    for (const img of this.images) {
      const color = o.colors.layers[img.layer] ?? o.colors.burn;
      gl.uniform4f(this.u(this.quadProgram, 'u_color'), ...color);
      for (const t of img.tiles) {
        if (!intersects(t.rect, visible)) continue;
        gl.bindTexture(gl.TEXTURE_2D, t.texture);
        gl.uniform4f(this.u(this.quadProgram, 'u_rect'), t.rect.x, t.rect.y, t.rect.width, t.rect.height);
        gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
        stats.drawCalls++;
        stats.tiles++;
      }
    }

    // segments
    gl.useProgram(this.segProgram);
    const p = this.segProgram;
    gl.uniformMatrix3fv(this.u(p, 'u_clip'), false, clip);
    gl.uniform1ui(this.u(p, 'u_executed'), Math.max(0, Math.floor(o.executed)));
    gl.uniform1i(this.u(p, 'u_showTravel'), o.showTravel ? 1 : 0);
    gl.uniform4f(this.u(p, 'u_travel'), ...o.colors.travel);
    gl.uniform4f(this.u(p, 'u_burn'), ...o.colors.burn);
    gl.uniform4f(this.u(p, 'u_done'), ...o.colors.done);
    const layers = new Float32Array(64);
    o.colors.layers.slice(0, 16).forEach((c, i) => layers.set(c, i * 4));
    gl.uniform4fv(this.u(p, 'u_layers'), layers);
    for (const b of [this.paths, this.toolpath]) {
      if (!b || b.count === 0) continue;
      gl.uniform1i(this.u(p, 'u_mode'), b.mode);
      gl.uniform1f(this.u(p, 'u_maxPower'), b.maxPower);
      gl.bindVertexArray(b.vao);
      gl.bindBuffer(gl.ARRAY_BUFFER, b.buffer);
      // consecutive visible cells are merged into one draw
      let runStart = -1;
      let runCount = 0;
      const flush = () => {
        if (runCount === 0) return;
        this.pointAttributes(runStart);
        gl.drawArraysInstanced(gl.LINES, 0, 2, runCount);
        stats.drawCalls++;
        stats.segments += runCount;
        runCount = 0;
      };
      for (const k of b.buckets) {
        if (!intersects(k.rect, visible)) {
          flush();
          continue;
        }
        if (runCount === 0) runStart = k.start;
        runCount += k.count;
      }
      flush();
    }
    gl.bindVertexArray(null);
    return stats;
  }

  /** Frees the GPU resources. */
  dispose(): void {
    this.freeCache();
    if (this.gridBuffer) this.gl.deleteBuffer(this.gridBuffer);
    if (this.gridVao) this.gl.deleteVertexArray(this.gridVao);
    this.setToolpath(null);
    this.setPaths(null);
    this.setImages([]);
  }

  /** Device pixel ratio in use. */
  get ratio(): number {
    return this.pixelRatio;
  }
}

/**
 * Grid lines over the visible part of the work area, in the segment layout (6
 * floats each; the last two unused): minor every ≥ 12 px, major every ≥ 80 px.
 */
export function gridLines(camera: CameraState, workArea: Rect): { minor: number[]; major: number[] } {
  const v = Camera.visible(camera);
  const x0 = Math.max(v.x, workArea.x);
  const x1 = Math.min(v.x + v.width, workArea.x + workArea.width);
  const y0 = Math.max(v.y, workArea.y);
  const y1 = Math.min(v.y + v.height, workArea.y + workArea.height);
  const out = { minor: [] as number[], major: [] as number[] };
  if (x1 <= x0 || y1 <= y0) return out;
  const minor = niceStep(camera.scale, 12);
  const major = niceStep(camera.scale, 80);
  const isMajor = (t: number) => Math.abs(t / major - Math.round(t / major)) < 1e-6;
  for (let x = Math.ceil(x0 / minor) * minor; x <= x1; x += minor) {
    (isMajor(x) ? out.major : out.minor).push(x, y0, x, y1, 0, 0);
  }
  for (let y = Math.ceil(y0 / minor) * minor; y <= y1; y += minor) {
    (isMajor(y) ? out.major : out.minor).push(x0, y, x1, y, 0, 0);
  }
  return out;
}

function contains(outer: Rect, inner: Rect): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.width <= outer.x + outer.width &&
    inner.y + inner.height <= outer.y + outer.height
  );
}

/** What invalidates the cached picture besides the camera. */
function cacheKey(o: RenderOptions, background: Rgba, version: number): string {
  const wa = o.workArea;
  return JSON.stringify([
    version,
    o.executed,
    o.showTravel,
    wa.x,
    wa.y,
    wa.width,
    wa.height,
    background,
    o.colors,
  ]);
}

function intersects(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}
