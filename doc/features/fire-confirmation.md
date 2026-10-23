# Fire confirmation

Nothing that can turn the laser on runs when it is asked for (PLAN §9.5,
`design/ux.md` "Segurança" 3). An action of tier `laser` (`job.run`, `job.resume`,
`machine.send`, `machine.custom_code`) becomes a **pending request**; a person
confirms it on screen after a countdown; only then does it run.

## How it works

1. A front end asks: `fire.request {action, params}` (tier `machine`). Only laser
   actions are accepted. The request (id, action, parameters, who asked, summary,
   countdown) goes into the view state (`fire_request`), the view switches to the
   **Produce mode** (the request is always confirmed in front of the job's preview
   and the machine, also when a script asks from the Design mode) and the fire
   confirmation dialog opens. A new request replaces the previous one.
2. The dialog shows the summary (program, lines, estimated time, size, or the line
   or custom code to send), the bench checklist (glasses right for this laser,
   exhaust and air on, material held and focused, extinguisher nearby: a reminder,
   not a lock) and **Fire laser**, disabled while the countdown runs ("Fire in 3 s").
3. Pressing **Fire laser** calls `confirm_fire(id, accept_stale)` (Tauri command →
   `Service::confirm_fire`). The service checks that `id` is the pending request
   and that the countdown is over **on its own clock**, then runs the action as the
   user. A request runs once.
4. Cancel, Esc, `fire.cancel` (tier `safety`) or closing the dialog (`view.close_dialog`,
   also from a script) drops the
   request. Clicking the backdrop does not close this dialog (no cancel by an
   accidental touch); STOP stays clickable over it.

## Out-of-date jobs

A job records the document's revision when it was planned (`job.from_document`)
or loaded (a G-code file, `job.load` / `job.load_text`): `ProgramInfo.document_revision`.
When the design has objects and its revision is another one now, the job is **out
of date** and `fire.request` of `job.run` says where it came from in
`summary.stale`: `design` or `file` (a file loaded while the design was empty, or
never touched after, is never out of date).

- The dialog shows a warning (the design changed after the job was made; after
  *name* was loaded), **Update from the design** / **Make from the design** (drops
  the request and plans the design again, placed as the job was or, for a file,
  as the machine panel says: the person then sees the new preview and starts
  again), and a box **Burn this out-of-date job anyway** / **Burn *name* anyway**.
  Fire laser stays disabled until the box is ticked. Unlike the checklist, this is
  a lock.
- `confirm_fire` without `accept_stale` refuses such a request and keeps it
  pending. A job that went out of date after the request (the dialog did not say
  it) is dropped and refused: ask again.
- `job.resume` is not checked: it continues the job that was cut.

## Who can do what

| Caller | Dispatch a laser action | Ask (`fire.request`) | Confirm |
| --- | --- | --- | --- |
| desktop window (`max_tier: machine`) | refused | yes | yes, the Fire laser button (`confirm_fire`) |
| scripts | refused unless a person confirms (`Prompter::confirm_laser`; headless: refused) | yes (when they have `machine`) | **never**: confirming is not an action of the registry |
| agents (MCP) | **never** (no setting or grant) | **never** (refused, `check_agent_laser`) | never |
| CLI, `host --stdio` (the user, all tiers) | yes (headless, no screen) | — | — |

## Settings

`[safety] fire_countdown_s` (default 3, clamped to 1–10 s) is **user-only**: actions,
scripts and agents cannot change it (`freekerf config set` can).

## Tests

- `freekerf-session::fire` (countdown, only the shown request, once, cancel, limits)
  and `view` (`fire_requests_open_and_close_the_dialog`).
- `service.rs::laser_actions_run_only_after_a_confirmed_countdown` (emulator, simulated
  clock): every laser action refused for the window's caller; too early, wrong id
  and second confirmation refused; the job starts after the countdown; cancel and
  closing the dialog drop the request.
- `service.rs::a_job_is_fired_from_produce_and_an_out_of_date_one_only_when_accepted`:
  asking switches to Produce; a file and a design job going out of date; refused
  without `accept_stale` and kept pending; dropped when it went out of date after
  the request; the view locked in Produce while the job runs or is paused.
- `freekerf-desktop` bridge: `every_laser_action_needs_the_confirmation`.
- `ui/web/src/produce/fire.test.ts`: no UI code dispatches a laser action (scan of
  the sources against the generated catalogue); the dialog fires only after the
  countdown through `confirmFire`; Cancel / Esc / backdrop never fire; STOP over it;
  asked in Produce; an out-of-date job fires only once accepted, or is replaced by
  the job from the design.
