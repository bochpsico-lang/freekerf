# 0008. Tolerance for conformance cases drawn by GDI+

- Status: accepted (M2)
- Date: 2026-10-07

## Context

Some LaserGRBL image steps go through GDI+ (`Graphics.DrawImage` with a
`ColorMatrix`, `ImageAttributes.SetThreshold`, an interpolation mode, antialiased
path fills). The `conformance` goldens of those steps were recorded on Linux with
Mono's **libgdiplus** (Cairo underneath) and are marked `platform_dependent`:
`img_grayscale_*`, `img_invert`, `img_threshold*`, `img_resize_*` (bitmaps),
`image_processor` (`ip_*`) and `potrace_raster_fill*` (G-code made from such
bitmaps). Windows GDI+ gives other roundings, and its resize filters are not
documented; libgdiplus even blends pixels in a nearest-neighbour resize. FreeKerf
cannot reproduce "GDI+" exactly because there is no single GDI+ to reproduce, and
should not reproduce libgdiplus' quirks bit for bit.

PLAN §10 M2 asks for a documented tolerance metric, applied by `conformance` for the
Rust host only (the C# host stays exact), with the cases that still fail recorded
as divergences.

## Decision

**Bitmaps** (`pixels` output of `image_op` cases): the two images have the same size,
and over every ARGB channel of every pixel the absolute difference is at most **8**
and the mean absolute difference at most **1.5** (0..255 scale). The maximum allows
rounding and filter differences on edges; the mean keeps a systematic shift (a
brightness or contrast error) from passing.

**G-code made from GDI+ bitmaps** (`image_processor`, `potrace_raster_fill*`):
compared through their *burn maps*. Both programs are run through the same
interpreter (absolute and relative moves, `G0`/`G1`/`G2`/`G3`, `M3`/`M4`/`M5`, `S`), and
every cutting move is drawn onto a grid of the job's raster resolution (one cell per
dot), each cell keeping the highest power that crossed it, normalised to 0..255. The
two maps must cover the same cells to within one cell on every side and agree with
the bitmap metric above. The header and footer lines (before the first and after the
last cutting move) are compared exactly. This metric lands in `conformance` with the
FreeKerf kinds that produce such G-code (A6/A7); its thresholds are calibrated
there and recorded in this ADR if they change.

Rules:

- The tolerance is implemented in `conformance` (`lasergrbl_harness/tolerance.py`)
  and applies only when `LASERGRBL_HOST=rust` and the case is `platform_dependent`.
- A case outside the tolerance is a divergence (`DIV-NNN`, `rust_divergence` in the
  case), never a reason to widen the tolerance for one case.
- Where libgdiplus clearly differs from the documented GDI+ behaviour, FreeKerf
  follows the documentation (DIV-108: nearest-neighbour resize does not blend).
- Everything that does not go through GDI+ (`GetPixel` loops, dithering,
  Line2Line, vectorization of a given bitmap) stays exact.

## Consequences

- `freekerf-raster` implements the GDI+ steps itself (`ops::color_matrix` with
  libgdiplus' integer truncation, `ops::threshold`, `ops::resize`), so results are
  identical on every platform and in wasm.
- libgdiplus never applies the interpolation mode of `DrawImage` (it sets it on a
  pattern it does not use): every scaled draw uses Cairo's default `GOOD` filter.
  `freekerf_raster::gdi::draw_scaled` ports that path (Cairo's box filter, pixman's
  16.16 separable convolution, libgdiplus' premultiplied alpha tables) and is
  FreeKerf's smooth resize: it gives LaserGRBL-on-Linux results bit for bit and a
  proper area-averaging downscale, the common case in raster jobs.
- Today: 9 of the 10 `platform_dependent` bitmap cases match exactly (resize
  included), and `img_resize_nearest` / `img_resize_keep_alpha` are DIV-108.
- A user comparing a raster job with LaserGRBL on Windows can see one-level grey
  differences on edges after a resize; the burn is the same to the eye.
