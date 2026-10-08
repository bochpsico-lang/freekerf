# Desktop app

The FreeKerf desktop app is a [Tauri 2](https://v2.tauri.app/) window
(`apps/desktop/src-tauri`, crate `freekerf-desktop`) around the same
`freekerf-service` the CLI uses, with a web front end in `ui/web` (Svelte 5,
[ADR 0011](../adr/0011-ui-framework.md)). Decision: [ADR 0006](../adr/0006-ui-toolkit.md).

## Running it

See `AGENTS.md` ("Desktop app"). In short: `npm ci` in `ui/web`, then
`cargo run -p freekerf-desktop` (development, loads the Vite dev server) or
`npm run build && cargo run -p freekerf-desktop --features custom-protocol`
(the front end embedded, as in a release). `FREEKERF_HOME` points the app at
another settings directory (tests).

`npm run dev` and `http://localhost:5173/dev.html` run the same UI in a browser on
the **fake service** (`ui/web/src/dev/`), without Rust: useful for layout work and
for the Design mode screens until M2 lands.

## Boundary between the window and the app

| Tauri command | Does |
| --- | --- |
| `dispatch(action, params)` | runs an action of the registry ([actions.md](actions.md)) and returns its JSON result or an `ActionError` |
| `catalog()` | the action specs (name, tier, parameter and result schemas) |
| `subscribe(events)` | streams service events (`EventMessage` JSON) on a Tauri channel |

- The window calls as `{kind: user, name: "desktop", max_tier: machine}`. Actions
  of tier `laser` (`job.run`, `job.resume`, `machine.send`, `machine.custom_code`)
  are **refused** until the fire confirmation (PLAN §9.5) is in place.
- STOP (`machine.stop`) and the other `safety` actions are always allowed.
- No Tauri plugin is enabled and the capability file grants nothing: the web view
  has no file system, shell or network access of its own. The content security
  policy only allows the app's own resources and the IPC.
- The app makes no network calls (no telemetry, no update checks).

## Types

The TypeScript types of actions (parameters, results, tiers), events and settings
are generated from the Rust schemas into `ui/web/src/gen/api.ts`
([ADR 0013](../adr/0013-generated-ui-types.md)); `cargo xtask check-docs` fails in CI
when they are stale.

## Design system

Tokens (`ui/tokens/fk-tokens.css`), UI icons, fonts and app icons come from
`freekerf/design` at the commit in `ui/tokens/design.lock`
(`cargo xtask sync-tokens`, checked in CI). Credits: `ui/CREDITS.md`.

## Platforms

Linux (WebKitGTK 4.1) is the first target and runs the CI smoke test (the app
starts on Xvfb and stays up). Windows (WebView2) and macOS (WKWebView) compile in
CI on native runners (`desktop-platforms`); installers come with M5.

## Tests

- `apps/desktop/src-tauri/src/bridge.rs`: the window's caller (laser refused, STOP
  allowed, events forwarded as JSON).
- `ui/web`: Vitest unit and component tests (`npm test`), the architecture lint
  rules (`tests/unit/architecture.test.ts`) and the bundle check
  (`tests/unit/bundle-check.test.ts`).
- CI jobs `ui`, `desktop`, `desktop-platforms`.
