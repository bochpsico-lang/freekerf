# Raster engraving

`freekerf-raster` turns bitmaps into engraving G-code the way LaserGRBL's raster
import does: the image operations of the raster dialog, the dithering modes and
**Line2Line** (one G-code line per run of equal power). With the same image and
settings the G-code is identical to LaserGRBL's (conformance goldens `l2l_*`,
`img_*`).

## Bitmaps

`image::Image` is a 32-bit ARGB bitmap (not premultiplied, rows from the top), built
from pixels, from RGBA bytes or decoded from PNG, JPEG, BMP or GIF bytes
(`Image::decode`). It flips, rotates by quarter turns and crops.

## Image operations (`ops`)

| Operation | LaserGRBL | Notes |
| --- | --- | --- |
| `grayscale(img, r, g, b, brightness, contrast, formula)` | `ImageTransform.GrayScale` | formulas `SimpleAverage`, `WeightAverage`, `OpticalCorrect`, `Custom`; a GDI+ colour matrix with libgdiplus' integer truncation |
| `color_matrix(img, cm)` | `Graphics.DrawImage` with a `ColorMatrix` | drawn onto a transparent bitmap; alpha goes through the matrix |
| `invert` | `ImageTransform.Negative` | |
| `threshold(img, t, apply)` | `ImageAttributes.SetThreshold` | cut-off `round(t·255)`; without `apply` only the gray matrix |
| `whitenize(img, threshold, demo)` | `ImageTransform.Whitenize` | pixels lighter than the threshold become white (red in demo mode) |
| `fill(img, at, color, tolerance)` / `outline(img, at)` | flood fill and outline tools | 4-connected |
| `resize(img, w, h, kill_alpha, filter)` | `ImageTransform.ResizeImage` | nearest neighbour, bilinear, bicubic |

## Dithering (`dither`)

Atkinson, Floyd–Steinberg, Burkes, Jarvis-Judice-Ninke, Sierra (2 and 3 rows,
Lite), Stucki (the Cyotek error-diffusion code LaserGRBL uses, MIT) and Random.
Every result is pure black and white; the alpha channel is kept.

## Line2Line (`l2l`)

`load_image_l2l(img, conf, options)` writes the job of `GrblFile.LoadImageL2L`:
the image is flipped so Y grows up, scanned at `res` dots/mm horizontally,
vertically or diagonally, each run of equal grey becomes one `G1` with
`S = min + (max − min)·darkness`, white runs become rapid moves (or `G1 S0` with
G0 disabled), optionally in one direction only, with a border speed and
`vector_filling` (fill resolution `fres`). Smoothie gets the power as `0.00`
fractions. `L2lConf` holds the raster settings, `L2lOptions` the machine options
that matter (`disable_g0`, `unidirectional`, `max_pwm`).

## Platform differences

The GDI+ steps (colour matrix, threshold, resize) are implemented in Rust, so the
results are the same on every platform and in wasm. The conformance goldens of
those steps were recorded with libgdiplus; they are compared with the tolerance of
[ADR 0008](../adr/0008-gdiplus-tolerance.md).

## Divergences

Error diffusion keeps LaserGRBL's quirks (no error on row 0 / column 0, swapped
green and blue errors, DIV-041); random dithering is seeded by the caller (DIV-042);
a nearest-neighbour resize does not blend (DIV-108). See
[divergences](../divergences.md).

## Tests

Unit tests in `crates/core/freekerf-raster/src/`; `tests/properties.rs`;
`freekerf-service/tests/import_cases.rs` (conformance `raster` and `image_op` cases
copied here); conformance goldens (`make test-rust`).
