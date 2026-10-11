# FreeKerf

Free software for designing jobs and driving laser machines (Grbl, grblHAL, Smoothie, Marlin…), written in Rust, cross-platform, for mouse, keyboard and touch. Successor of [LaserGRBL](https://github.com/arkypita/LaserGRBL).

> **Status: desktop MVP (milestones M0 to M4b).** The desktop app designs and runs jobs: import SVG, DXF and images, shapes, text, booleans, arrays, colour layers with cut and raster settings, material test, preview and time estimate; connect to Grbl/grblHAL, Smoothie, Marlin and VigoWork boards (or the built-in emulator), jog, frame, run, pause, overrides, job recovery, STOP always visible, a fire confirmation before the laser turns on; LaserGRBL settings and custom buttons are migrated on first start; scripts and AI agents (MCP) drive the same actions. There are no installers yet (M5): build it from source. The plan is in **[PLAN.md](PLAN.md)**; how to work on the code is in **[AGENTS.md](AGENTS.md)**.

## Try it

The desktop app needs Rust ≥ 1.90 and Node.js 24 (the version CI uses); on Linux also WebKitGTK 4.1
(Debian/Ubuntu: `sudo apt install libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`).

```bash
cd ui/web && npm ci && npm run build && cd ../..                       # the front end, into ui/web/dist
cargo run --release -p freekerf-desktop --features custom-protocol      # the app, with the front end embedded
```

Without `--features custom-protocol` the app loads the front end from the Vite dev
server (`npm run dev` in `ui/web`, for development) and shows an empty window when it
is not running. Opening `http://localhost:5173` in a browser is not the app either: the
page needs the desktop shell, and `dev.html` there is the UI on a fake service.
To try it without a machine, connect to **FreeKerf emulator (no machine)**.

The command line:

```bash
cargo run -p freekerf-cli -- analyze job.nc                 # time, ranges, quadrant
cargo run -p freekerf-cli -- send job.nc --emulator          # stream to the built-in emulator
cargo run -p freekerf-cli -- send job.nc --port /dev/ttyUSB0 # stream to a real machine
cargo run -p freekerf-cli -- console --emulator              # interactive console
```

Documentation: [features](doc/features/README.md), [architecture](doc/architecture.md),
[divergences from LaserGRBL](doc/divergences.md).

| Repository | Content |
| --- | --- |
| **freekerf** (this one) | Rust monorepo: core, config, native, runtime, UI and apps |
| [conformance](https://github.com/freekerf/conformance) | suite that specifies the behaviour of the current LaserGRBL core |
| [design](https://github.com/freekerf/design) | UX, brand, design system and clickable prototype |
| [library](https://github.com/freekerf/library) | materials and machine profiles |
| [website](https://github.com/freekerf/website) | website and user documentation |

## License

[GPL-3.0-or-later](LICENSE). Derived from LaserGRBL, Copyright (c) 2016 Diego Settimi (see [NOTICE](NOTICE)). Dependencies are checked with `cargo deny` ([deny.toml](deny.toml)).
