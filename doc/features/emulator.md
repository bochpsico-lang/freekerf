# Emulator

## Purpose

A Grbl 1.1 board in software: the "simulator mode" of the app and the fake device
of the tests.

## Behaviour

- Real-time bytes (`?`, `!`, `~`, ctrl-x, ≥ 0x80) act at once; other bytes enter a
  128-byte RX buffer (configurable), with overflow counting.
- `$$` (34 settings of a 400×300 laser), `$I` (`[VER:1.1f..]`, `[OPT:V,15,128]`),
  `$X`, `$H` (needs `$22=1`), `$C`, `$G`, `$N=V`, `$J=`, alarms, feed hold/resume,
  soft reset, overrides, status reports with `MPos`, `Bf`, `FS`, `WCO`, `Ov`.
- Motions take their estimated time in a 15-block planner (simulator), or are
  instant (tests, `emulate --instant`).
- Test controls: manual acknowledge (`release`), error rules (regex → `error:N`),
  `ok` before data, unsolicited lines, alarms.

## Use

- In process: `freekerf send FILE --emulator`, `freekerf console --emulator`,
  action `machine.connect {"emulator": true}`.
- On a PTY: `freekerf emulate --pty` prints `/dev/pts/N`; any program (FreeKerf,
  LaserGRBL on Mono, a terminal) can open it as a serial port. The banner is
  printed when the first byte arrives.

## Tests

`crates/core/freekerf-emulator` unit tests, `freekerf-transport` tests, every
protocol test (it is the device of `Bench`).

## Divergences

DIV-023, DIV-025.
