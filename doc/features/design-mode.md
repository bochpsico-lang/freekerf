# Design mode (M4a, on the UI contract)

The Design mode is where a job is drawn: colour layers, objects, the inspector, the
palette and the drawing tools, over the same canvas as the Produce mode. In M4a it
is built **against the UI contract** ([ui-contract.md](ui-contract.md)) and a fake
document (`ui/web/src/dev/fake-document.ts`); M2 implements the contract in Rust
(M4b) and the screens stay as they are.

It is behind `flags.designMode` (on in development and tests, off in distributed
builds) **and** needs `AppService.document`: the desktop app has none until M4b, so
there the mode switch shows Produce only. The narrow layout (< 820 px) is always
Produce.

Code: `ui/web/src/design/` (screens and the editor), `ui/web/src/contract/document.ts`
(the contract types), `ui/web/src/state/document.svelte.ts` (the projection).

## Screens

| Screen | What it does | Actions |
| --- | --- | --- |
| Tool rail | select (V), rectangle (R), ellipse (E), text (T), import (Ctrl+I), undo, redo; node editing, trace, boolean operations, offset, array and material test are listed and disabled until M2 | `view.set_tool`, `view.open_dialog`, `document.undo/redo` |
| Layers panel | one row per colour layer in output order: code (C00…), mode, object count, speed · power · passes; the row makes the layer current; show/hide; expand to edit mode, speed, power (% of `$30`), passes; move up/down in the output order | `document.set_current_layer`, `document.layer_update`, `document.layer_move` |
| Palette (over the canvas) | colour = layer: with a selection, moves it to that colour's layer (created if missing); without, makes it the layer of new shapes | `document.layer_for_color`, `document.set_layer` / `document.set_current_layer` |
| Inspector (Object panel) | X/Y of the centre, width, height (keep proportions), rotation, text and its size, layer swatches, align (2+ objects), duplicate, delete | `document.transform`, `document.set_text`, `document.align`, `document.duplicate`, `document.delete` |
| Import dialog | a file (SVG, DXF, image) and its colours: keep (one layer per colour) or put everything on the current layer; the report comes back as a toast | `document_import` (raw) |
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

The canvas draws the document geometry ("FKTP" segments, `power` = layer index)
coloured by layer (`CanvasView.setLayerColors`), hidden layers excluded; the G-code
preview is not drawn in this mode.

## Keyboard (Design mode, not while typing or over a dialog)

| Keys | Command |
| --- | --- |
| V · R · E · T | select · rectangle · ellipse · text |
| Ctrl+I | import |
| Ctrl+Z · Ctrl+Y / Ctrl+Shift+Z | undo · redo |
| Ctrl+A · Ctrl+D · Del / Backspace | select all · duplicate · delete |
| Arrows | move the selection 1 mm (Shift 10 mm, Alt 0.1 mm) |
| Esc | back to the select tool, else clear the selection |

## Limits (M4a)

- The fake importer reads simple SVG shapes only (rect, circle, ellipse, line,
  polyline, polygon); DXF and images come with M2's importers.
- No material picker in the layer editor yet (`materials.list` /
  `document.apply_material` are in the contract), no context menu, no z-order, no
  array, no node editing: listed in [roadmap.md](../roadmap.md).
- The welcome dialog does not open by itself on the first run yet (needs a
  "welcomed" flag in the settings, M4b).

## Tests

- `ui/web/src/design/editor.test.ts`: hit test, click/shift/marquee selection,
  handle drags with preview and one transform on release, cancel, rotation snap,
  placing shapes and text, nudge.
- `ui/web/src/design/transform.test.ts`: the handle maths.
- `ui/web/src/design/design.test.ts` (component, on the fake): mode hidden without a
  document, tools and undo, keyboard, layers panel, palette, inspector, import,
  welcome and migration.
- `ui/web/src/dev/fake-document.test.ts`, `ui/web/src/state/document.test.ts`.
- `ui/web/tests/web/design-canvas.spec.ts` (Playwright, real WebGL2 canvas): draw,
  select, move, resize with a handle, marquee, undo; the palette.
- `ui/web/tests/web/long-strings.spec.ts`: German in the Design screens at 1366 and
  1000 px.
- Rust: `view.rs` (`tool`), `service.rs::view_actions_change_the_view_and_publish_it`.
