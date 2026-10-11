# 0010. Centerline tracing with the external autotrace program

- Status: accepted (M2)
- Date: 2026-10-10

## Context

LaserGRBL's "centerline" raster mode saves the bitmap as a PNG, runs the bundled
Windows `autotrace.exe` (`-output-fo svg -background-color FFFFFF -centerline
[-corner-t N] [-line-t X.X] file.png`) and converts the SVG it prints with its SVG
converter, scaled to a tenth of the image size in pixels (`GrblFile.LoadImageCenterline`).
autotrace (GPL-2.0-or-later, C, depends on GLib and image libraries) has no Rust
port; its centerline algorithm (thinning + curve fitting) is substantial.

Options:

1. **Bind the autotrace library** (`libautotrace`): FFI and `unsafe` (forbidden),
   GLib and its build on every platform, no `wasm32`.
2. **Run the external program**, as LaserGRBL does.
3. **Postpone** the feature.
4. Write a centerline tracer in Rust: a project of its own; results would differ from
   LaserGRBL's.

## Decision

Option 2. The core builds the request (`freekerf_raster::centerline::autotrace_args`,
the PNG with `Image::encode_png`) and defines the `Tracer` trait
(`freekerf_raster::centerline`); `freekerf_service::autotrace::ExternalAutotrace`
implements it: it writes the PNG to a temporary file, runs
`autotrace` (on the `PATH`, or a configured path) without a shell through
`freekerf_platform::run_capture`, deletes the file and returns the SVG; the SVG is
converted with the SVG importer and planner (`freekerf_planner::image::replay_svg`,
shared with the `svg_text` conformance kind). Tests and headless runs use a `Tracer` double.

A failing or silent tracer is reported as an error with its cause (LaserGRBL logs it
and then fails on the empty output as "Root element is missing", DIV-048).

## Consequences

- The `centerline` and `svg_text` goldens pass exactly with a double that prints the
  recorded autotrace output; what autotrace itself returns is outside FreeKerf.
- The desktop installers bundle an `autotrace` binary on every platform (decided
  2026-10-10; built in M4, `doc/roadmap.md`); a configured path or the `PATH` still
  overrides it. Licensing: the program is GPL-2.0-or-later, which is compatible with
  FreeKerf's GPL-3.0-or-later, and it runs as a separate program, so bundling is
  aggregation. Each release ships autotrace's license and the corresponding source
  of the exact binary and of the libraries linked into it (GLib, LGPL-2.1-or-later;
  libpng): the release CI builds it from a pinned upstream commit, without the
  optional ImageMagick and pstoedit, and publishes that source with the release.
- Headless and wasm builds have centerline only when a tracer is provided.
