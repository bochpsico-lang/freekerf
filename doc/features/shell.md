# App shell

The frame of the desktop app around the canvas: top bar, tool rail, dock, status
bar, bottom bar, dialogs and toasts. It follows `freekerf/design` (`ux.md`,
`interaction.md`, `design-system.md`) and is grown from its prototype: the markup
and `ui/web/src/styles/shell.css` keep the prototype's class names. Code:
`ui/web/src/App.svelte`, `ui/web/src/shell/`.

## View state

What the app shows is data in Rust (`freekerf_session::view::ViewState`), held by
the session next to the document and the selection ([session.md](session.md)), so
script transactions roll it back too. It changes only through `view.*` actions
([actions.md](actions.md)); every change is a `{"event": "session", "revision": n,
"change": "view"}` event and the UI reads `view.get` again. The UI renders it;
scripts, agents and tests read and drive the same screen.

| Field | Meaning | Actions |
| --- | --- | --- |
| `mode` | `design` (default) or `produce` (Produce opens the Machine panel) | `view.set_mode` |
| `tool` | Design mode tool: `select`, `rect`, `ellipse`, `text` | `view.set_tool` |
| `panel`, `drawer_open` | active dock panel; drawer open in the medium layout | `view.open_panel {name}`, `view.close_panel {name}` |
| `dialogs` | open dialogs, bottom to top (closing the fire confirmation drops its request) | `view.open_dialog {name}`, `view.close_dialog {name?}` |
| `toasts` | at most 3 messages (i18n id + args) | `view.toast`, `view.dismiss_toast` |
| `viewport` | `center` `[x, y]` (mm) and `zoom` (CSS px per mm) | `view.set_viewport` |
| `fit_request` | increases on each `view.fit` | `view.fit` |
| `focus` | element with the keyboard focus | `view.focus` |
| `fire_request` | the laser action waiting for the person's confirmation | `fire.request`, `fire.cancel` ([fire-confirmation.md](fire-confirmation.md)) |

Local to the window (purely visual, PLAN §3): the layout (from the window width),
the kind of the last pointer (for the automatic density), hover, a gesture in
progress. The camera is local while a gesture runs and is committed with
`view.set_viewport` 250 ms after it settles; a viewport or a fit request set by
someone else is applied to the canvas.

## Layouts

| Width | Layout | Composition |
| --- | --- | --- |
| ≥ 1180 px | wide | tool rail · canvas · dock (Layers on top in Design, tabs) |
| 820–1179 px | medium | tool rail · canvas · panel rail; panels open as drawers over the canvas (in Produce the Machine panel is docked) |
| < 820 px | narrow | canvas preview on top, panels below, **bottom bar with STOP**; Produce only (no editing) |

## Safety

- **STOP** is in the top bar in every layout, and in the bottom bar (thumb zone) in
  the narrow layout. The top bar is above the dialog backdrop and drawers
  (`z-index`), so STOP stays visible and clickable with any dialog open.
- **Shift+Esc** is STOP from anywhere, text fields and dialogs included (captured
  before any other handler).
- Dialogs are not modal: a backdrop under the top bar; clicking it closes the top
  dialog, **except the fire confirmation** (no cancel by an accidental touch).
- The machine state is always visible: colour + icon + word, stripes while running,
  blinking on alarm (off with `prefers-reduced-motion`).

## Keyboard

| Keys | Command |
| --- | --- |
| Shift+Esc | STOP (everywhere) |
| F5 / F6 / F7 | Start (opens the fire confirmation) / pause–resume / frame |
| Esc | pause a running job; otherwise close the dialog on top |
| Ctrl+1 / Ctrl+2 | Design / Produce |
| + / − · F · Ctrl+0 | zoom · fit the table |
| F1 or ? | keyboard shortcuts |

Design mode keys (tools, undo, duplicate, delete, nudge): [design-mode.md](design-mode.md).
Pure mapping: `ui/web/src/shell/shortcuts.ts`.

## Dispatch-only

The UI changes app state only with `service.dispatch(action)` (PLAN §9, principle
7). Enforced by:

- the projections of app state (`src/state/`) expose getters only (TypeScript
  rejects writes);
- the ESLint rule `freekerf/no-app-state-mutation` (`ui/web/eslint-rules/`): no
  assignment, update or `delete` on `machine`, `preview`, `view`, `settings`,
  `doc`, `job`, `console`, `materials` outside `src/state/`;
- `localStorage`, `sessionStorage` and `indexedDB` are forbidden (app state lives
  in Rust);
- `dispatch` only accepts the generated `ActionName`s, so every control maps to a
  registry action (no UI-only actions).

`tests/unit/architecture.test.ts` proves the rules fail on violations.

## Design mode flag

The Design mode screens are built against the UI contract until M2 lands (M4b).
`flags.designMode` is on in development and tests and off in distributed builds
(`VITE_FK_DESIGN_MODE=1` turns it on); without it, or without `AppService.document`
(the desktop app until M4b), the mode switch shows Produce only. See
[design-mode.md](design-mode.md).

## Tests

`src/App.test.ts` (STOP with a dialog open, Shift+Esc from a text field, modes,
settings, backdrop vs. fire confirmation, toasts, keyboard, narrow layout),
`src/shell/shortcuts.test.ts`, `src/state/environment.test.ts`, the architecture
lint tests; Rust: `freekerf-session` `view` tests and
`service.rs::view_actions_change_the_view_and_publish_it`.
