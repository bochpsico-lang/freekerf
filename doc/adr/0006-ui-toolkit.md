# 0006. UI toolkit: Flutter + flutter_rust_bridge (proposed)

- Status: **proposed** — accepted only after experiments A–C of [`doc/research/ui-toolkit.md`](../research/ui-toolkit.md) §8 pass
- Date: 2026-10-07
- Replaces the preliminary recommendation (Tauri 2) of `design/tech-notes.md`

## Context

The UI is a thin client of `freekerf-service` (PLAN §2, §9): app state lives in Rust and the UI renders it and dispatches actions. Requirements: Linux first (X11 and Wayland), Windows, macOS, then Android tablets with USB serial and iPad as a remote client; real multi-touch; a canvas with 20,000 paths and an 8000×6000 image at ≥ 50 fps; 60 Hz head tracking; screen readers; GPL-3.0-compatible licenses.

M3 was changed from a code spike to desk research. The research report and its verification notes are in `doc/research/ui-toolkit.md`. The verification found errors in part of the report's supporting evidence (§0 there), so this decision is not final yet.

## Proposal

- **Desktop and Android UI: Flutter**, rendering with Impeller, talking to the Rust core through **flutter_rust_bridge v2** (Dart FFI, typed data without serialization).
- **Android USB serial:** usb-serial-for-android through a small Kotlin plugin, behind `freekerf-transport`'s `Transport` trait.
- **Remote iPad client:** not Flutter Web; a small HTML5/WebGL page served by the Rust core over HTTP/WebSocket (`freekerf-remote`, M6).
- **Design tokens:** `design/tokens.json` stays the single source; a build step generates Dart theme data (and keeps the CSS for the remote page and the website).
- **Canvas:** static scene cached as a GPU picture and transformed while panning/zooming, re-rasterized at rest; the 60 Hz head on its own repaint boundary.

## Alternatives considered

- **Tauri 2:** reuses the HTML/CSS prototype, smallest installer; depends on the distribution's WebKitGTK on Linux (canvas/WebGL and touch risks, not yet measured). Raw-byte IPC exists in v2, so IPC alone does not rule it out.
- **Electron:** same Chromium everywhere and full prototype reuse; 300–500 MB idle memory, no Android.
- **Kotlin Multiplatform / Compose:** excellent on Android; on desktop a bundled JRE, Wayland touch limits and JNI/GC costs at 60 Hz.
- **Slint / iced / egui / Dioxus:** small and fast, but a full rewrite, weak gestures, and a custom CAD canvas with little ecosystem help.

## What decides it

Experiments A (canvas stress), B (IPC latency, including Tauri's raw `ipc::Response` and channels) and C (Wayland multi-touch) in `doc/research/ui-toolkit.md` §8, on Linux with Intel and NVIDIA GPUs.

- If **Flutter passes A–C**: accept this ADR.
- If **Flutter fails A or C on Linux and Tauri passes them**: switch the proposal to Tauri 2.
- If **both fail A on Linux**: choose between Electron (Chromium consistency, memory cost) and a renderer of our own inside the shell; product decision.

## Consequences (if accepted)

- The HTML prototype in `design` becomes a specification, not code to reuse; the UI is rebuilt in Dart.
- Two front ends exist from M6 on: the Flutter app and the small remote web page. Both use only the service API.
- Contributors need Dart for the UI (Rust for the core). Flutter is BSD-3-Clause and flutter_rust_bridge is MIT: GPL-compatible.
- The automation design (§9) is unchanged: view state stays in Rust; window screenshots come from Flutter (`RepaintBoundary.toImage`) for MCP.
- PLAN §2 (`ui/`, `apps/desktop`), §6 (UI end-to-end tooling) and M4 are updated when this ADR is accepted, not before.
