# Design mode

The Design mode is where a job is drawn: colour layers, objects, the inspector, the
palette and the drawing tools, over the same canvas as the Produce mode. It works on
the session's document ([session.md](session.md)) through the same actions scripts
and agents use, so everything a person does here can be scripted. The app starts in
it; the narrow layout (< 820 px) is always Produce.

Code: `ui/web/src/design/` (screens, the editor, `import.ts`),
`ui/web/src/state/document.svelte.ts` (the projection: `session.snapshot` on every
document, selection or history event; the geometry again only when
`document_revision` changed), `ui/web/src/dev/fake-document.ts` (the same actions in
memory, for development in a browser and component tests).

## Screens

| Screen | What it does | Actions |
| --- | --- | --- |
| Tool rail | select (V), rectangle (R), ellipse (E), text (T), import (Ctrl+I), weld and subtract (2+ selected), offset and array (open the inspector), undo, redo; node editing, trace and material test are listed and disabled until they exist | `view.set_tool`, `view.open_dialog`, `document.boolean`, `view.open_panel`, `history.undo/redo` |
| Layers panel | one row per colour layer in output order: code (C00…), mode, object count, speed · power · passes; the row makes the layer current; show/hide; expand to edit mode, speed, power (% of `$30`), passes; move up/down in the output order | `document.set_current_layer`, `document.update_layer`, `document.move_layer` |
| Palette (over the canvas) | colour = layer: the colour's layer (created if missing) becomes the layer of new shapes, and a selection moves to it | `document.layer_for_color`, `document.move_to_layer` |
| Inspector (Object panel) | X/Y of the centre, width, height (keep proportions), rotation (read from the object's transform), text and its size, layer swatches, align (2+ objects), combine (weld, subtract, intersect; 2+ objects), offset outline (distance), array (columns, rows, gap), duplicate, delete | `document.transform`, `document.set_text`, `document.align`, `document.boolean`, `document.offset`, `document.array`, `document.duplicate`, `document.delete` |
| Raster dialog (an image selected: *Image engraving…* in the inspector) | method (grey levels, dithering with the 9 algorithms, as it is, trace outlines), resolution, brightness, contrast, threshold, direction, speed, minimum and maximum power; a preview of the bitmap the settings make (computed in Rust); Apply is one undo step | `document.raster_settings`, `document.raster_preview`, `document.set_raster` |
| Import dialog | a file (SVG, DXF, image) and its colours: keep (one layer per colour) or put everything on the current layer; the bytes cross as base64; the report comes back as a toast | `document.import_data` |
| Welcome dialog | set up the machine (opens Settings), import from LaserGRBL (scan, choose, report; the LaserGRBL files are only read), explore with the emulator; reopened from Settings | `migrate.detect`, `migrate.run`, `machine.connect` |

The active tool is part of the view state (`tool`, [shell.md](shell.md)), so scripts
and agents see and set it like a person.

## Canvas editing

| Gesture | Effect |
| --- | --- |
| Click / tap an object | select it (topmost on a visible layer, bounds + 4 px) |
| Shift/Ctrl + click | toggle it in the selection |
| Click on empty space | clear the selection |
| Drag on empty space | window (left→right: inside) or crossing (right→left: touching) selection |
| Drag an object | select it (kept if already selected) and move the selection |
| Drag a corner handle | resize from the opposite corner, proportions kept (Shift frees them; Alt resizes from the centre) |
| Drag a side handle | resize one axis |
| Drag the round handle | rotate around the centre (Shift: 15° steps; touch: 5° steps) |
| Rectangle / ellipse tool | click: a 40×30 mm rectangle or a 30 mm circle centred there; drag: the box drawn; back to select |
| Text tool | click: "Text", 10 mm high (drag: the height of the box); opens the inspector to type it |
| A second finger | cancels the drag (nothing is sent) and pinches |

While dragging, the overlay shows the outline the selection will have; on release
the window sends **one** `document.transform` (one undo step). Handles are 8 px
(14 px hit area) with a mouse and 14 px (36 px hit area) after a touch.

The canvas draws the document geometry (`AppService.documentGeometry`, the desktop
command `document_geometry`: "FKTP" segments, `power` = layer index; arcs and curves
flattened within 0.01 mm; images as their frame) coloured by layer
(`CanvasView.setLayerColors`), hidden layers excluded; the G-code preview is not
drawn in this mode.

## Keyboard (Design mode, not while typing or over a dialog)

| Keys | Command |
| --- | --- |
| V · R · E · T | select · rectangle · ellipse · text |
| Ctrl+I | import |
| Ctrl+Z · Ctrl+Y / Ctrl+Shift+Z | undo · redo |
| Ctrl+A · Ctrl+D · Del / Backspace | select all · duplicate · delete |
| Arrows | move the selection 1 mm (Shift 10 mm, Alt 0.1 mm) |
| Esc | back to the select tool, else clear the selection |

## Limits

- Images show as their frame on the canvas; the dithered dots show in the raster
  dialog's preview.
- No material picker in the layer editor yet, no context menu, no z-order, no
  node editing: listed in [roadmap.md](../roadmap.md). Booleans and offsets turn
  curves into polygons (0.01 mm) and drop editable text.
- The welcome dialog does not open by itself on the first run yet (needs a
  "welcomed" flag in the settings).
- The fake document reads simple SVG shapes only (rect, circle, ellipse, line,
  polyline, polygon); the app uses the real importers.

## Tests

- `ui/web/src/design/editor.test.ts`: hit test, click/shift/marquee selection,
  handle drags with preview and one transform on release, cancel, rotation snap,
  placing shapes and text, nudge.
- `ui/web/src/design/transform.test.ts`: the handle maths.
- `ui/web/src/design/design.test.ts` (component, on the fake): tools and undo,
  keyboard, layers panel, palette, inspector, import, welcome and migration.
- `ui/web/src/design/import.test.ts`, `ui/web/src/dev/fake-document.test.ts`,
  `ui/web/src/state/document.test.ts`.
- `ui/web/tests/e2e/specs/design.e2e.ts` (the real app): draw with the tool, import
  an SVG through the dialog, the palette and undo, a photo and its raster settings.
- `ui/web/tests/web/design-canvas.spec.ts` (Playwright, real WebGL2 canvas): draw,
  select, move, resize with a handle, marquee, undo; the palette.
- `ui/web/tests/web/long-strings.spec.ts`: German in the Design screens at 1366 and
  1000 px.
- Rust: the actions (`freekerf-session/src/actions.rs`), the geometry
  (`freekerf-service/src/preview.rs`, desktop `bridge.rs::preview_and_head`),
  `view.rs` (`tool`), `service.rs::view_actions_change_the_view_and_publish_it`.
