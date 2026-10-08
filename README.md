# FreeKerf

Free software for designing jobs and driving laser machines (Grbl, grblHAL, Smoothie, Marlin…), written in Rust, cross-platform, for mouse, keyboard and touch. Successor of [LaserGRBL](https://github.com/arkypita/LaserGRBL).

> **Status: headless core (milestones M0 and M1).** The machine core and the `freekerf` command line work: G-code analysis, streaming to Grbl/grblHAL, Smoothie, Marlin and VigoWork boards, jog, overrides, jobs with passes and resume, a built-in emulator. They pass the protocol and G-code golden layers of the [conformance](https://github.com/freekerf/conformance) suite. The desktop app comes with M4 (M4a: shell and Produce mode; M4b: Design mode on top of M2). The plan is in **[PLAN.md](PLAN.md)**; how to work on the code is in **[AGENTS.md](AGENTS.md)**.

## Try it

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
