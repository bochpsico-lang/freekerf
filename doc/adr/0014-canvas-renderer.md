# 0014. Canvas renderer: WebGL2, own code, scene cache

- Status: accepted (M4a), measured on real hardware: 60 fps with a GPU (`doc/benchmarks.md`)
- Date: 2026-10-08
- Context: ADR 0006 (Tauri 2, canvas risk on WebKitGTK), ADR 0013 (Svelte)

## Context

The canvas shows the machine table with tens of thousands of paths (converted text,
imported SVG/DXF), large raster images (20–50 MP), the G-code preview with the
burned part, the head at 60 Hz, a grid, rulers and selection handles. M4a requires
20,000 paths + an 8000×6000 image at ≥ 50 fps while panning with the mouse on
Linux (WebKitGTK), and a path to WebGPU.

Options: Canvas2D; WebGL2 with a library (PixiJS, regl, three.js); WebGL2 with our
own renderer; WebGPU now.

## Decision

**Our own WebGL2 renderer** (`ui/web/src/canvas/`, framework-free TypeScript) plus a
**Canvas2D overlay**:

- **Segments** (tool path, design paths) live in GPU buffers in the binary layout
  the Rust side produces (`freekerf_service::preview`, 24 bytes per segment), drawn
  with one `drawArraysInstanced(LINES)` per run of visible grid cells (16×16 spatial
  buckets). Pan and zoom only change a uniform; progress ("done" vs "to burn") is a
  uniform compared with each segment's command index.
- **Raster images** are single-channel (`R8`) textures in tiles of ≤ 2048² with
  mipmaps, tinted with the layer colour in the shader: an 8000×6000 image is 48 MB,
  not 192 MB of RGBA.
- **Scene cache**: the scene is drawn into a multisampled offscreen target 2× the
  view (half a view of margin on each side) and resolved into a texture. While the
  camera only pans inside it, a frame is one textured quad (~1 ms of CPU); zoom,
  new content, progress or theme changes redraw it.
- **Overlay (Canvas2D)**: grid on the work area, rulers, work area outline, job
  frame, head, selection rectangle. A few hundred lines per frame.
- **Gestures** are a pure state machine (`interaction.ts`, tested without a DOM).
- Colours come from the `fk-` tokens at runtime (`theme.ts`), so the canvas follows
  the light, dark and high-contrast themes.
- **WebGPU path**: the renderer owns only "upload segment batches / image tiles,
  draw with a camera"; the data layout (instanced segments, R8 tiles) maps 1:1 to
  WebGPU vertex buffers and textures. A `WebGPURenderer` with the same methods can
  replace it when WebKitGTK ships WebGPU, without touching the view, the overlay or
  the gestures.

## Why not the alternatives

- **Canvas2D**: 600k segments per frame is far beyond what Canvas2D strokes in
  16 ms, and images of 48 MP stall `drawImage` on WebKitGTK.
- **PixiJS / three.js**: large runtimes (100–600 kB) built for sprites or 3D; paths
  would still need custom geometry, and the instanced-segment layout shared with the
  Rust side would be wrapped, not used directly.
- **WebGPU now**: not available in WebKitGTK 2.52 by default; WebView2 and WKWebView
  support differs.

## Measurements

`freekerf-desktop --bench-canvas` (details and numbers in `doc/benchmarks.md`). On a
4-vCPU cloud container **without a GPU** (Mesa llvmpipe under Xvfb) the full scene
pans at 18–21 fps with long drags and 25 fps inside the cache, while an empty scene
pans at 24–25 fps: the software compositor is the limit there, not the renderer.
The acceptance number (≥ 50 fps) must be measured on a Linux machine with a GPU;
until then this ADR's performance claim is unverified.

## Consequences

- Bulk geometry never crosses the IPC as JSON; the Rust side encodes once, the GPU
  reads the same bytes (contract test: `ui/web/tests/fixtures/toolpath-v1.bin`).
- WebGL lines are 1 device pixel wide. Thick strokes (selection emphasis, kerf
  preview) will need quads in the vertex shader; listed in `doc/roadmap.md`.
- A context loss drops the GPU resources; the view must upload them again (handled
  when the shell lands: PR 4).
