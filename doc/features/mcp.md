# MCP: AI agents

FreeKerf can be driven by AI agents through the
[Model Context Protocol](https://modelcontextprotocol.io). **It is disabled by
default.** An agent gets a small fixed set of tools; the main one runs JavaScript
against the same `app` API as user scripts ([scripting.md](scripting.md)), so new
actions never add tools (PLAN §9, [ADR 0012](../adr/0012-mcp-transport-and-security.md)).

## Setup

1. Enable it (only the user can; scripts and agents cannot):

   ```bash
   freekerf config set automation.mcp.enabled true
   ```

2. Configure the agent to run `freekerf mcp` as a stdio MCP server. For example, for
   Claude Code:

   ```bash
   claude mcp add freekerf -- freekerf mcp
   ```

   Other agents take the same command (`"command": "freekerf", "args": ["mcp"]`).

`freekerf mcp` **attaches** to the running FreeKerf, or **starts** a headless one
(`freekerf serve --headless`, which exits after 60 s without clients; `--idle-exit`)
and attaches to it. While MCP is disabled it exits at once with a message saying how
to enable it, and a running instance refuses agents.

## Tools

| Tool | Tier | What it does |
| --- | --- | --- |
| `run_script(code, dry_run?)` | each call inside | runs JavaScript: value, log, dispatched actions, error; one undo step, rolled back on error; `dry_run` applies nothing |
| `get_state(path?)` | read | the semantic snapshot: session (view, selection, document, history), machine, console; `path` selects a part (`session.view.mode`) |
| `screenshot()` | read | the app window (desktop app); headless answers with an error |
| `search_api(query, limit?)` | read | actions matching every word, with their script signature, tier and parameters |
| `stop()` | always | cancels running scripts and soft-resets the machine; answered even while a script runs |

The app's refusals (permission denied, laser refused, a failing script) come back as
tool results with `isError: true`, readable by the agent.

## Resources

`freekerf://state/view` (mode, panels, dialogs, viewport, selection),
`freekerf://state/document` (layers and objects without geometry, undo/redo, unsaved
changes), `freekerf://state/machine`, `freekerf://state/console` and
`freekerf://api.d.ts`. All but the declarations can be subscribed to:
`notifications/resources/updated` follows every change (machine and console at most
every 500 ms).

## Tiers and safety

| Tier | Agents |
| --- | --- |
| `read`, `ui`, `edit` | allowed when MCP is enabled |
| `machine` | with `automation.mcp.allow_machine` (off), or a grant |
| `laser` | requested only with `automation.mcp.allow_laser` (off), and **always** confirmed by a person on the machine's screen; refused headless |

- Tiers are checked on every action, including each call inside a script.
- Above its baseline a call asks the person in front of the app, who can grant it
  for this run, this session or always (`automation.grants`). Headless, nobody is
  asked: only the baseline and "always" grants apply.
- `automation.*` settings and grants cannot be changed by scripts or agents.
- STOP is always allowed and cancels scripts first.

## Tests

`freekerf-mcp/tests/mcp.rs` (the tool set, `run_script` reaching every namespace with
the machine on the built-in emulator, resources and notifications, tiers, laser
refused, STOP during a script, disabled app, attach or launch),
`freekerf-service/tests/remote.rs` and `permissions.rs`,
`apps/cli/tests/cli.rs::mcp_is_off_by_default_then_attaches_or_launches`.
