# Local endpoint (IPC)

A running FreeKerf (`freekerf serve --headless` today, the desktop app later) listens
on a local endpoint for `freekerf mcp` and tests (`freekerf-ipc`,
`freekerf_service::remote`). One instance per user.

| | Linux / macOS | Windows |
| --- | --- | --- |
| endpoint | Unix socket `<runtime>/ipc/ipc.sock` in a `0700` directory (the socket itself `0600` on Linux) | named pipe `\\.\pipe\freekerf-<hash>`, owner and system only |
| token | `<runtime>/ipc/ipc.token`, `0600` | `<runtime>\ipc\ipc.token` |

`<runtime>` is `$XDG_RUNTIME_DIR/freekerf` on Linux, the data directory elsewhere, or
the `--home` / `$FREEKERF_HOME` directory. Each run writes a new random token; a
socket nobody accepts on is stale and replaced.

## Protocol (version 1)

JSON Lines, one object per line (at most 64 MiB).

```
→ {"hello": {"token": "…", "client": "agent", "name": "my-agent", "version": 1}}
← {"welcome": {"version": 1, "app_version": "…", "pid": 1234, "headless": true, "tier": "edit"}}
   or {"error": {"kind": "unauthorized" | "disabled" | "unsupported" | "protocol", "message": "…"}}
→ {"id": 1, "method": "dispatch", "params": {"action": "view.set_mode", "params": {"mode": "produce"}}}
← {"id": 1, "result": null}
   or {"id": 1, "error": {"kind": "permission_denied", "message": "…"}}
← {"event": {"event": "session", "revision": 7, "change": "view"}}      (after subscribe)
→ {"bye": true}                                                       (leaving)
```

| Method | Params | Result |
| --- | --- | --- |
| `ping` | | `"pong"` |
| `specs` | | every action spec (name, description, tier, parameter schema) |
| `api_dts` | | `doc/scripting/api.d.ts` |
| `search_api` | `{query, limit?}` | matching actions with their signatures |
| `get_state` | `{path?}` | `{session, machine, console, headless}` or the part at `path` |
| `dispatch` | `{action, params?}` | the action's result |
| `run_script` | `{code, dry_run?, name?, timeout_s?}` | the script report ([scripting.md](scripting.md)) |
| `stop` | | STOP: cancels scripts, soft-resets the machine |
| `subscribe` | | events from now on (`{"event": …}` lines, as `host --stdio` prints them) |
| `screenshot` | | error `unavailable` (desktop app only) |

Requests of one client run concurrently (answers may come out of order; match them
by `id`), so `stop` is answered while a script runs. Agents need
`automation.mcp.enabled`; the tier is the caller's baseline from the settings
([mcp.md](mcp.md)), never what the client says. Agents never run `laser` actions
and save only new `.fkproj` files.
