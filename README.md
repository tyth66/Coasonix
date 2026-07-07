# Coagent

Codex-Orchestrated Reasonix Runtime — single-binary Rust MCP server with a 10-state agent runtime core.

```
Codex   = assigns work and makes the final decision
Coagent = performs safe protocol translation, runtime gating, and audit
Reasonix = completes the delegated expert task
Codex   = evaluates the result and decides the next step
```

## Quick Start

```powershell
# Build
cargo build -p coagent-mcp-server

# Register with Codex
codex mcp add coagent `
  --env COAGENT_REPO_ROOT=D:\your-repo `
  -- D:\Coagent\target\debug\coagent-mcp-server.exe
```

## Architecture

```
Codex MCP Host
  -> coagent-mcp-server.exe (~5 MB, single binary, zero dependencies)
      ├── rmcp              MCP protocol (initialize, tools/list, tools/call)
      ├── RuntimeKernel      same-process state machine + policy engine + SQLite audit
      │   ├── StateMachine   10-state FSM (Queued→Running→Completed|Failed|Cancelled)
      │   │                   + Blocked, WaitingApproval, Retrying, PartiallyCompleted
      │   │                   + subtask dependencies, timeout, cancel propagation
      │   ├── PolicyEngine   dynamic ToolRegistry + approval gates + path sandbox
      │   ├── Sandbox        execution isolation (env allowlist, resource budgets)
      │   ├── Replay         event-sourcing replay engine with idempotency
      │   └── Audit          SQLite 12 tables, WAL, append-only, foreign keys
      └── Backend            Mock | Reasonix (ACP -> DeepSeek models)
```

## Project Structure

```
crates/
  coagent-runtime-core/     Runtime state + policy + audit + sandbox + replay (library)
  coagent-runtime-worker/   [DEPRECATED] JSON-RPC stdio worker
  coagent-mcp-server/       MCP server binary (primary)

docs/coagent/              Canonical documentation
schemas/                   Contract fixtures (JSON Schema 2020-12)
```

## Implementation Status

```text
MCP protocol (rmcp):                        implemented (official Rust SDK, 14.7M downloads)
10-state task FSM:                          implemented (Queued, Running, Blocked, WaitingApproval,
                                            Retrying, PartiallyCompleted, Completed, Failed, Cancelled)
Subtask dependencies + timeout/cancel:      implemented
Dynamic tool registry:                      implemented (register, unregister, enable, disable, upgrade)
Approval gate:                              implemented (RequireApproval decision, paused execution)
Policy engine:                              implemented (operation, permission, path, network, approval)
Execution sandbox:                          implemented (env allowlist/denylist, resource budgets)
Event-sourcing replay:                      implemented (replay_task_state, idempotency check)
Artifact policy:                            implemented (allowlist/denylist, glob, traversal, symlink)
Schema unification:                         implemented (SchemaRegistry single authority, JSON Schema 2020-12)
SQLite audit + runtime events:              implemented (12 tables, WAL, append-only audit/events)
Pure review result boundary:                implemented (Reasonix returns semantic-only; Coagent wraps)
Mock Reasonix backend:                      implemented (instant mock review)
Real Reasonix ACP backend:                  implemented (DeepSeek models over ACP protocol)
Reasonix ACP contract tests:                implemented (fake stdio ACP backend, 5 protocol scenarios)
```

## Verification

```powershell
cargo test --workspace    # 128 pass, 1 ignored (live Reasonix integration)
cargo build -p coagent-mcp-server
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

## Documentation

- [Collaboration Model](docs/coagent/architecture/00-collaboration-model.md)
- [Runtime: State, Policy, Audit](docs/coagent/architecture/01-runtime.md)
- [MCP Server (rmcp)](docs/coagent/architecture/02-mcp-server.md)
- [General Agent Runtime Gaps](docs/coagent/architecture/03-general-agent-runtime-gaps.md)
- [Architecture Backlog](docs/coagent/architecture/04-backlog.md) — P1-P8 implementation issues
- [Documentation Index](docs/coagent/README.md)
