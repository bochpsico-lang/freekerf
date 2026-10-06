# FreeKerf

Free software for designing jobs and driving laser machines (Grbl, grblHAL, Smoothie, Marlin…), written in Rust, cross-platform, for mouse, keyboard and touch. Successor of [LaserGRBL](https://github.com/arkypita/LaserGRBL).

> **Status: planning.** There is no code yet. What will be built, the layered architecture, the testing and documentation rules and the milestones are in **[PLAN.md](PLAN.md)**.

| Repository | Content |
| --- | --- |
| **freekerf** (this one) | Rust monorepo: core, config, native, runtime, UI and apps |
| [conformance](https://github.com/freekerf/conformance) | suite that specifies the behaviour of the current LaserGRBL core |
| [design](https://github.com/freekerf/design) | UX, brand, design system and clickable prototype |
| [library](https://github.com/freekerf/library) | materials and machine profiles |
| [website](https://github.com/freekerf/website) | website and user documentation |

## License

[GPL-3.0-or-later](LICENSE). Derived from LaserGRBL, Copyright (c) 2016 Diego Settimi (see [NOTICE](NOTICE)). Dependencies are checked with `cargo deny` ([deny.toml](deny.toml)).
