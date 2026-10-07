# Jog

## Purpose

Move the head by hand, with the laser off.

## Behaviour

| Call | Grbl 1.1 | Grbl 0.9 / Smoothie / Marlin |
| --- | --- | --- |
| step (`machine.jog`, direction + step) | `$J=G91X2.5Y2.5F800` | `G91`, `G1X2.5Y2.5F800`, `G90` |
| home direction | `$J=G90X0Y0F800` | `G90`, `G1X0Y0F800` |
| to a position (`machine.jog_to`) | `$J=G90X10.00Y20.50F600` | `G90`, `G1X10.00Y20.50F600` |

- Directions: `N S E W NE NW SE SW Zup Zdown Home`.
- Steps print one to three decimals (`2.5`, `10.0`, `0.25`; DIV-022), feeds are
  invariant (DIV-021).
- Jog is allowed when connected and Idle or Jog (1.1), or Idle/Run outside a job (0.9).
- `jog_to` is clamped to the table (`$130`, `$131`) when soft limits (`$20`) are on.
- **Continuous jog** (`machine.continuous_jog = true`, Grbl 1.1): a direction jogs
  towards the table edge (`$J=G53X400.0Y300.0F1000`) until released
  (`machine.jog_stop`, which sends the jog cancel 0x85). A new direction cancels the
  previous one first. Z always jogs by steps.

## Configuration

`jog.step_mm`, `jog.speed_mm_min` (defaults for the actions), `machine.continuous_jog`.

## Safety

Jog never turns the laser on (tier `machine`). Continuous jog by touch needs a
heartbeat (dead-man switch) in the remote client (M6).

## Tests

`protocol.rs::jog_uses_dollar_j_relative`, `host_behaviour.rs::jog_*`,
`continuous_jog_sends_targets_and_aborts`, `jog.rs` unit tests.

## Divergences

DIV-021, DIV-022.
