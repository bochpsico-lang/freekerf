# Session: document, selection, view and history

The app's state is data in `freekerf_session::session::Session` (PLAN §9.1): the
document, the selection, the view (mode, open panels, viewport, dialogs — the desktop
app renders it from M4) and the undo history. It changes only through **actions** of
the registry, so the CLI, `host --stdio`, scripts and agents (and later the UI) do it
the same way, with the caller's tier checked on every call.

## Actions

| Group | Actions | Tier |
| --- | --- | --- |
| read | `session.snapshot` (revision, selection, view, undo/redo labels, layers and objects without geometry), `document.get`, `document.to_fkproj`, `view.get` | read |
| selection and view | `document.select` (replace/add/remove), `document.select_all`, `document.clear_selection`, `view.set_mode`, `view.open_panel`, `view.close_panel`, `view.set_viewport`, `view.open_dialog`, `view.close_dialog` | ui |
| document | `document.new`, `document.from_fkproj`, `document.import_data` (base64 bytes), `document.delete`, `document.translate`, `document.move_to_layer`, `document.add_layer`, `document.update_layer`, `document.remove_layer`, `history.undo`, `history.redo` | edit |
| files (service) | `document.open`, `document.save`, `document.import` (a path; format from the extension), `job.from_document` (plan the document and load it as the job) | edit |

Parameters and results: [actions.md](actions.md) (generated). Object ids default to
the selection where an action takes `ids`.

## Undo, transactions and events

- Every document change is **one undo step** with a label (`import`, `move`,
  `layer settings`...); selection and view changes are not undo steps. At most 100
  steps are kept; opening or creating a document starts the history again.
- A **transaction** (`Session::begin` / `commit` / `rollback`) groups every change
  into one step, or undoes all of them, view included: scripts run inside one
  (`doc/features/scripting.md`). Transactions do not nest; undo and redo are refused
  inside one.
- Every change bumps the **revision** and produces an event (`document`,
  `selection`, `view`, `history`); the service forwards them to its subscribers as
  `{"event": "session", "revision": N, "change": "..."}` (`host --stdio` prints them).

## Layers

The session is core (no I/O, builds for wasm): `document.import_data` takes bytes;
the service's `document.import`, `document.open` and `document.save` read and write
the files.

## Tests

`crates/core/freekerf-session/src/session.rs` (undo, transactions, events, history
limit), `actions.rs` (every action, tiers, import formats),
`freekerf-service/tests/service.rs::document_session_actions_and_events`.
