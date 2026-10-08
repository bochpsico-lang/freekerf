# Features

One page per feature: what it does, how to use it, configuration, limits, safety,
tests and divergences from LaserGRBL. The action catalogue is generated.

| Page | Content | Since |
| --- | --- | --- |
| [connection.md](connection.md) | transports, connect sequence, status polling, `$$`/`$I`, timeouts | M1 |
| [streaming.md](streaming.md) | buffered / synchronous / repeat-on-error, RX buffer accounting | M1 |
| [job-lifecycle.md](job-lifecycle.md) | run, passes, abort, STOP, resume, issues, progress | M1 |
| [custom-code.md](custom-code.md) | header, footer, passes code, `[expr]`, immediate characters | M1 |
| [jog.md](jog.md) | step jog, jog to position, continuous jog | M1 |
| [overrides.md](overrides.md) | feed, rapid and power overrides | M1 |
| [machine-settings.md](machine-settings.md) | the board's `$$` table, writing settings, Wi-Fi | M1 |
| [firmware-variants.md](firmware-variants.md) | Grbl, grblHAL, Smoothie, Marlin, VigoWork, vendor detection | M1 |
| [gcode-analysis.md](gcode-analysis.md) | parsing, time estimate, ranges, quadrant, `analyze --json` | M1 |
| [laser-life.md](laser-life.md) | laser module life counters | M1 |
| [emulator.md](emulator.md) | the built-in Grbl 1.1 emulator, on a PTY | M1 |
| [settings.md](settings.md) | `settings.toml`, schema, migrations, `freekerf config` | M1 |
| [cli.md](cli.md) | the `freekerf` command line | M1 |
| [host-protocol.md](host-protocol.md) | `freekerf host --stdio` (JSON Lines) | M1 |
| [actions.md](actions.md) | the action registry (generated) | M1 |
| [shell.md](shell.md) | app shell: view state, layouts, STOP, dialogs, keyboard, dispatch-only | M4a |
| [fire-confirmation.md](fire-confirmation.md) | the human confirmation before any laser action | M4a |
| [produce-mode.md](produce-mode.md) | machine panel, jog, frame, overrides, console, recovery | M4a |
| [i18n.md](i18n.md) | Fluent translations, language setting, LaserGRBL translations (`resx2ftl`) | M4a |
| [theming.md](theming.md) | themes, high contrast, density, units | M4a |
| [canvas.md](canvas.md) | the canvas: renderer, gestures, G-code preview, head, performance | M4a |
| [desktop-app.md](desktop-app.md) | the Tauri app, its boundary with the service, generated types, design system | M4a |
