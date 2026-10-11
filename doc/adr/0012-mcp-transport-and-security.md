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
- **Local endpoint.** A Unix domain socket in a `0700` directory (`$XDG_RUNTIME_DIR/freekerf/ipc/`
  on Linux, where the socket is also `0600`), or a Windows named pipe whose DACL
  allows only its owner and the system. One instance per user: binding fails while
  another instance accepts; a socket nobody accepts is stale and replaced.
- **Token.** Each run of the app writes a random 256-bit token to a user-only file
  (atomically); every client presents it in its hello. It keeps out other users and
  processes that cannot read the user's files.
- **Identity and tiers.** A client declares a kind (`agent`, the only one today)
  and a name, but the tier comes from the **settings**, never from the client:
  `automation.mcp.enabled` (off by default: agents are refused at the hello and
  `freekerf mcp` exits at once) gives `edit`, `allow_machine` gives `machine`.
  Tiers are checked on every dispatch, including
  each call inside a script. Grants above the baseline need the person (this run,
  this session, always); agent names can be spoofed, so "always" grants name the
  risk in the dialog (M4a).
- **Laser.** Agents never run `laser` actions: no setting or grant allows them
  (2026-10-10; an `allow_laser` setting existed until then). STOP is always
  allowed, cancels running scripts first, and is answered while a script runs
  (requests run concurrently).
- **Files.** Agents read any file the user can (`document.import`, `document.open`)
  but write only new `.fkproj` projects: `document.save` to another extension or
  to an existing file is refused with the reason, in scripts and dry runs too.
- **Implementation.** MCP is written directly over `serde_json` (`freekerf-mcp`):
  newline-delimited JSON-RPC 2.0, protocol versions 2024-11-05 to 2025-11-25,
  tools and resources with subscriptions, synchronous threads like the rest of the
  app (ADR 0001). The official Rust SDK (`rmcp`) would bring `tokio` and an async
  stack for the five tools and five resources FreeKerf exposes.
- **Not now.** The optional Streamable HTTP transport (`127.0.0.1` only, token and
  `Origin` check), screenshots (desktop app) and MCP prompts are postponed
  (`doc/roadmap.md`).

## Consequences

- An agent can do anything a user can do in the UI within its tier, except turning
  the laser on and overwriting files; a person decides anything above the baseline.
- A process running as the same user can read the token and connect; it could also
  read every file of the user, so the endpoint adds no new exposure. A future
  external client (scripting from other languages) would use the same model; a
  Python client was written and removed in M2 for lack of a need.
- Following new MCP protocol versions is our job (small: the subset is stable);
  `freekerf-mcp` has no logic of its own to keep in sync with the app.
