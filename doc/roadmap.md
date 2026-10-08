# Roadmap

What was postponed, and open decisions. Milestones are in [PLAN.md](../PLAN.md) §10.

## Postponed from M0/M1

| Item | Why | When |
| --- | --- | --- |
| Telnet and WebSocket transports, network discovery | M5 scope in the plan | M5 |
| `freekerf-ipc`, `freekerf-script`, `freekerf-mcp`, Python client | M2 scope | M2 |
| Fuzzing (`cargo-fuzz` targets for G-code) | planned with the importers' parsers | M2 |
| Permission grants (this run / session / always) | needs callers other than the user (scripts, agents) | M2 |
| Sounds, notifications | UI concerns (hotkeys and the safety countdown are done in M4a) | M5 |
| Importing LaserGRBL settings (`Settings.bin`), buttons, counters | `freekerf-migrate` | M2 |
| Canvas ≥ 50 fps measurement on Linux with a GPU (`freekerf-desktop --bench-canvas`) | the cloud CI and dev containers have no GPU; only the software floor is recorded | before M4a is closed |
| Head interpolation between status reports along the tool path | status reports arrive a few times per second; the head jumps between them | M4b |
| Thick lines in the canvas (selection emphasis, kerf width) | WebGL lines are 1 px; needs quads in the vertex shader | M4b |
| Job start (absolute / current position / user origin) with the 3×3 anchor | needs the planner to offset the job | M4b |
| Drop a file on the window to open it | Tauri intercepts drops; done with the import flow | M4b |
| Design mode on the real document (M2 implements `doc/features/ui-contract.md`; flag on in builds) | M2's crates and actions | M4b |
| Material picker in the layer editor, context menu, z-order, array, node editing, trace, boolean operations | need `freekerf-materials` and M2's geometry | M4b |
| Welcome dialog on the first run | needs a "welcomed" flag in `[ui]` (settings migration) | M4b |
| Gestures: no hover cursor feedback over handles; the rotation handle sits above the box and can leave the view on small selections near the top edge; trackpad pinch on WebKitGTK arrives as Ctrl+wheel only | found while building the Design mode | M4b |
| WebGPU renderer | not in WebKitGTK yet; same interface as the WebGL2 renderer (ADR 0012) | later |
| Tauri dependencies flagged unmaintained (gtk-rs 0.18 macros, `unic-*` via `urlpattern`) | ignored in `deny.toml` (no vulnerability); upstream Tauri must move | when Tauri updates |

## Decided

- UI toolkit: Tauri 2, desktop first (ADR 0006). Multi-touch problems found on Linux are listed here when they appear and handled later.

- No auto-update for now (PLAN §14).
- No MCP prompts for now (PLAN §14).

## Open


- MCP: should `allow_laser` exist at all (PLAN §15)?
