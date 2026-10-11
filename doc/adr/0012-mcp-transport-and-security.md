# 0012. MCP: transport and security model

- Status: accepted (M2)
- Date: 2026-10-10

## Context

AI agents drive FreeKerf through MCP (PLAN §9). An agent is untrusted: it can be
wrong, prompt-injected, or impersonate another client. The machine has a laser.
The decisions to make: how `freekerf mcp` reaches the app, how the app knows who
connects and what it may do, and which MCP implementation to use.

## Decision

- **Topology.** `freekerf mcp` is a stdio MCP server the agent starts as a command.
  It holds no state: it proxies the fixed tools and resources to the running
  instance through the **local endpoint** (`freekerf-ipc`), attaching to the
  running instance or starting `freekerf serve --headless` (which exits when idle).
  Agents see the same session the user sees.
- **Local endpoint.** A Unix domain socket in a `0700` directory with mode `0600`
  (`$XDG_RUNTIME_DIR/freekerf/ipc/` on Linux), or a Windows named pipe whose DACL
  allows only its owner and the system. One instance per user: binding fails while
  another instance accepts; a socket nobody accepts is stale and replaced.
- **Token.** Each run of the app writes a random 256-bit token to a user-only file
  (atomically); every client presents it in its hello. It keeps out other users and
  processes that cannot read the user's files.
- **Identity and tiers.** A client declares a kind (`agent`, `python_client`) and a
  name, but the tier comes from the **settings**, never from the client:
  `automation.mcp.enabled` (off by default: agents are refused at the hello and
  `freekerf mcp` exits at once), `allow_machine`, `allow_laser`,
  `automation.python.allow_machine`. Tiers are checked on every dispatch, including
  each call inside a script. Grants above the baseline need the person (this run,
  this session, always); agent names can be spoofed, so "always" grants name the
  risk in the dialog (M4a).
- **Laser.** No grant or setting skips the person's confirmation of a `laser`
  action; a headless instance refuses them. STOP is always allowed, cancels running
  scripts first, and is answered while a script runs (requests run concurrently).
- **Implementation.** MCP is written directly over `serde_json` (`freekerf-mcp`):
  newline-delimited JSON-RPC 2.0, protocol versions 2024-11-05 to 2025-11-25,
  tools and resources with subscriptions, synchronous threads like the rest of the
  app (ADR 0001). The official Rust SDK (`rmcp`) would bring `tokio` and an async
  stack for the five tools and five resources FreeKerf exposes.
- **Not now.** The optional Streamable HTTP transport (`127.0.0.1` only, token and
  `Origin` check), screenshots (desktop app) and MCP prompts are postponed
  (`doc/roadmap.md`).

## Consequences

- An agent can do anything a user can do in the UI, within its tier; a person stays
  in the loop for every laser action and for anything above the baseline.
- A process running as the same user can read the token and connect; it could also
  read every file of the user, so the endpoint adds no new exposure. The Python
  client relies on the same model.
- Following new MCP protocol versions is our job (small: the subset is stable);
  `freekerf-mcp` has no logic of its own to keep in sync with the app.
