# UI contract for the Design mode (M4a → M4b)

The Design mode screens of the desktop app (layers, inspector, palette, tools,
handles, import, welcome and migration) are built in M4a **against this contract
and a fake implementation** (`ui/web/src/dev/fake-document.ts`), behind
`flags.designMode` (off in distributed builds). In M4b the M2 crates implement it in
Rust; the UI then only swaps the implementation. This page is the request from the
UI to M2: types, actions, events and raw payloads.

The TypeScript side of the proposal is `ui/web/src/contract/document.ts` (types, the
action map and the `DocumentService` interface). Those types are hand-written only
because the Rust ones do not exist yet: in M4b they are deleted and replaced by the
generated `ui/web/src/gen/api.ts` (ADR 0015), with the same names.

It follows the model of `freekerf-document` (freekerf/freekerf#19): `Layer`
(`id`, `color`, `name`, `mode`, `speed_mm_min`, `power`, `passes`, `fill`,
`visible`) and `Object` (`id`, `layer`, `name`, `source`, `kind`). Where the UI needs
more, it is marked **new**.

## Principles

- App state stays in Rust; the UI reads summaries and **changes the document only
  through actions** (PLAN §9). Every action below is a registry action with a JSON
  schema and a tier, so scripts and agents can do the same.
- **Geometry never crosses as JSON** (ADR 0006): the UI gets summaries (ids, bounds,
  layer) as JSON and the drawable geometry as raw bytes.
- One event after every change: `{"event": "document", "revision": n}`. The UI
  re-reads `document.get` and, when `geometry_revision` changed, the geometry.
- Undo/redo is in Rust: every editing action is one undo step (a drag is one
  `document.transform` sent on release).

## State: `document.get` (tier `read`)

```ts
interface DocumentSummary {
  revision: number;            // increases on every change (new)
  geometry_revision: number;   // increases when geometry changed (new)
  name: string;                // project name ("" for a new one)
  dirty: boolean;              // changed since the last save (new)
  layers: Layer[];             // freekerf-document Layer, in output order
  objects: ObjectSummary[];    // in drawing order
  selection: number[];         // ObjectIds (new: selection lives with the document)
  current_layer: number;       // LayerId new shapes go to (new)
  can_undo: boolean;           // (new)
  can_redo: boolean;           // (new)
}

interface ObjectSummary {
  id: number;                  // ObjectId
  layer: number;               // LayerId
  name: string;
  kind: 'shape' | 'image' | 'text' | 'unsupported';
  bounds: [number, number, number, number] | null;   // [xmin, ymin, xmax, ymax] mm, rotated outline's box
  transform: [number, number, number, number, number, number]; // 2×3 affine, mm (new: see below)
  text?: { text: string; font: string; height_mm: number };    // kind = text (new)
  image?: { width_px: number; height_px: number; dither: string }; // kind = image
}
```

`Color` crosses as `"#RRGGBB"`. Layer codes shown in the UI (C00–C11, T1) are the
index of the layer colour in the design palette when it matches one, else none.

**New in the model: a per-object transform.** `freekerf-document` objects carry no
transform yet ("moving an object moves its geometry"). Scaling and rotating with
handles without losing precision needs `transform` (2×3 affine applied to the
geometry when planning). Until then `document.transform` may bake the transform
into the geometry; the summary then reports the identity.

## Raw payloads

| Tauri command | Returns | Format |
| --- | --- | --- |
| `document_geometry()` | every drawable segment of the document | "FKTP" v1 (`freekerf_service::preview`, 24 bytes per segment) with **`power` = index of the layer in `layers`** and **`command` = index of the object in `objects`**; arcs and Béziers flattened within 0.01 mm; transforms applied |
| `document_image(id)` | the processed raster of an image object, as engraved | **"FKIM" v1** (new): `magic "FKIM"`, `u32 version`, `u32 width`, `u32 height`, `f32 x, y, w, h` (mm), then `width × height` bytes (0 = burn, 255 = nothing) |
| `document_import(bytes)` | `ImportReport` (JSON) | request body: the file bytes, raw; headers `x-file-name`, `x-import-options` (JSON `ImportOptions`) |

The canvas uploads `document_geometry` as one segment batch and colours segments by
layer (the shader gets the layer colours as a uniform array); image objects are
uploaded as single-channel tiles. Hit testing uses the bounds from the summary,
then the segments of the candidates.

## Actions

| Action | Tier | Parameters | Result |
| --- | --- | --- | --- |
| `document.get` | read | — | `DocumentSummary` |
| `document.select` | ui | `{ids: number[], mode: "replace" \| "add" \| "toggle"}` | — |
| `document.select_all` | ui | — | — |
| `document.add_shape` | edit | `{shape: "rect" \| "ellipse", x, y, width, height}` (mm, on `current_layer`) | `{id}` |
| `document.add_text` | edit | `{text, x, y, height_mm, font?}` (single-stroke Hershey font by default) | `{id}` |
| `document.set_text` | edit | `{id, text?, height_mm?, font?}` | — |
| `document.transform` | edit | `{ids, op}` with `op` = `{translate: [dx, dy]}` \| `{scale: [sx, sy], pivot: [x, y]}` \| `{rotate_deg: a, pivot: [x, y]}` | — |
| `document.align` | edit | `{ids, how: "left" \| "center_h" \| "right" \| "top" \| "middle_v" \| "bottom"}` | — |
| `document.delete` | edit | `{ids}` | — |
| `document.duplicate` | edit | `{ids, offset_mm?: [dx, dy]}` | `{ids}` (the copies, selected) |
| `document.set_layer` | edit | `{ids, layer}` (move objects to a layer) | — |
| `document.layer_for_color` | edit | `{color: "#RRGGBB"}` (palette: the layer of this colour, created with defaults if missing) | `{layer}` |
| `document.set_current_layer` | ui | `{layer}` | — |
| `document.layer_update` | edit | `{layer, name?, mode?, speed_mm_min?, power?, passes?, fill?, visible?}` | — |
| `document.layer_move` | edit | `{layer, to: number}` (output order index) | — |
| `document.undo`, `document.redo` | edit | — | — |
| `document.new` | edit | — | — |
| `materials.list` | read | `{query?}` | `Material[]` (from `freekerf-materials`) |
| `document.apply_material` | edit | `{layer, material}` (sets mode, speed, power, passes, fill) | — |
| `migrate.detect` | read | — | `MigrationScan` |
| `migrate.run` | edit | `{items: MigrationItemKind[]}` | `MigrationReport` |

```ts
interface ImportOptions { colors: 'keep' | 'unify'; }     // SVG/DXF; images use the layer's raster settings
interface ImportReport { objects: number; layers: number; unsupported: number; warnings: string[]; }
interface MigrationScan {
  found: boolean;
  path: string | null;            // %APPDATA%/LaserGRBL, or the Wine prefix on Linux
  items: { kind: MigrationItemKind; count: number }[];
}
type MigrationItemKind = 'connection' | 'buttons' | 'materials' | 'counters' | 'shortcuts' | 'preferences' | 'language';
interface MigrationReport { imported: { kind: MigrationItemKind; count: number }[]; warnings: string[]; }
```

- `migrate.run` never writes the LaserGRBL files (`design/ux.md` flow 2) and keeps a
  log of what it imported. Custom buttons become `[[buttons]]` (M4a) and, with
  `freekerf-script`, scripts.
- Errors are the usual `ActionError` (`invalid_params`, `refused`, `failed`).

## Events

| Event | When | The UI |
| --- | --- | --- |
| `{"event": "document", "revision": n}` | after any change (edit, import, undo, load) | re-reads `document.get`; re-reads `document_geometry` when `geometry_revision` changed |

## What the UI does with it today (M4a, fake)

| Screen | Uses |
| --- | --- |
| Layers panel (rows, expand to edit mode / speed / power / passes, show, order) | `document.get`, `document.layer_update`, `document.layer_move`, `document.set_current_layer` |
| Palette (13 swatches) | `document.layer_for_color`, then `document.set_layer` (with a selection) or `document.set_current_layer` |
| Inspector (X/Y centre, W/H, rotation, keep proportions, text, layer, align, duplicate, delete) | `document.transform`, `document.set_text`, `document.set_layer`, `document.align`, `document.duplicate`, `document.delete` |
| Tools (select, rectangle, ellipse, text), keyboard (V/R/E/T, Del, Ctrl+D, Ctrl+A, arrows, Ctrl+Z/Y) | `document.select*`, `document.add_shape`, `document.add_text`, `document.transform`, `document.undo/redo` |
| Canvas: click / marquee selection, move, resize and rotate handles | bounds + `document_geometry`, `document.select`, `document.transform` on release |
| Import dialog (file, keep or unify colours) | `document_import` |
| Welcome flow with the LaserGRBL migration step | `migrate.detect`, `migrate.run` |

Not used by the M4a screens yet: `materials.list` and `document.apply_material`
(material picker), `document_image` (image objects come with M2's importers),
`document.new`. The active tool is not part of the document: it is the view
state's `tool` (`view.set_tool`, [shell.md](shell.md)). The screens are described in
[design-mode.md](design-mode.md).

## Phase 2 checklist (M4b)

1. M2 registers the actions above (names and parameters as here, or this page is
   updated in the same PR), describes their results (`Registry::describe_result`)
   and publishes the `document` event.
2. `freekerf-desktop` adds the three raw commands; `ipc/tauri.ts` implements
   `DocumentService` over `dispatch` and those commands.
3. Delete `ui/web/src/contract/document.ts`; import the same names from
   `ui/web/src/gen/api.ts`.
4. Turn `flags.designMode` on in distributed builds.
5. The component tests keep running on the fake; the 8 flows of `design/ux.md` run
   end to end on the real app (`ui/web/tests/e2e`).
