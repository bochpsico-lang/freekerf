# Firmware variants

## Purpose

Talk to the firmware families LaserGRBL supports, selected with `machine.firmware`.

| Behaviour | Grbl / grblHAL | Smoothie | Marlin | VigoWork |
| --- | --- | --- | --- | --- |
| streaming | setting | synchronous | synchronous | setting |
| soft reset on connect | setting (ctrl-x) | no | setting (none sent) | setting |
| first bytes | `?` | newline, `?` | `M114` | 0x88 |
| status | `<...>` reports | `<...>` (`F:feed,power`) | `X:.. Y:.. Z:..` answers to `M114` (not while running) | `<VSta:..\|SBuf:..>` |
| STOP | ctrl-x | `reset` line | none | ctrl-x |
| `$$`/`$I`, unlock | yes | unlock no | no | yes |
| jog | `$J=` from 1.1 | `G1` | `G1` | `$J=` from 1.1 |
| job wrapper | — | — | Idle forced at the end | vendor start/end lines, `G0X0Y0M5` |

Grbl banners recognized (vendor data kept in the version): `Grbl 1.1f`,
`GrblHAL 1.1f`, `Grbl-Vigo:1.1f|Build:...`, `SimpleLaser 1.1f`, Ortur/Aufero model
lines and `OLF` firmware, Longer `[Machine:..]`/`[Software:..]`, and any
`Name 1.2x` banner before the first one.

## Extending

A new firmware is a new `impl Firmware` (defaults are Grbl's) and a `FirmwareKind`;
see [architecture.md](../architecture.md#extension-points).

## Tests

`host_behaviour.rs::firmware_banners_and_vendors`, `smoothie_*`, `marlin_*`,
`vigo_*`, `firmware.rs` unit tests.

## Divergences

DIV-005, DIV-008, DIV-009.
