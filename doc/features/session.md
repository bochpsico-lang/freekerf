# Session: document, selection, view and history

The app's state is data in `freekerf_session::session::Session` (PLAN §9.1): the
document, the selection, the view (`freekerf_session::view::ViewState`: mode, tool,
panel, dialogs, toasts, viewport, focus, the pending laser request — rendered by the
desktop app, [shell.md](shell.md)) and the undo history. It changes only through
**actions** of the registry, so the CLI, `host --stdio`, scripts, agents and the UI do it
the same way, with the caller's tier checked on every call.

## Actions

| Group | Actions | Tier |
| --- | --- | --- |
| read | `session.snapshot` (revision, selection, current layer, view, undo/redo labels, layers and objects without geometry: bounds, transform, text), `document.get`, `document.to_fkproj`, `view.get` | read |
| selection and view | `document.select` (replace/add/remove/toggle), `document.select_all`, `document.clear_selection`, `document.set_current_layer`, `view.set_mode`, `view.set_tool`, `view.open_panel`, `view.close_panel`, `view.set_viewport`, `view.open_dialog`, `view.close_dialog`, `view.toast`, `view.dismiss_toast`, `view.fit`, `view.focus` | ui |
| document | `document.new`, `document.from_fkproj`, `document.import_data` (base64 bytes, or text for SVG/DXF), `document.delete`, `document.translate`, `document.move_to_layer`, `document.add_layer`, `document.update_layer`, `document.remove_layer`, `document.move_layer`, `document.layer_for_color`, `history.undo`, `history.redo` | edit |
| drawing | `document.add_shape` (rectangle, ellipse), `document.add_text`, `document.set_text`, `document.transform` (translate, scale or mirror about a pivot, rotate about a pivot, any matrix), `document.align` (left, center_h, right, top, middle_v, bottom), `document.duplicate` | edit |
| files (service) | `document.open`, `document.save`, `document.import` (a path; format from the extension), `job.from_document` (plan the document and load it as the job) | edit |

Parameters and results: [actions.md](actions.md) (generated). Object ids default to
the selection where an action takes `ids`.

- **Current layer.** New shapes and text go to the current layer
  (`document.set_current_layer`, or the layer `document.layer_for_color` returns);
  when it is gone, the first layer; with no layer at all, a black one is made. It is
  not an undo step; changing it is a `selection` event.
- New shapes, text and copies are selected (alone), so the next action applies to them.
- **Text** stays editable: `document.set_text` writes the strokes again through the
  object's transform ([projects.md](projects.md)).

## Undo, transactions and events

- Every document change is **one undo step** with a label (`import`, `move`,
  `layer settings`...); selection and view changes are not undo steps. At most 100
  steps are kept; opening or creating a document starts the history again.
- A **transaction** (`Session::begin` / `commit` / `rollback`) groups every change
  into one step, or undoes all of them, view included: scripts run inside one
  (`doc/features/scripting.md`). Transactions do not nest; undo and redo are refused
  inside one.
- Every change bumps the **revision** and produces an event (`document`,
  `selection` (also the current layer), `view`, `history`); the service forwards them to its subscribers as
  `{"event": "session", "revision": N, "change": "..."}` (`host --stdio` prints them).

## Layers

The session is core (no I/O, builds for wasm): `document.import_data` takes bytes;
the service's `document.import`, `document.open` and `document.save` read and write
the files.

## Tests

`crates/core/freekerf-session/src/session.rs` (undo, transactions, events, history
limit), `actions.rs` (every action, tiers, import formats),
`freekerf-service/tests/service.rs::document_session_actions_and_events`.
