# Roadmap

What was postponed, and open decisions. Milestones are in [PLAN.md](../PLAN.md) §10.

## Postponed from M0/M1

| Item | Why | When |
| --- | --- | --- |
| Telnet and WebSocket transports, network discovery | M5 scope in the plan | M5 |
| Fuzzing (`cargo-fuzz` targets for G-code) | planned with the importers' parsers | M2 |
| Human fire confirmation of `laser` actions, the permission dialog (`Prompter`), "agent connected" and running-script indicators | need the desktop UI; headless refuses both today | M4a |
| Sounds, notifications, hotkeys, safety countdown | UI concerns | M4a (hotkeys, countdown), M5 (sounds, notifications) |

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
