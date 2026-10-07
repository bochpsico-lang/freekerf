# Planner: from a document to G-code

`crates/core/freekerf-planner` turns a [document](projects.md) into a job (G-code
lines). It writes G-code the way LaserGRBL writes it, so a job made from an imported
file is the same, line for line, as LaserGRBL's (`conformance` importer goldens).

## Order

1. Layers in **execution order**: fill and fill-and-line layers first, then line
   layers, then cut layers; user order inside each group; `ignore` layers skipped.
2. For each layer and each of its passes: a comment `(Layer #RRGGBB Mode pass i/n)`,
   the layer's feed and power, the **filling** (when the mode has one), then the
   **outlines** of its objects (when the mode has them), in drawing order.

## Fillings

The shapes of the layer are approximated with lines (arcs: ~0.2 mm chords for SVG
sources, 0.01 mm for DXF ones), open paths whose ends touch (within 0.05 mm) are
joined, and the hatch pattern of the layer (`freekerf-geometry::hatch`: lines,
grids, crosses, squares, zig-zag, Hilbert, inset) is generated with one line of
margin and clipped with the even-odd rule (nested shapes are holes). Lines are
ordered nearest-neighbour from the origin, in blocks of 2048.

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

## Limits

- Raster images are planned by `freekerf-raster` (line by line, dithering,
  vectorize, centerline); `plan` refuses documents with images until then.
- No travel optimization of outlines yet: they keep the drawing order (as LaserGRBL).

## Tests

- Unit tests in `src/{writer,vector,fill,generators,hershey}.rs`.
- `freekerf-gcode/tests/dotnet_format.rs`: number formatting against Mono.
- `conformance` importer goldens (`make test-rust`): `generator`, `hershey` cases.

Related: [test-patterns.md](test-patterns.md), [divergences](../divergences.md#importers-and-generators).
