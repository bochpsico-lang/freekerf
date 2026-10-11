# 0009. Bitmap tracing: a Rust port of CsPotrace

- Status: accepted (M2)
- Date: 2026-10-10

## Context

LaserGRBL's "vectorize" raster mode traces the image with **CsPotrace**
(`LaserGRBL/CsPotrace`): Wolfgang Nagl's C# translation of potrace.js, itself a
translation of Peter Selinger's Potrace 1.x (GPL-2.0-or-later). The outlines then
become G-code with Béziers approximated by biarcs (`BezierToBiarc`, single
precision), filled with hatch lines (`PotraceClipper`, ADR 0007) or engraved through
a GDI+ drawing. The `conformance` goldens `potrace_*` pin every vertex and every arc.

Options:

| Option | Notes |
| --- | --- |
| Bind the C potrace library | GPL-2.0-or-later (accepted by PLAN §4). Needs FFI (`unsafe` is forbidden in the workspace), a C build on every platform, no `wasm32` build of the core. Its bitmap thresholding, path decomposition details and curve output differ from CsPotrace's, so the goldens would not match without rewriting the glue anyway |
| Port CsPotrace to Rust | ~1,000 lines of plain arithmetic; pure Rust, `wasm32`; can be checked line by line against LaserGRBL |

## Decision

Port CsPotrace (`freekerf_raster::potrace::trace`), the biarc approximation
(`potrace::biarc`, `f32` where the C# code uses `float`, .NET `Complex` as LaserGRBL's
copy implements it) and the G-code export (`potrace::gcode`), and compose them as
`GrblFile.LoadImagePotrace` does (`freekerf_raster::vectorize`). The shapes handed
to the hatch filling go through a `GraphicsPath` emulation (`freekerf_geometry::gdipath`:
`AddLine`/`AddBezier` point compression, libgdiplus' `Flatten`).

The raster filling draws the traced shapes into a bitmap. LaserGRBL does that with
GDI+ (antialiased fill and a white inset outline); FreeKerf uses **tiny-skia**
(BSD-3-Clause, pure Rust, `wasm32`), with libgdiplus' half-pixel offset and alpha
tables, then the exact libgdiplus resize (`freekerf_raster::gdi`, ADR 0008).

## Consequences

- No C dependency, no `unsafe`, the core still builds for `wasm32`.
- All `potrace_*` goldens pass: the 10 outline and vector-fill cases exactly; the 2
  raster-fill cases (`platform_dependent`) within ADR 0008's burn-map tolerance
  (max 1/255).
- Potrace improvements upstream are not inherited; the port tracks LaserGRBL's
  CsPotrace, whose behaviour is the specification here (`TraceOptions` documents the
  defaults LaserGRBL uses for unchecked options, DIV-045).
- A loop LaserGRBL could enter on a degenerate curve (a biarc split into itself) is
  bounded (`biarc::MAX_CURVES`); it never happens on traced curves.
