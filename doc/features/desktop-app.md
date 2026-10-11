# Desktop app

The FreeKerf desktop app is a [Tauri 2](https://v2.tauri.app/) window
(`apps/desktop/src-tauri`, crate `freekerf-desktop`) around the same
`freekerf-service` the CLI uses, with a web front end in `ui/web` (Svelte 5,
[ADR 0013](../adr/0013-ui-framework.md)). Decision: [ADR 0006](../adr/0006-ui-toolkit.md).

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
| `confirm_fire(id)` | the Fire laser button: runs the pending laser request after its countdown ([fire-confirmation.md](fire-confirmation.md)) |
| `preview()` | the tool path of the loaded program, raw bytes ([canvas.md](canvas.md)) |
| `subscribe_head(frames)` | head frames, raw 24 bytes, up to 60 Hz ([canvas.md](canvas.md)) |
| `bench_report(report)` | only with `--bench-canvas`: prints the canvas benchmark and exits |

- The window calls as `{kind: user, name: "desktop", max_tier: machine}`. Actions
  of tier `laser` (`job.run`, `job.resume`, `machine.send`, `machine.custom_code`)
  are refused; they run only through the fire confirmation (`fire.request`, then
  `confirm_fire` after the countdown).
- STOP (`machine.stop`) and the other `safety` actions are always allowed.
- No Tauri plugin is enabled and the capability file grants nothing: the web view
  has no file system, shell or network access of its own. The content security
  policy only allows the app's own resources and the IPC.
- The app makes no network calls (no telemetry, no update checks).

## Types

The TypeScript types of actions (parameters, results, tiers), events and settings
are generated from the Rust schemas into `ui/web/src/gen/api.ts`
([ADR 0015](../adr/0015-generated-ui-types.md)); `cargo xtask check-docs` fails in CI
when they are stale.

## Design system

Tokens (`ui/tokens/fk-tokens.css`), UI icons, fonts and app icons come from
`freekerf/design` at the commit in `ui/tokens/design.lock`
(`cargo xtask sync-tokens`, checked in CI). Credits: `ui/CREDITS.md`.

## Platforms

Linux (WebKitGTK 4.1) is the first target and runs the CI smoke test (the app
starts on Xvfb and stays up). Windows (WebView2) and macOS (WKWebView) compile in
CI on native runners (`native-check`, which checks the whole workspace); installers come with M5.

## Tests

- `apps/desktop/src-tauri/src/bridge.rs`: the window's caller (laser refused, STOP
  allowed, events forwarded as JSON).
- `ui/web`: Vitest unit and component tests (`npm test`), the architecture lint
  rules (`tests/unit/architecture.test.ts`) and the bundle check
  (`tests/unit/bundle-check.test.ts`).
- `ui/web/tests/web/` (Playwright, Chromium on `dev.html` and the fake service):
  German long strings in the three layouts, the Design mode gestures on the real
  WebGL2 canvas.
- `ui/web/tests/e2e/` (WebdriverIO + `tauri-driver`, Linux/WebKitWebDriver): the
  real app, the real service and the built-in emulator (never a serial port), a
  fresh `FREEKERF_HOME` per spec with a 1 s fire countdown. Produce: connect, console
  `$$`, jog, load a program, the fire confirmation (disabled during the countdown,
  cancel runs nothing), a job to its end, STOP during a run. Safety: `machine.send`
  called directly from the page is refused (only `fire.request` is allowed),
  Shift+Esc from a text field resets the controller, settings reach
  `settings.toml`. Run:

  ```bash
  cargo install tauri-driver --locked          # once; Linux also needs webkit2gtk-driver
  cd ui/web && npm run build
  cargo build -p freekerf-desktop --features custom-protocol
  xvfb-run -a npm run test:e2e                 # or without xvfb-run on a desktop
  ```

  WebKitWebDriver cannot type into file inputs: the tests set the program with a
  `DataTransfer` on the panel's own input (the native picker is not driven).
- CI jobs `ui` (with coverage in the job summary), `desktop` (smoke run and the
  end-to-end tests), `native-check` (Windows, macOS).
