# Vectorize and centerline

Two raster modes turn a black and white image into outlines instead of engraving it
dot by dot, as LaserGRBL's raster dialog does.

## Vectorize (Potrace)

`freekerf_raster::vectorize::load_image_potrace(img, options, conf, machine, support_pwm)`
writes the job of `GrblFile.LoadImagePotrace`:

1. the image is flipped (Y up) and traced with Potrace (`potrace::trace`, a port of
   LaserGRBL's CsPotrace, [ADR 0009](../adr/0009-potrace.md)): black is
   `R + G + B < 382.5`;
2. optional filling: **vector** (hatch lines clipped by the outlines, any LaserGRBL
   pattern, `Filling::Vector`) or **raster** (the shapes drawn, resized to the filling
   resolution and engraved by Line2Line, `Filling::Raster`);
3. the outlines: a rapid move to each path, the laser on (`S<max power>` with PWM,
   the laser-on command without), lines as `G1`, Béziers as biarcs (`G2`/`G3`,
   within 2 px, sampled every 5 px), the laser off; inner outlines first, or
   nearest-neighbour order with *optimize fast*.

`TraceOptions`: spot removal (`turdsize`, 2 when off), smoothing (`alphamax`, 0
when off: corners only), curve optimization and its tolerance (0.2 when off). These
fall-backs are LaserGRBL's (DIV-045).

## Centerline (autotrace)

`freekerf_service::autotrace::centerline(img, options, tracer, offset, smart_bezier, machine)`
does `GrblFile.LoadImageCenterline`: the image goes as a PNG to the external
`autotrace` program with `-centerline` and the thresholds (`CenterlineOptions`:
corner 110°, line 1.0 by default; each can be off), and the SVG it prints is
converted like any SVG, scaled so its larger side is a tenth of the image's larger
side in pixels (the 10 px/mm the raster dialog uses for this tool), moved by the
offset. `ExternalAutotrace` runs the program (`autotrace` on the `PATH` by default);
tests use a double ([ADR 0010](../adr/0010-autotrace.md)).

## Limits

- Centerline needs `autotrace` installed; a missing or failing program is an error
  with its cause (DIV-048).
- The raster filling's drawing is tiny-skia's, not GDI+'s: pixels differ by at most
  one level from LaserGRBL on Linux (ADR 0008).

## Divergences

DIV-045 (unchecked options keep defaults, kept), DIV-046 (raster filling header
follows the raster PWM option while the outlines follow the machine setting, kept),
DIV-047 (no side effect on the caller's settings, not applicable), DIV-048 (tracer
errors reported), DIV-049 (centerline settings are explicit parameters, not
applicable). See [divergences](../divergences.md).

## Tests

Unit tests in `crates/core/freekerf-raster/src/potrace/`, `vectorize.rs`,
`render.rs` (the drawing against libgdiplus' pixels), `centerline.rs`;
`freekerf-raster/tests/properties.rs` (tracing and vectorizing never panic, paths
closed); `freekerf-service/src/autotrace.rs` (a tracer double, the external program
with a stand-in); conformance `potrace_*`, `centerline_*`, `svgtext_*` (`make test-rust`).
