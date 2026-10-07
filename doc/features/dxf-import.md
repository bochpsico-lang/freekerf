# DXF import

`freekerf_import::dxf::import(bytes, options)` reads an ASCII DXF drawing as lines
and **true arcs** in millimetres, one layer per DXF colour, as LaserGRBL does
(`DxfReader`). Arcs become `G2`/`G3`; the job is identical to LaserGRBL's
(conformance goldens `dxf_*`).

## What is read

| Entity | Result |
| --- | --- |
| `LINE` | a line |
| `LWPOLYLINE`, `POLYLINE` (2D and 3D) | lines, and arcs for bulges; closed flag |
| `CIRCLE`, `ARC` | arcs (a circle is two half arcs) |
| `ELLIPSE` | lines within 0.02 mm |
| `SPLINE` | sampled every ~0.1 mm (rational B-splines included), then fitted with lines and arcs within the spline tolerance (default 0.01 mm; 0 keeps short lines); fit points when there are no control points |
| `INSERT`, `MINSERT` | the block's entities, translated, rotated, scaled, mirrored, nested up to 32 levels; entities on layer `0` take the insert's layer, `ByBlock` its colour |

- **Units:** `$INSUNITS` (inches, feet, cm, m, µm…), mm when missing.
- **Colours:** true colour (420), ACI index (62), `ByLayer`, `ByBlock`; ACI 7 (white
  on black) is black for the laser.
- **Hidden:** entities on layers that are off or frozen, invisible entities and paper
  space are skipped.
- The drawing is moved so its bounds start at the origin.
- Arcs shorter than 0.05 mm or closer than 0.001 mm to their chord are written as
  lines.

## Errors

A binary DXF, malformed group codes or numbers, or a drawing without entities give an
error; the current document is not touched.

## Limits

Text, dimensions, hatches, 3D solids and meshes are ignored.

## Tests

Unit tests in `crates/core/freekerf-import/src/dxf/`; `tests/properties.rs` (never
panics); `freekerf-service/tests/import_cases.rs`; conformance `dxf_*` goldens (all
identical).
