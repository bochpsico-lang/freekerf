# Divergences from LaserGRBL

FreeKerf reproduces the observable behaviour of LaserGRBL's core as pinned by
[`freekerf/conformance`](https://github.com/freekerf/conformance), except where
this page says otherwise. Every finding of `conformance/FINDINGS.md` has an entry
with the same number (`F-12` → `DIV-012`); FreeKerf-only differences start at
`DIV-101`. Code comments cite these ids.

- **kept**: FreeKerf does what LaserGRBL does (usually because the conformance
  goldens or the wire traffic depend on it).
- **fixed**: FreeKerf behaves differently, on purpose.
- **not applicable**: the code involved does not exist in FreeKerf (yet).

When a conformance test fails because of a *fixed* entry, it is marked
`@pytest.mark.rust_divergence("DIV-NNN")` in conformance (strict xfail for the
Rust host only). Today no protocol or golden (G-code) test needs the marker: the
fixed entries are outside what those layers observe.

## Protocol and streaming

| Id | LaserGRBL | FreeKerf | Status | Tests |
| --- | --- | --- | --- | --- |
| DIV-008 | Vigo, outside a job: commands marked `ok` at once but left pending, bytes not counted | Same | kept (no board to test a change on) | `host_behaviour.rs::vigo_wraps_jobs_and_uses_sbuf_counters` |
| DIV-015 | A `\|` status report with `Pin:` and an unknown version is read with the 0.9 parser and dropped | Any report with `\|` is read with the 1.1 parser | fixed | `host_behaviour.rs::legacy_status_reports_without_version` |
| DIV-016 | A successful `$N=V` updates a throw-away configuration when none was read yet | The settings table is always updated, and used (a table read for no version takes the board's) | fixed | `protocol.rs::write_settings_sends_one_dollar_line_per_setting`, `host_behaviour.rs::settings_written_before_any_read_are_used`, conformance `test_jog.py::test_settings_written_before_any_read_are_forgotten` |
| DIV-017 | Comment-only lines are kept as empty commands and streamed as empty lines | Same | kept (goldens `stream`) | golden `comments_spacing`, `host_behaviour.rs::comment_lines_are_streamed_as_empty_lines_and_header_footer_wrap_the_job` |
| DIV-019 | Resume queues an empty command when no modal group is settled | The empty command is not queued | fixed | `host_behaviour.rs::resume_adds_the_motion_mode_homing_and_work_offset` |
| DIV-020 | Safety door sends `@` (Grbl 0.9) | Sends 0x84 (Grbl 1.1) | fixed | `host_behaviour.rs::state_rules_of_immediate_commands` |
| DIV-021 | Jog feed formatted with the current culture (`F1234,5`) | Always invariant | fixed | `jog.rs` unit tests |
| DIV-022 | Jog step rounded to one decimal (0.25 → 0.3, 0.04 → 0.0) | One to three decimals (0.25, 0.04, 10.0) | fixed | `jog.rs::steps_and_speeds`, `host_behaviour.rs::jog_variants_v11` |
| DIV-024 | `WriteConfig`/`RefreshConfig` silently do nothing unless Idle/Alarm | The action fails with "settings can be written only in Idle or Alarm" | fixed | `host_behaviour.rs::manual_commands_are_refused_during_a_job` |
| DIV-028 | `$$`/`$I` without answer in 10 s: the query stays pending and the next `ok` shifts every later response | The query fails after 10 s but stays pending, so a late answer acknowledges the query itself | fixed | `host_behaviour.rs::settings_refresh_and_timeouts` |
| DIV-029 | `WriteConfig` busy-waits with no timeout | Writes are a non-blocking operation; the service waits for its end with a 30 s timeout | fixed | `service.rs::manual_commands_settings_and_jog` |
| DIV-030 | A job started during the connect-time `$$`/`$I` is lost (queue pointers swapped) and stays "in program" forever | Starting a job is refused (`busy`) until those queries end; manual commands wait in the queue | fixed | `host_behaviour.rs::run_program_preconditions` |

## G-code parsing and analysis

| Id | LaserGRBL | FreeKerf | Status | Tests |
| --- | --- | --- | --- | --- |
| DIV-001 | Parsing stops silently at a malformed number or a repeated letter (`G90 G1 X3`); the line is streamed unchanged | Same; `Command::has_parse_error` exposes it for a future warning | kept (goldens) | golden `malformed`, `command.rs::malformed_number_stops_parsing_and_keeps_text` |
| DIV-002 | `Clone()` shares the result holder between the file and the queue | Commands are values | not applicable | — |
| DIV-004 | Removing an inline comment keeps the spaces on both sides | Same | kept (goldens `commands`) | golden `comments_spacing` |
| DIV-012 | `G2/G3 R`: center at the chord midpoint | Same | kept (goldens `arcs`) | golden `arcs`, `arc.rs::r_arcs_use_the_chord_midpoint` |
| DIV-018 | G20 never tracked: inch programs analyzed as mm | Same | kept (golden `inches`) | golden `inches` |
| — | Time estimates rounded to whole ms per command (`TimeSpan.From*`) | Same (`number::millis_from`) | kept | every golden `offsets_s` |

## Configuration, versions, settings

| Id | LaserGRBL | FreeKerf | Status | Tests |
| --- | --- | --- | --- | --- |
| DIV-003 | Unknown `ALARM:N` decodes to null | The text is kept | fixed | `console.rs::decoding` |
| DIV-005 | `VendorName` compares a lower-cased string with `"Vigotec"`: never true | Vigotec and Grbl-Vigo boards report "Vigotec" | fixed | `version.rs::vendor_flags` |
| DIV-009 | `IsLuckyWiFi` relies on `&&`/`\|\|` precedence | Same expression | kept | `version.rs::vendor_flags` |
| DIV-010 | `OnFileLoading` raised only with `OnFileLoaded` subscribers | No file events; `job.load` returns the summary and emits `ProgramLoaded` | not applicable | — |
| DIV-011 | `.lps` projects use a `\` separator | No project files yet (M2) | not applicable | — |
| DIV-013 | `$$` value fallback parsed with the current culture (`500.5` → 5005) | Always invariant | fixed | `grbl_settings.rs::unparseable_values_fall_back` |
| DIV-014 | End of a pass does not set the global end: negative global time between passes | The global end is set at every pass | fixed | `projection.rs::passes_keep_the_global_start` |
| — | Settings persisted with `BinaryFormatter` | `settings.toml` (ADR 0005); importing LaserGRBL settings is M2 (`freekerf-migrate`) | different format | `freekerf-settings` tests |

## Laser life counters

| Id | LaserGRBL | FreeKerf | Status | Tests |
| --- | --- | --- | --- | --- |
| DIV-007 | `OnConnect` with no counters falls through to the selector dialog | No counters: nothing is tracked | fixed | `life.rs::selection_on_connect` |
| DIV-026 | Run time measured from the last *power* sample | Measured from the last *status* sample | fixed | `life.rs::run_time_only_in_run_and_plausible_deltas` |
| DIV-027 | Statistics uploaded (invalid JSON) | No upload at all (no telemetry) | not applicable | — |

## Emulator

| Id | LaserGRBL | FreeKerf | Status | Tests |
| --- | --- | --- | --- | --- |
| DIV-023 | The emulator wrapper discards the banner printed on open | The banner reaches the host | fixed | `freekerf-transport` emulator tests |
| DIV-025 | An empty line follows every status report | No empty line | fixed | `freekerf-emulator` tests |
| — | Emulator executes lines with sleeps on its own threads | Sans-IO device with a 15-block planner; RX buffer accounting, error rules and manual acknowledge for tests | different design | `freekerf-emulator` tests |

## Importers (M2)

DIV-031 to DIV-050 (SVG, DXF, Hershey, raster, Potrace, Centerline findings) are
**not applicable** until the importers exist (milestone M2); each will get its
entry then. The conformance importer goldens are skipped for the Rust host.

## FreeKerf only

| Id | Difference | Why | Tests |
| --- | --- | --- | --- |
| DIV-101 | `[WPos.X]`, `[WPos.Y]`, `[WPos.Z]` in custom code are the work position; LaserGRBL gives the machine position | The names say work position | `host_behaviour.rs::work_position_variables_are_the_work_position` |
| DIV-102 | STOP (soft reset) works while connecting; LaserGRBL needs the first status report | STOP must always work (PLAN §11) | `host_behaviour.rs::soft_reset_works_while_connecting` |
| DIV-103 | Requests that LaserGRBL ignores when not allowed (unlock while running, hold while idle...) return an error with the reason | Front ends and agents need to know | `host_behaviour.rs::state_rules_of_immediate_commands` |
| DIV-104 | No sounds, Telegram notifications, hotkeys or dialogs in the core | UI concerns (M4); the core emits events | — |
| DIV-105 | No "safety countdown" before a job | UI concern: the fire confirmation of the desktop app (M4) | — |
