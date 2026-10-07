# Machine settings (`$$`)

## Purpose

Read and write the board's own settings (steps/mm, maximum rates, laser mode...).

## Behaviour

- Read with `$$` at connect time and with `machine.read_settings`. Lines `$N=V` are
  collected; descriptions some boards add (`$20=0 (soft limits, bool)`) are removed.
  An empty or rejected answer keeps the previous table.
- `machine.write_settings {"settings": {"110": "6000"}}` sends one `$N=V` per value,
  synchronously, and returns when the board answered all of them (or fails with
  the lines the board rejected). Accepted values update the table at once (DIV-016).
  Writing is refused while a job runs or the machine is not Idle/Alarm (DIV-024).
- Values used by FreeKerf: `$110` (time estimate), `$22` (homing allowed),
  `$20`, `$130`, `$131` (soft limits, table size for jog), `$30` (max power, life
  counters, `[expr]`), `$32` (laser mode, auto cooling).
- Ortur boards: `$33` is protected (`GrblSettings::validate`). Wi-Fi of Ortur
  (`$74`, `$75`, `$WRS`) and Longer (`$radio/...`) boards: `Host::write_wifi`.
- The last table and version are kept in `machine-cache.json` and used until the
  next connection.

## Tests

`grbl_settings.rs` unit tests, `host_behaviour.rs::settings_*`, `eeprom_writes_*`,
`wifi_configuration_by_vendor`, `protocol.rs::write_settings_sends_one_dollar_line_per_setting`.

## Divergences

DIV-013, DIV-016, DIV-024, DIV-028, DIV-029.
