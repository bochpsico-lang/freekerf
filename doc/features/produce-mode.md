# Produce mode

The Produce mode of the desktop app drives the machine: connect, jog, frame, run,
watch and recover a job (`design/ux.md`, flows 3 and 7). It uses only the M1
service. Code: `ui/web/src/produce/`.

## Machine panel

| Group | Controls | Actions |
| --- | --- | --- |
| Connection | port (serial ports found, or the built-in emulator), Connect / Disconnect, detected firmware and work area | `machine.list_ports`, `machine.connect {port \| emulator}`, `machine.disconnect`, `machine.info` |
| Job | Open G-code… (file picked in the window and sent as text), estimate, progress bar, elapsed / remaining / lines, **Start**, Pause ↔ Resume, Stop job, Frame | `job.load_text`, `job.run` (through the fire confirmation), `machine.feed_hold`, `machine.resume`, `job.abort`, `job.frame` |
| Job from the design (when the document has objects) | **Job start**: absolute (the document's coordinates), current position (the head's work position; needs a connection) or user origin (work 0,0), and the **3×3 anchor** (the point of the job's box that goes there; off when absolute); *Make the job from the design* plans the document and loads it (preview and estimate as for a file; "Making the job…" while it plans) | `job.from_document {start, anchor}` |
| Overrides | speed and power sliders (10–200 %), rapid moves 25/50/100 % | `machine.overrides` |
| Jog | DRO (head at 60 Hz), 3×3 pad with Home, step 0.1/1/10/50 mm, continuous, Home, Unlock, Origin here | `machine.jog`, `machine.jog_stop`, `machine.home`, `machine.unlock`, `machine.set_zero` |
| Custom buttons | `[[buttons]]` of `settings.toml` | `machine.custom_code` (through the fire confirmation) |
| Laser module | life counters of the modules, the one in use | `machine.info` |

- **The job and the design**: over the canvas, when the design has objects, a
  banner offers *Make from the design* when no job is loaded (placed by the
  panel's job start and anchor), or *Update from the design* when the job was
  made from an older design (placed as it was: the job records the document
  revision, start and anchor it was made from, `ProgramInfo.from_document`). A
  G-code file loaded as such gets no banner; neither does a running job or a board
  in alarm. Any change counts, an undo too.
- **Continuous jog is a dead-man control**: the head moves while the button is
  held; releasing it, leaving it, losing the pointer capture or the window focus
  sends `machine.jog_stop` (always allowed). Step mode moves one step per press.
- The work area of the canvas comes from the board (`$130`/`$131`) once its `$$`
  table is read; 400×400 mm before.
- **Frame** (`job.frame`, new in M4a, tier `machine`) jogs around the bounds of the
  loaded program with the laser off and ends at the first corner.
- Anything that can turn the laser on (Start, custom buttons, G-code typed in the
  console, job recovery) goes through the **fire confirmation**
  ([fire-confirmation.md](fire-confirmation.md)); the window itself cannot dispatch
  `laser` actions.

## Console

Sent lines (`›`), answers (`‹`), errors (`✕`) and host messages (`•`): the prefix,
not only the colour, tells them apart. `$` commands (`$$`, `$I`, `$H`, `$X`,
`$N=V`, `$J=…`) go to `machine.system_command` (new in M4a, tier `machine`: they
cannot turn the laser on); any other line is `machine.send` (tier `laser`, through
the fire confirmation). The last 400 rows are shown (`console.get`, then the
host's `Console` events).

## Interruptions and recovery

When the board is in alarm, or a job was interrupted (`state.get` → `resume`), a
banner over the canvas tells what happened and where, with **Unlock** and
**Recover job…**. The recovery dialog follows LaserGRBL's `ResumeJobForm`:

| Option | Position | Recommended when |
| --- | --- | --- |
| From the start | 0 | — |
| A few lines before the interruption | executed − 17 | alarm, reset, STOP (the board's buffer was lost) |
| From the last line sent | sent | connection lost (what was sent may have run) |
| From line N | N − 1 | — |

plus "home first" and "restore the work origin" (pre-filled from the suggestion).
Going on runs `job.resume` through the fire confirmation.

## Layouts

In the medium layout the Machine panel stays docked in Produce; in the narrow
layout Start, Pause and Frame move to the bottom bar next to STOP.

## Not yet

Dropping a file on the window is not wired yet (Tauri intercepts drops; listed in
`doc/roadmap.md`).

## Tests

`ui/web/src/produce/produce.test.ts` (open a file, the job from the design with
its start and anchor, the banner to make or update it, step jog, dead-man continuous
jog, home/unlock/origin/frame/overrides, Start and console G-code never dispatch a
laser action, alarm banner and recovery), `job.test.ts` (progress, recovery
options), `state/console.test.ts`; Rust:
`service.rs::produce_mode_helpers_frame_info_and_system_commands` (emulator),
`service.rs::document_session_actions_and_events` (job start and anchor, the
revision the job records),
`freekerf-planner/src/origin.rs` (anchors),
`freekerf-settings::custom_buttons`. End to end against the emulator: PR 9.
