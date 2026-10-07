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
the PNG with `Image::encode_png`); `freekerf_service::autotrace` defines a `Tracer`
trait and `ExternalAutotrace`, which writes the PNG to a temporary file, runs
`autotrace` (on the `PATH`, or a configured path) without a shell through
`freekerf_platform::run_capture`, deletes the file and returns the SVG; the SVG is
converted with the SVG importer and planner (`autotrace::replay_svg`, shared with the
`svg_text` conformance kind). Tests and headless runs use a `Tracer` double.

A failing or silent tracer is reported as an error with its cause (LaserGRBL logs it
and then fails on the empty output as "Root element is missing", DIV-048).

## Consequences

- The `centerline` and `svg_text` goldens pass exactly with a double that prints the
  recorded autotrace output; what autotrace itself returns is outside FreeKerf.
- autotrace must be installed (Linux packages and Homebrew have it) or shipped. How
  the desktop app ships it per platform (bundled binary, as LaserGRBL does on
  Windows, or a detected system install) is decided with the installers (M4,
  `doc/roadmap.md`).
- Headless and wasm builds have centerline only when a tracer is provided.
