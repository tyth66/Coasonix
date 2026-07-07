# Coagent Collaboration Model

Coagent connects two agent systems without merging their responsibilities:

```
Codex   = assigns work and makes the final decision
Coagent = performs safe protocol translation, runtime gating, and audit
Reasonix = completes the delegated expert task
Codex   = evaluates the result and decides the next step
```

## Architecture (Rust, single binary)

```
Codex MCP Host
  -> coagent-mcp-server.exe (Rust, ~5 MB)
      ├── rmcp crate  (MCP protocol: initialize, tools/list, tools/call)
      ├── RuntimeKernel (same-process state + policy + audit)
      │     └── SQLite (append-only, .agent/coagent.sqlite)
      └── Backend (pluggable)
            ├── Mock      — instant mock review
            └── Reasonix  — ACP protocol → DeepSeek models
```

## Role Boundaries

### Codex
- Owns user intent, planning, workspace changes, final decision
- Calls `reasonix.review_diff` through MCP but owns the workflow

### Coagent
- MCP tool surface: `reasonix.review_diff`
- Runtime gate: state machine (Created→Running→Completed/Failed/Cancelled)
- Policy engine: operation, permission level, path allowlist/denylist, network
- SQLite append-only audit and runtime events (12 tables, WAL, foreign keys)
- Context projection (future), result validation, error taxonomy

### Reasonix
- Delegated expert task: diff review only
- Returns pure review result (verdict, summary, findings, tests_to_run, etc.)
- Must NOT return Coagent runtime metadata (task_id, request_id, status)

## Current Scope

One tool: `reasonix.review_diff`

Out of scope until v1 boundary is complete: propose_patch, apply_patch,
human approval, remote transport, additional Reasonix tools.
