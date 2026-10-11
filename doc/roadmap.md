# Roadmap

What was postponed, and open decisions. Milestones are in [PLAN.md](../PLAN.md) §10.

## Postponed from M0/M1

| Item | Why | When |
| --- | --- | --- |
| Telnet and WebSocket transports, network discovery | M5 scope in the plan | M5 |
| Fuzzing (`cargo-fuzz` targets for G-code) | planned with the importers' parsers | M2 |
| The permission dialog (`Prompter`), "agent connected" and running-script indicators in the desktop app (the fire confirmation is done: M4a) | need the desktop UI; headless refuses permission requests today | M4b |
| Sounds, notifications | UI concerns (hotkeys and the safety countdown are done in M4a) | M5 |
| Canvas ≥ 50 fps measurement on Linux with a GPU (`freekerf-desktop --bench-canvas`) | the cloud CI and dev containers have no GPU; only the software floor is recorded | before M4a is closed |
| Head interpolation between status reports along the tool path | status reports arrive a few times per second; the head jumps between them | M4b |
| Thick lines in the canvas (selection emphasis, kerf width) | WebGL lines are 1 px; needs quads in the vertex shader | M4b |
| Job start (absolute / current position / user origin) with the 3×3 anchor | needs the planner to offset the job | M4b |
| Drop a file on the window to open it | Tauri intercepts drops; done with the import flow | M4b |
| Design mode on the real document (M2 implements `doc/features/ui-contract.md`; flag on in builds) | M2's crates and actions | M4b |
| Material picker in the layer editor, context menu, z-order, array, node editing, trace, boolean operations | need `freekerf-materials` and M2's geometry | M4b |
| Welcome dialog on the first run | needs a "welcomed" flag in `[ui]` (settings migration) | M4b |
| Gestures: no hover cursor feedback over handles; the rotation handle sits above the box and can leave the view on small selections near the top edge; trackpad pinch on WebKitGTK arrives as Ctrl+wheel only | found while building the Design mode | M4b |
| App end-to-end tests on Windows (WebView2 driver) and macOS (no WKWebView driver for Tauri) | Linux runs them in CI; PLAN §6 asks Linux and Windows | M5 |
| Coverage of the browser-only canvas code (Playwright/E2E instrumentation) and a frontend coverage gate | Vitest in jsdom cannot run WebGL2 | M4b |
| WebGPU renderer | not in WebKitGTK yet; same interface as the WebGL2 renderer (ADR 0014) | later |
| Tauri dependencies flagged unmaintained (gtk-rs 0.18 macros, `unic-*` via `urlpattern`) | ignored in `deny.toml` (no vulnerability); upstream Tauri must move | when Tauri updates |

## Postponed from M2

| Item | Why | When |
| --- | --- | --- |
| Per-object transform in the document (scale/rotate without touching the geometry) | needs the editor's handles to be useful | M4/M5 |
| Bundling `autotrace` with the desktop app (built per platform from a pinned commit, source published with each release) | needs the installers; M2 runs it from the `PATH` (ADR 0010) | M4 (installers) |
| Keeping imported LaserGRBL user materials; downloading library packages | come with the material library UI | v1 |
| MCP: Streamable HTTP transport (`127.0.0.1`, token, `Origin` check), screenshots | optional transport; screenshots need the desktop window | M4/M5 |
| Zipped project container (`.fkproj` with images as separate entries) | JSON with base64 images is enough until projects with many large images appear | when needed |

## Decided

- UI toolkit: Tauri 2, desktop first (ADR 0006). Multi-touch problems found on Linux are listed here when they appear and handled later.

- No auto-update for now (PLAN §14).
- No MCP prompts for now (PLAN §14).
- Agents never run `laser` actions; there is no `allow_laser` (2026-10-10).
- No Python client: scripts are JavaScript in the app, `freekerf run` covers the
  command line, and the local endpoint protocol (`doc/features/ipc.md`) stays open
  for an external client if one is ever needed (2026-10-10).

## Open

None right now.
