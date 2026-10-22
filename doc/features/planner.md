# Planner: from a document to G-code

`crates/core/freekerf-planner` turns a [document](projects.md) into a job (G-code
lines). It writes G-code the way LaserGRBL writes it, so a job made from an imported
file is the same, line for line, as LaserGRBL's (`conformance` importer goldens).

## Order

1. Layers in **execution order**: fill and fill-and-line layers first, then line
   layers, then cut layers; user order inside each group; `ignore` layers skipped.
2. For each layer and each of its passes: a comment `(Layer #RRGGBB Mode pass i/n)`,
   the layer's feed and power, the **filling** (when the mode has one), then the
   **outlines** of its objects (when the mode has them), in drawing order. On a
   filling layer without outlines (`Fill`), the strokes of texts in single-line
   fonts are still engraved, as lines.

## Fillings

The shapes of the layer are approximated with lines (arcs: ~0.2 mm chords for SVG
sources, 0.01 mm for DXF ones), open paths whose ends touch (within 0.05 mm) are
joined, and the hatch pattern of the layer (`freekerf-geometry::hatch`: lines,
grids, crosses, squares, zig-zag, Hilbert, inset) is generated with one line of
margin and clipped with the even-odd rule (nested shapes are holes). Lines are
ordered nearest-neighbour from the origin, in blocks of 2048.

The open strokes of texts (single-line fonts: a text keeps every stroke open,
also an O that comes back to its start) are left out of the filling and engraved
as lines; closing them would fill the inside of an S or an e. Imported open paths
are still closed and filled, as LaserGRBL does (it has no editable text). Closed
paths of a text (outline fonts, an outline around single-line strokes) are filled.

## G-code writer

The writer (`freekerf_planner::writer`) is LaserGRBL's `gcodeRelated`:

- the laser starts on at zero power (`M3 S0` with PWM, otherwise `M5 S<$30>`), pen up
  is `S0` and pen down `S<power>` (without PWM: the laser off/on commands, the power
  set once per layer);
- moves are compressed: `G`, `X`, `Y` and `F` are written only when they change, and
  `F` again after every pen down;
- numbers have at most three decimals, formatted like .NET formats `float`
  (`freekerf_gcode::dotnet`, checked against Mono on 4000 values);
- circular arcs are `G2`/`G3` with `I`/`J`; for DXF sources, arcs shorter than
  0.05 mm or closer than 0.001 mm to their chord are lines;
- Smoothieware gets the power on every move, scaled to 0–1 (`/255`);
- *Disable G0* writes travel moves as `G1`.

Differences between SVG and DXF jobs (comments on pen lines, the first pen up, pass
ends) follow LaserGRBL and are described in `vector.rs`.

## Options

`PlanOptions { machine: MachineOptions, vector_style, layer_preamble }`;
`MachineOptions` holds the firmware, PWM support, *disable G0*, the board's `$30`, the
laser on/off commands, the defaults of jobs without layers and unidirectional
engraving (LaserGRBL settings of the same meaning).

**Placing a job** (`origin::place`, design/ux.md flow 3): before planning, the
service can move the document so a point of its bounds (the 3×3 `Anchor`) lands on
a start point: the head's current work position or the user origin
(`job.from_document {start, anchor}`).

## Limits

- Raster images: after the vector objects, every image in document order, as the
  raster dialog generates it (`image::plan_image_object`, see
  [raster.md](raster.md#images-in-jobs)). Centerline images need a tracer:
  `plan_with(doc, options, Some(&tracer))`; `plan` refuses them.
- No travel optimization of outlines yet: they keep the drawing order (as LaserGRBL).

## Tests

- Unit tests in `src/{writer,vector,fill,generators,hershey}.rs`.
- `freekerf-gcode/tests/dotnet_format.rs`: number formatting against Mono.
- `conformance` importer goldens (`make test-rust`): `generator`, `hershey` cases.

Related: [test-patterns.md](test-patterns.md), [divergences](../divergences.md#importers-and-generators).
