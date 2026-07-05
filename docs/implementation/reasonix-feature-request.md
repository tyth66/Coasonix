# Feature Request: Agent-to-Agent Collaboration Mode (Programmatic Session API)

## Motivation

Reasonix is a great coding agent for humans. But there is a growing need for
**agent-to-agent collaboration** — where one coding agent (e.g., Codex) delegates
a bounded expert task to Reasonix, Reasonix completes it, and the result flows
back for the orchestrator to consume.

Today, Reasonix has:

| Mode | Target | Can be called programmatically? | Session cache reuse? |
|------|--------|-------------------------------|---------------------|
| `reasonix chat` | Human (TUI) | No | Yes |
| `reasonix serve` | Human (Web UI) | No | Yes |
| `reasonix run` | Script / CI | Yes | **No** |
| Plugin (MCP client) | MCP servers → Reasonix | N/A | N/A |

`reasonix run` is the only programmatic entry point, but it starts a fresh
session every time — losing the prefix-cache stability that is Reasonix core
design advantage.

## Request

A new mode — call it `reasonix agent` or `reasonix session` — that exposes a
clean, minimal HTTP API for external agents to:

1. **Create a session** with a task description + system prompt
2. **Send follow-up messages** to the same session
3. **Receive the response** (streaming text + final structured output)
4. **Close the session**

This is **not** `reasonix serve` (which renders a Web UI). This is a headless
daemon mode with a simple JSON API.

### Proposed API

```
POST /session
  Body: { "task": "...", "system": "...", "model": "...", "tools": [...] }
  Response: { "session_id": "sess-abc123" }

POST /session/:id/message
  Body: { "content": "..." }
  Response: SSE stream of { "type": "delta", "content": "..." } ...
             then { "type": "done", "result": "full text" }

DELETE /session/:id
```

Or, alternatively, expose this through MCP as a proper MCP server — the reverse
of Reasonix current plugin/MCP-client role. Reasonix would become an MCP server
with a `reasonix.run_task` tool that accepts task descriptions and returns
structured results, with session affinity preserved.

### Current Workaround (Not Ideal)

We are currently calling `reasonix run` from a middleware project (Coagent) for
each review request. This works for one-shot reviews but wastes tokens on every
call because the session prefix is rebuilt from scratch.

### Use Case Context

Coagent is a middleware that sits between Codex and Reasonix, providing:
- Safe protocol translation
- Runtime policy enforcement (Rust allow/deny gate)
- Append-only SQLite audit logging
- Agent backend abstraction

The architecture is:

```
Codex ──MCP──→ Coagent ──spawn──→ Reasonix run (one-shot)
```

We want to evolve this to:

```
Codex ──MCP──→ Coagent ──HTTP──→ Reasonix session (reusable, cache-stable)
                                       │
                                  Multiple turns, cache retained
```

## Impact

This would unlock:
- Multi-agent orchestration where Reasonix is the expert worker
- CI pipelines that reuse session context across steps
- Any MCP client being able to call Reasonix as an expert tool
- Reasonix being used as a building block in larger agent systems

## Related

- [SPEC.md §3.4 Agent](https://github.com/esengine/DeepSeek-Reasonix/blob/main-v2/docs/SPEC.md#34-agent-internalagent) — already has `Session` + `Run(ctx, input)` abstractions
- Plugin system already supports MCP protocol

The foundation is there — it just needs an externally-callable surface.
