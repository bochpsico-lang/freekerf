# Roadmap

What was postponed, and open decisions. Milestones are in [PLAN.md](../PLAN.md) §10.

## Postponed from M0/M1

| Item | Why | When |
| --- | --- | --- |
| Telnet and WebSocket transports, network discovery | M5 scope in the plan | M5 |
| `freekerf-ipc`, `freekerf-script`, `freekerf-mcp`, Python client | M2 scope | M2 |
| Fuzzing (`cargo-fuzz` targets for G-code) | planned with the importers' parsers | M2 |
| Permission grants (this run / session / always) | needs callers other than the user (scripts, agents) | M2 |
| Human fire confirmation of `laser` actions | needs the desktop UI | M4a |
| Sounds, notifications, hotkeys, safety countdown | UI concerns | M4a (hotkeys, countdown), M5 (sounds, notifications) |
| Importing LaserGRBL settings (`Settings.bin`), buttons, counters | `freekerf-migrate` | M2 |

## Postponed from M2

| Item | Why | When |
| --- | --- | --- |
| Per-object transform in the document (scale/rotate without touching the geometry) | needs the editor's handles to be useful | M4/M5 |
| Bundling `autotrace` with the desktop app (built per platform from a pinned commit, source published with each release) | needs the installers; M2 runs it from the `PATH` (ADR 0010) | M4 (installers) |
| Keeping imported LaserGRBL user materials; downloading library packages | come with the material library UI | v1 |
| Zipped project container (`.fkproj` with images as separate entries) | JSON with base64 images is enough until projects with many large images appear | when needed |

## Decided

- UI toolkit: Tauri 2, desktop first (ADR 0006). Multi-touch problems found on Linux are listed here when they appear and handled later.

- No auto-update for now (PLAN §14).
- No MCP prompts for now (PLAN §14).

## Open


- MCP: should `allow_laser` exist at all (PLAN §15)?
