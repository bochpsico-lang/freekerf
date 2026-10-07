# Overrides

## Purpose

Change feed, rapid and power while a job runs (Grbl 1.1 real-time overrides).

## Behaviour

`machine.overrides {feed, rapid, power}` sets **targets** (%). At each status
report with `Ov:` the host sends at most one real-time byte per override towards
its target (coarse ±10 %, then fine ±1 %, reset when the target is 100 %;
rapids: 100/50/25 %). The current values come from the board (`state.get`:
`machine.overrides`); a reset or a new banner sets everything back to 100.

| Byte | Meaning |
| --- | --- |
| 0x90 / 0x91 / 0x92 / 0x93 / 0x94 | feed 100 % / +10 / −10 / +1 / −1 |
| 0x95 / 0x96 / 0x97 | rapids 100 / 50 / 25 % |
| 0x99 / 0x9A / 0x9B / 0x9C / 0x9D | power 100 % / +10 / −10 / +1 / −1 |

## Tests

`protocol.rs::overrides_converge_to_targets`, service `manual_commands_settings_and_jog`.
