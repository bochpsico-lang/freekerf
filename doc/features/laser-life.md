# Laser life counters

## Purpose

Track how much each laser module is used: time in `Run`, emission time weighted by
power, and time per 10 % power class.

## Behaviour

- On connect the counter is chosen automatically when there is one counter or one
  alive (not dead) counter; with several alive modules the host emits
  `LaserSelectionNeeded` and waits for `machine.select_laser`. Without counters
  nothing is tracked (DIV-007).
- Every status sample adds the time since the previous status sample when the
  previous state was `Run` (DIV-026). Every `FS:` sample adds the elapsed time
  weighted by the previous power over `$30` (ignored when `$30` is not between 10
  and 100000); powers above 3 % count as emission, above 1 % in a class.
- Deltas of 600 s or more are ignored.
- Counters are saved in `machine-cache.json` after connections and jobs. Nothing is
  uploaded (no telemetry, DIV-027).

## Tests

`life.rs` unit tests, `host_behaviour.rs::laser_life_counters_follow_the_run_state`,
`several_laser_modules_ask_the_user`.
