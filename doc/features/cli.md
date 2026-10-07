# Command line (`freekerf`)

The headless front end. Every command goes through the action registry of
`freekerf-service`. Global options: `--home DIR` (settings and data directory),
`--log LEVEL` (diagnostics on stderr).

| Command | What it does | Exit code |
| --- | --- | --- |
| `freekerf analyze FILE [--json] [--max-rate N]` | summary, or the JSON report of the conformance goldens ([gcode-analysis.md](gcode-analysis.md)); `--max-rate` is `$110` (default 4000) | 0, 2 on error |
| `freekerf send FILE (--port P [--baud B] \| --emulator) [--passes N] [--homing]` | connects, runs the job, waits for the end; progress on stderr; Ctrl-C sends STOP | 0, 1 when the board reported errors or an issue happened, 2 on failure |
| `freekerf console (--port P \| --emulator)` | interactive: typed lines are sent; `:stop :hold :resume :unlock :home :state :jog DIR [STEP] :run ACTION [JSON] :quit` | 0 |
| `freekerf emulate --pty [--instant]` | Grbl 1.1 emulator on a PTY; prints the path; runs until stdin closes | 0 |
| `freekerf host --stdio [--persistent]` | JSON Lines access to every action ([host-protocol.md](host-protocol.md)) | 0 |
| `freekerf config path\|get\|set\|schema` | settings file ([settings.md](settings.md)) | 0, 2 |
| `freekerf actions [--json\|--markdown]` | the action catalogue | 0 |
| `freekerf run ACTION [JSON]` | runs one action, prints its JSON result | 0, 2 |
| `freekerf import-case CASE [--inputs DIR]` / `--kinds` | runs a `conformance` importer golden case (`-` reads it from stdin) and prints the harness's JSON; `--kinds` lists the case kinds this build runs | 0, 2 |

Examples:

```bash
freekerf analyze job.nc
freekerf send job.nc --port /dev/ttyUSB0 --passes 2
freekerf send job.nc --emulator
freekerf run state.get
PTY=$(freekerf emulate --pty | head -1) &   # then open $PTY in any program
```

## Safety

`send` and `console` install a Ctrl-C handler that sends STOP (soft reset) before
leaving, so a job is never left running unattended.

## Tests

`apps/cli/tests/cli.rs` (every command, including `host --stdio` talking to
`emulate --pty` over a real PTY).
