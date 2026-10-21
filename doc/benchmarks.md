# Benchmarks

```bash
cargo bench -p freekerf-gcode            # with rayon (default)
cargo bench -p freekerf-gcode --no-default-features
```

`crates/core/freekerf-gcode/benches/analysis.rs` generates a synthetic raster of
about 1,000,000 lines (rows of 500 `G1 X.. S..` moves) and measures parsing plus
analysis (time estimate, ranges).

| Date | Machine | Case | Time |
| --- | --- | --- | --- |
| 2026-10-07 | 4 vCPU Xeon @ 2.8 GHz (cloud container), Rust 1.97 | 1M lines, parallel parsing (rayon) | 0.67 s |
| 2026-10-07 | same | 1M lines, sequential | 0.93 s |
| 2026-10-07 | Intel Core i5-1235U (10 cores, 12 threads), Linux, Rust 1.97.1 | 1M lines, parallel parsing (rayon) | 0.36 s |
| 2026-10-07 | same | 1M lines, sequential | 0.50 s |

## LaserGRBL (C#)

`conformance`'s `scripts/large_file.py` writes the same raster, times LaserGRBL's
`GrblFile.LoadFile` (load and analysis, in process; LaserGRBL bf15096 built with Mono)
and `freekerf analyze --json`, and checks that every field of the analysis is equal
(`stream` included). Details: `conformance/doc/large-files.md`.

| Date | Machine | Case | LaserGRBL | FreeKerf |
| --- | --- | --- | --- | --- |
| 2026-10-07 | i5-1235U, Mono 6.8 / Rust 1.97.1 release | 100k lines: load + analysis / `analyze --json` | 0.41 s | 0.24 s |
| 2026-10-07 | same | 1M lines: load + analysis / `analyze --json` | 3.96 s | 2.59 s |
| 2026-10-07 | same | 1M lines: load + analysis / parse + analysis (rayon) | 3.96 s | 0.36 s |

`analyze --json` also simulates the whole job run for its `stream` field, which is most
of its time. Mono on Linux is not .NET Framework on Windows: take the C# column as an
order of magnitude. Both give identical results on 100,003 lines.

## Canvas (desktop app, M4a)

PLAN M4a acceptance: **20,000 paths + an 8000×6000 image, ≥ 50 fps while panning
with the mouse, on Linux** (WebKitGTK). The check runs inside the real app window:

```bash
cd ui/web && npm ci && npm run build && cd ../..
cargo build --release -p freekerf-desktop --features custom-protocol
target/release/freekerf-desktop --bench-canvas=canvas.json   # exit code 0 when ≥ 50 fps
# diagnostics: --bench-query="paths=0&image=16x16" (empty scene), "zooms=1,4,16", "ms=10000"
```

It builds 20,000 closed paths of 12–48 segments (598,722 segments) and an
8000×6000 ordered-dither image (12 tiles of ≤ 2048²), then drags with the middle
button through the canvas' own pointer handlers (synthetic `PointerEvent`s),
back and forth over 120 % of the view width, for 6 s at the "fit" zoom and at 4×.
The drag leaves the cached area, so whole-scene redraws are part of the numbers
(`scene_redraws`). Frame times are `requestAnimationFrame` intervals. Renderer:
[ADR 0014](adr/0014-canvas-renderer.md).

| Date | Machine | Case | Mean fps (1× / 4×) | 1 % low | Worst frame | CPU/frame | Peak RSS (app) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-10-08 | 4 vCPU Xeon @ 2.8 GHz cloud container, **no GPU**: Xvfb + Mesa llvmpipe (software), WebKitGTK 2.52, 1280×800 | full scene, long drags (5 scene redraws per phase) | 18.3 / 20.7 | 2.2 / 3.0 | 452 ms | 17 / 12 ms | 157 MiB |
| 2026-10-08 | same | full scene, drags inside the cached area (no redraw) | 26.4 / 24.6 | 19.6 / 17.9 | 56 ms | 1.2 ms | 156 MiB |
| 2026-10-08 | same | **empty scene** (`paths=0&image=16x16`): ceiling of the software compositor | 24.0 / 24.7 | — | — | — | — |
| 2026-10-20 | Intel Core i5-1235U, **Iris Xe (Alder Lake GT2)**, Ubuntu 26.04, GNOME 50 on Wayland, WebKitGTK 2.52.6, 60 Hz display, 1280×753 | full scene (acceptance), Wayland | **60 / 60** | 58.8 / 54.5 | 19 ms | 0.7 / 0.5 ms | 166 MiB |
| 2026-10-20 | same, `GDK_BACKEND=x11` (XWayland), 1280×800 | full scene (acceptance), X11 | **59.8 / 59.0** | 41.7 / 34.1 | 34 ms | 0.8 / 0.8 ms | 150 MiB |

Reading the software numbers: an empty scene pans at about the same 24–25 fps as the
full one inside the cache, so on llvmpipe the limit is WebKitGTK's software compositing of a 1280×800
canvas, not the renderer (≈1 ms of CPU per cached frame). A whole-scene redraw of
600k segments + 12 tiles costs ~300–450 ms in software and happens once every half
view of panning; on a GPU it is a single instanced draw per batch. The acceptance
number has to come from a machine with a GPU (CI runners have none).

On the Iris Xe laptop the pan holds the display's 60 Hz on both Wayland and X11, whole-scene
redraws included (5–8 per phase): the acceptance passes. The mean is capped by vsync,
so it does not show the headroom; the 1 % low and the worst frame do.
