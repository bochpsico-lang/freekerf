# Projects (`.fkproj`) and the document model

A FreeKerf project is a **document**: objects on **colour layers**, each layer with
its cut settings. Importers (SVG, DXF, images) fill it, the session edits it through
actions, and the planner turns it into G-code. The model is
`crates/core/freekerf-document` (core layer: no I/O).

## Model

| Part | What |
| --- | --- |
| `Layer` | a colour (`#RRGGBB`), a name, a **mode** (`line`, `fill`, `fill_and_line`, `cut`, `ignore`), `speed_mm_min`, `power` (the `S` value), `passes`, the hatch `fill` (pattern and lines per mm), `visible` |
| `Object` | an id (never reused), the layer it is on, a name, its source (file format and name) and its kind |
| `shape` | vector paths in millimetres, Y up: lines, circular arcs (centre and direction, so they become `G2`/`G3`) and cubic Béziers (`freekerf-geometry`) |
| `image` | a raster image (the file as imported, or RGBA pixels), its position and size on the bed, the raster settings of LaserGRBL's dialog (tool, direction, quality, grey adjustments, dithering, vectorize and centerline options, speeds, powers, laser commands) and a list of non-destructive edits (crop, rotate, flip, invert, fill, outline) |
| `unsupported` | an element of an imported file that was not converted (an SVG `text` or `image`, a polygon without commas): kept so the user sees it |

**Execution order** (`Document::execution_order`): fill layers first, then line
layers, then cut layers (a cut can free the piece before it is engraved); user order
inside each group; `ignore` layers are skipped. This is LaserGRBL's colour-layer order.

## File format

- JSON, UTF-8, pretty-printed with a stable field order; binary data (images) is
  base64.
- `"format": "freekerf-project"` and `"version": 1`. Older versions are migrated
  when read; a newer version is refused with a clear error (written by a newer
  FreeKerf).
- Reading checks the invariants: unique layer and object ids, every object on an
  existing layer, at least one pass per layer.
- Numbers round-trip exactly (shortest representation that reads back to the same
  `f64`).
- JSON schema: [`doc/schema/fkproj.schema.json`](../schema/fkproj.schema.json),
  generated from the Rust model by `cargo xtask gen-docs` and checked in CI
  (`cargo xtask check-docs`).

## API

```rust
let mut doc = Document::new();
let layer = doc.layer_for_color(Color::BLACK, 1000, 500);   // speed, power
doc.add_object(layer, ObjectKind::Shape { paths })?;
let text = doc.to_fkproj();
let back = Document::from_fkproj(text.as_bytes())?;
```

Editing actions (select, move, change layer settings, undo/redo) are in
`freekerf-session` ([actions.md](actions.md)).

## Limits

- Objects carry no transform of their own yet: moving an object moves its
  geometry. A per-object transform (for scaling and rotating without loss) comes
  with the editor (M4/M5).
- Large images are stored as the original file; projects with many large images
  are large JSON files. A zipped container is an option for later
  ([roadmap](../roadmap.md)).

## Tests

- Unit tests in `crates/core/freekerf-document/src` (layers, objects, ids,
  execution order, version and invariant checks, schema).
- `tests/properties.rs`: any generated document survives `to_fkproj` →
  `from_fkproj` unchanged.
