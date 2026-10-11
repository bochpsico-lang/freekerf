# Fire confirmation

Nothing that can turn the laser on runs when it is asked for (PLAN §9.5,
`design/ux.md` "Segurança" 3). An action of tier `laser` (`job.run`, `job.resume`,
`machine.send`, `machine.custom_code`) becomes a **pending request**; a person
confirms it on screen after a countdown; only then does it run.

## How it works

1. A front end asks: `fire.request {action, params}` (tier `machine`). Only laser
   actions are accepted. The request (id, action, parameters, who asked, summary,
   countdown) goes into the view state (`fire_request`) and the fire confirmation
   dialog opens. A new request replaces the previous one.
2. The dialog shows the summary (program, lines, estimated time, size, or the line
   or custom code to send), the bench checklist (glasses right for this laser,
   exhaust and air on, material held and focused, extinguisher nearby: a reminder,
   not a lock) and **Fire laser**, disabled while the countdown runs ("Fire in 3 s").
3. Pressing **Fire laser** calls `confirm_fire(id)` (Tauri command →
   `Service::confirm_fire`). The service checks that `id` is the pending request
   and that the countdown is over **on its own clock**, then runs the action as the
   user. A request runs once.
4. Cancel, Esc, `fire.cancel` (tier `safety`) or closing the dialog (`view.close_dialog`,
   also from a script) drops the
   request. Clicking the backdrop does not close this dialog (no cancel by an
   accidental touch); STOP stays clickable over it.

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
- `freekerf-desktop` bridge: `every_laser_action_needs_the_confirmation`.
- `ui/web/src/produce/fire.test.ts`: no UI code dispatches a laser action (scan of
  the sources against the generated catalogue); the dialog fires only after the
  countdown through `confirmFire`; Cancel / Esc / backdrop never fire; STOP over it.
