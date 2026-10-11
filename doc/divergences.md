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
Rust host only; a golden case carries the mark in the first line of its `.nc`).
Marked today: DIV-015, DIV-016, DIV-019, DIV-020, DIV-022, DIV-024, DIV-030,
DIV-101 (protocol tests), DIV-052, DIV-053 (golden cases) and DIV-031, DIV-032,
DIV-034, DIV-036 (importer cases, field `"rust_divergence"` of the case JSON). The differential layer
of conformance (`make test-diff`) removes these differences with one explicit rule
per id. Which white-box behaviour is compared where: conformance's
`doc/traceability.md`.

## Protocol and streaming

| Id | LaserGRBL | FreeKerf | Status | Tests |
| --- | --- | --- | --- | --- |
| DIV-008 | Vigo, outside a job: commands marked `ok` at once but left pending, bytes not counted | Same | kept (no board to test a change on) | `host_behaviour.rs::vigo_wraps_jobs_and_uses_sbuf_counters` |
| DIV-015 | A `\|` status report with `Pin:` and an unknown version is read with the 0.9 parser and dropped | Any report with `\|` is read with the 1.1 parser | fixed | `host_behaviour.rs::legacy_status_reports_without_version` |
| DIV-016 | A successful `$N=V` updates a throw-away configuration when none was read yet | The settings table is always updated, and used (a table read for no version takes the board's) | fixed | `protocol.rs::write_settings_sends_one_dollar_line_per_setting`, `host_behaviour.rs::settings_written_before_any_read_are_used`, conformance `test_jog.py::test_settings_written_before_any_read_are_forgotten` |
| DIV-017 | Comment-only lines are kept as empty commands and streamed as empty lines | Same | kept (goldens `stream`) | golden `comments_spacing`, `host_behaviour.rs::comment_lines_are_streamed_as_empty_lines_and_header_footer_wrap_the_job` |
| DIV-019 | Resume queues an empty command when no modal group is settled | The empty command is not queued | fixed | `host_behaviour.rs::resume_adds_the_motion_mode_homing_and_work_offset` |
| DIV-020 | Safety door sends `@` (Grbl 0.9) | Sends 0x84 (Grbl 1.1) | fixed | `host_behaviour.rs::state_rules_of_immediate_commands`, `service.rs::hold_resume_stop_and_abort`, conformance `test_responses.py::test_safety_door_sends_the_legacy_at_sign` |
| DIV-021 | Jog feed formatted with the current culture (`F1234,5`) | Always invariant | fixed | `jog.rs` unit tests |
| DIV-022 | Jog step rounded to one decimal (0.25 → 0.3, 0.04 → 0.0) | One to three decimals (0.25, 0.04, 10.0) | fixed | `jog.rs::steps_and_speeds`, `host_behaviour.rs::jog_variants_v11` |
| DIV-024 | `WriteConfig`/`RefreshConfig` silently do nothing unless Idle/Alarm | The action fails with "settings can be written only in Idle or Alarm" | fixed | `host_behaviour.rs::manual_commands_are_refused_during_a_job` |
| DIV-028 | `$$`/`$I` without answer in 10 s: the query stays pending and the next `ok` shifts every later response | The query fails after 10 s but stays pending, so a late answer acknowledges the query itself | fixed | `host_behaviour.rs::settings_refresh_and_timeouts` |
| DIV-029 | `WriteConfig` busy-waits with no timeout | Writes are a non-blocking operation; the service waits for its end with a 30 s timeout | fixed | `service.rs::manual_commands_settings_and_jog` |
| DIV-030 | A job started during the connect-time `$$`/`$I` goes to a private queue: its sent lines count as replies to `$$` (the settings read ends early), the lines not sent when the queues are restored are dropped, and the job ends as if complete, footer included (or stays "in program" forever) | Starting a job is refused (`busy`) until those queries end; manual commands wait in the queue | fixed | `host_behaviour.rs::run_program_preconditions`, conformance `test_jobs.py::test_job_started_during_the_connect_time_settings_read_is_cut_short` |
| DIV-051 | Any received line that contains `ok` in any case and is not a status report or a banner acknowledges the oldest pending line ("broken ok", a workaround for noise: LaserGRBL discussion #1498), including a startup block report `>G54G20:ok` (which the Grbl v1.1 interface says is not counted) and a well-formed message such as `[MSG:Look up]` | Same for garbled lines (`<Idle\|...ok`), but never a line starting with `>` or a well-formed `[...]` message; setting `machine.accept_broken_ok` (default on) turns the rule off, leaving only lines that start with `ok`/`error` | fixed in part (product decision: keep the noise tolerance, drop the false acknowledgements, let the user turn it off) | `host_behaviour.rs::broken_oks_skip_startup_reports_and_messages`, `broken_oks_can_be_turned_off`, conformance `test_responses.py::test_any_message_containing_ok_acknowledges_a_line`, `test_a_startup_line_report_acknowledges_a_line`, `test_an_ok_merged_into_a_status_report_acknowledges_a_line` |
| DIV-054 | Restarting an interrupted job from the start (resume dialog, position 0) skips the custom header (`RunProgramFromStart` with `first = false`) | `job.run` always queues the header; continuing is `job.resume` | fixed | conformance `test_core_gaps.py::test_restart_from_the_start_after_an_interruption_skips_the_header` (LaserGRBL only: a dialog path) |

## G-code parsing and analysis

| Id | LaserGRBL | FreeKerf | Status | Tests |
| --- | --- | --- | --- | --- |
| DIV-001 | Parsing stops silently at a malformed number or a repeated letter (`G90 G1 X3`); the line is streamed unchanged | Same; `Command::has_parse_error` exposes it for a future warning | kept (goldens) | golden `malformed`, `command.rs::malformed_number_stops_parsing_and_keeps_text` |
| DIV-002 | `Clone()` shares the result holder between the file and the queue | Commands are values | not applicable | — |
| DIV-004 | Removing an inline comment keeps the spaces on both sides | Same | kept (goldens `commands`) | golden `comments_spacing` |
| DIV-012 | `G2/G3 R`: center at the chord midpoint | Same | kept (goldens `arcs`) | golden `arcs`, `arc.rs::r_arcs_use_the_chord_midpoint` |
| DIV-018 | G20 never tracked: inch programs analyzed as mm | Same | kept (golden `inches`) | golden `inches` |
| DIV-052 | Lines are upper-cased with the current culture: on Turkish Windows `i` becomes `İ` (an arc's `I` is lost and `İ` is sent to the board); on en-US `ı` becomes `I` | Unicode default upper-casing, the same on every locale (`i` → `I`; the conformance harness runs LaserGRBL with the invariant culture, where `ı` stays) | fixed | golden `casing` (conformance), `command.rs::normalizes_spaces_and_comments` |
| DIV-053 | The first exception while analyzing a command (e.g. the time of an absurd move such as `X99999999999999999999` overflowing `TimeSpan`) stops the analysis: later commands get no time offset and do not extend the ranges | That move's time counts as 0 ms and the analysis goes on | fixed | golden `analysis_overflow` (conformance), `number.rs::millis_round_like_timespan` |
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

## Importers and generators

The conformance importer goldens run for the Rust host by case kind (`freekerf
import-case --kinds`); kinds not implemented yet are skipped.

| Id | LaserGRBL | FreeKerf | Status | Tests |
| --- | --- | --- | --- | --- |
| DIV-031 | SVG path numbers such as `-4.5.5` (`-4.5` then `.5`) read as one number: the command loses its arguments and the whole import fails | Read as two numbers; incomplete argument groups are dropped instead of failing | fixed | `convert.rs::numbers_and_tokens`, conformance `svg_compact_numbers` (marked) |
| DIV-032 | A `path` without `d` throws and loses the whole import | The element is skipped | fixed | conformance `svg_path_without_d` (marked) |
| DIV-033 | A failed import clears the loaded file first, and reports nothing | An import returns an error; the document is untouched | fixed | `freekerf-import` error tests |
| DIV-034 | `polyline`/`polygon`: point tokens of 3 characters or less (`9,9`) are skipped; points without commas give only a comment | Every pair is read (white space and/or commas, as SVG says) | fixed | conformance `svg_basic_shapes`, `svg_disable_g0`, `svg_no_pwm`, `svg_via_source` (marked); `*_portable` twins agree on both hosts |
| DIV-035 | An SVG arc with a zero radius draws nothing (a gap) | Same | kept (rare; the goldens `svg_paths*` depend on it) | conformance `svg_paths`, `svg_smoothie` |
| DIV-036 | `skewX`/`skewY` ignored; a transform without its `)` loses its last character | Both applied as SVG says | fixed | `convert.rs::transforms_as_laser_grbl_reads_them`, conformance `svg_transforms` (marked), `svg_transforms_portable` agrees |
| DIV-037 | A relative file name starting with `LaserGRBL.` is read as an embedded resource | Importers read bytes; there are no resource names | not applicable | — |
| DIV-038 | The SVG/DXF writer reads the firmware type once per process | Read for every job from the options | fixed | `writer.rs` tests |
| DIV-039 | Cutting test label for a pass range reads `1 - F2 pass` | Same | kept (golden `gen_cutting_grid`; a label, no effect on the job) | conformance `gen_cutting_grid` |
| DIV-040 | Hershey coordinates written with `double.ToString()`: 15 significant digits, floating point noise included (`Y0.00299999999999989`) | Same (`freekerf_gcode::dotnet`) | kept (goldens; harmless for the machine) | conformance `hershey_*`, `gen_*` |
| DIV-041 | Error diffusion never reaches row 0 or column 0 (a uniform mid-grey first row comes out all black); the green and blue errors are swapped | Same (`freekerf_raster::dither`) | kept (goldens `img_dither_*`; one row and column, no effect on grey images) | conformance `img_dither_*` |
| DIV-042 | Random dithering is seeded with the clock: the same image engraves differently each time | Seeded by the caller: the same image and seed give the same result | fixed | `dither.rs::every_mode_gives_black_and_white` |
| DIV-045 | Vectorize options unchecked in the dialog are not "off": spot removal keeps turdsize 2, optimization keeps tolerance 0.2 | Same (`TraceOptions::default`) | kept (goldens `potrace_*`) | conformance `potrace_*` |
| DIV-046 | Vectorize with raster filling: the filling header follows the raster PWM option, the outlines the machine's "Support Hardware PWM" | Same | kept (golden `potrace_raster_fill_no_pwm`) | conformance `potrace_raster_fill_no_pwm` |
| DIV-047 | The raster filling sets `vectorfilling` on the caller's `L2LConf` | Settings are passed by value; nothing of the caller's changes | not applicable | — |
| DIV-048 | A failing or missing autotrace is only logged; the empty output then fails as "Root element is missing" | The tracer's error is returned with its cause; empty output is an error too | fixed | `autotrace.rs::centerline_with_a_double` |
| DIV-049 | Centerline reads speed, power, laser commands and offset from the raster dialog settings, not from the `L2LConf` it receives | Every value is an explicit parameter of `freekerf_planner::image::centerline` (conformance maps the settings) | not applicable | conformance `centerline_settings` |
| DIV-050 | "Is grey scale" samples one pixel in ten in each direction (tolerance 20): colour details off that grid count as grey, and then the formula and the RGB weights are ignored | Same (`processor::is_gray`) | kept (golden `ip_gray_image_ignores_formula`) | conformance `ip_gray_image_ignores_formula` |
| DIV-043 | A malformed saved layer setting is half loaded | Layer settings are document data, validated when read | not applicable | — |
| DIV-044 | Every layer pass starts with a comment-only line (an empty command); pen up/down lines keep a space before their comment (`S0 `) | Same | kept (every `svg_*`/`dxf_*` golden) | conformance `svg_*`, `dxf_*` |


## FreeKerf only

| Id | Difference | Why | Tests |
| --- | --- | --- | --- |
| DIV-101 | `[WPos.X]`, `[WPos.Y]`, `[WPos.Z]` in custom code are the work position; LaserGRBL gives the machine position | The names say work position | `host_behaviour.rs::work_position_variables_are_the_work_position` |
| DIV-102 | STOP (soft reset) works while connecting; LaserGRBL needs the first status report | STOP must always work (PLAN §11) | `host_behaviour.rs::soft_reset_works_while_connecting` |
| DIV-103 | Requests that LaserGRBL ignores when not allowed (unlock while running, hold while idle...) return an error with the reason | Front ends and agents need to know | `host_behaviour.rs::state_rules_of_immediate_commands` |
| DIV-104 | No sounds, Telegram notifications, hotkeys or dialogs in the core | UI concerns (M4); the core emits events | — |
| DIV-105 | No "safety countdown" before a job | UI concern: the fire confirmation of the desktop app (M4) | — |
| DIV-106 | No intermediate converter text: LaserGRBL's centerline conversion builds a G-code string with debugging comments (`G0X1Y1(M)`) before loading it; FreeKerf plans the job directly | The comments never reach the machine or a saved file. Conformance compares `svg_text` cases for the Rust host without the comments | conformance `svgtext_*` |
| DIV-107 | SVG path data that does not begin with a moveto, when it is the first element of its colour, starts at its first point; LaserGRBL draws a line from wherever the laser was | The previous position is arbitrary (another colour, another pass). After an element of the same colour both draw from its end | `convert.rs` tests, conformance `svg_fill_capture_edge` |
| DIV-108 | A nearest-neighbour resize copies the nearest pixel; libgdiplus (Mono's GDI+, where the goldens were recorded) ignores the interpolation mode and blends neighbours and the transparent border with Cairo's default filter | Windows GDI+ and the mode's definition do not blend (ADR 0008) | `ops.rs` tests, conformance `img_resize_nearest`, `img_resize_keep_alpha` (marked) |
| DIV-109 | Robustness against hostile or broken files, where LaserGRBL would hang or run out of memory: a DXF whose blocks expand to more than a million entities or two million segments is refused; Bézier curves with a non-finite control point, or one beyond 10^6 mm, are not subdivided, and a negative flattening tolerance (an SVG with a negative size) is taken as positive; fillings needing more than 20 million pattern elements are refused | Found by fuzzing (`doc/fuzzing.md`); no real drawing is affected: LaserGRBL takes the same paths and gives the same output on every conformance golden | `dxf::tests::blocks_inside_themselves_and_huge_grids`, `bezier::tests::non_finite_curves_are_not_subdivided`, `hatch::tests::huge_fillings_are_refused` |
