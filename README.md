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
      ├── Pipeline         RuntimeToolExecutor (8-stage unified execution)
      ├── RuntimeKernel    same-process state machine + policy engine + SQLite audit
      │   ├── StateMachine 10-state FSM + operation-level steps (TaskState + OperationState)
      │   │                Queued→Running→Blocked/WaitingApproval/Retrying/PartiallyCompleted
      │   │                + subtask dependencies, timeout, cancel propagation
      │   ├── PolicyEngine dynamic ToolRegistry + approval gates + path sandbox
      │   ├── ContextProj  full input-to-prompt projection (focus, constraints, files)
      │   ├── Sandbox      execution isolation (env allowlist/denylist, resource budgets)
      │   ├── Replay       event-sourcing replay engine with idempotency
      │   └── Audit        SQLite 12 tables, WAL, append-only, foreign keys
      └── Backend          Mock | Reasonix (ACP -> DeepSeek models, session recovery)
```

## Project Structure

```
crates/
  coagent-runtime-core/     Library: state + policy + audit + sandbox + replay
  coagent-runtime-worker/   [DEPRECATED] JSON-RPC stdio worker
  coagent-mcp-server/       Binary (primary)
    src/
      pipeline/             RuntimeToolExecutor — unified 8-stage execution pipeline
      backends/             Mock, Reasonix ACP (with session recovery), ContextProjection
      tools/                Tool-specific input/output types + validation
docs/coagent/               Canonical documentation
schemas/                    JSON Schema 2020-12 contracts
.codex/skills/coagent/      Project-embedded usage skill
```

## Implementation Status

```text
MCP protocol (rmcp):                        implemented (official Rust SDK)
RuntimeToolExecutor pipeline:               implemented (8-stage: validate→gate→backend→validate→wrap)
Two-layer state machine:                    implemented (TaskState + per-operation steps, multi-op tasks)
10-state task FSM:                          implemented (9 alive + Cancelled terminal)
Subtask dependencies + timeout/cancel:      implemented
Dynamic tool registry:                      implemented (register/unregister/enable/disable/upgrade)
Approval gate:                              implemented (RequireApproval → WaitingApproval)
Context projection:                         implemented (all 9 input fields reach Reasonix prompt)
Finding type safety:                        implemented (strong Finding struct + Severity enum)
ID orchestration:                           implemented (COAGENT_REQUIRE_EXTERNAL_IDS)
ACP session recovery:                       implemented (reconnect + retry on recoverable errors)
Policy engine:                              implemented (operation, permission, path, network, approval)
Execution sandbox:                          implemented (env allowlist/denylist, resource budgets)
Event-sourcing replay:                      implemented (replay_task_state, idempotency check)
Artifact policy:                            implemented (allowlist/denylist, glob, traversal, symlink)
Schema unification:                         implemented (SchemaRegistry single authority, JSON Schema 2020-12)
SQLite audit + runtime events:              implemented (12 tables, WAL, append-only, schema validation audit)
Pure review result boundary:                implemented (Reasonix returns semantic-only; Coagent wraps)
Mock Reasonix backend:                      implemented (instant mock review)
Real Reasonix ACP backend:                  implemented (DeepSeek models over ACP protocol)
Reasonix ACP contract tests:                implemented (5 fake stdio ACP scenarios + multi-step task test)
```

## Verification

```powershell
cargo test --workspace    # 143 pass, 1 ignored (live Reasonix integration)
cargo build -p coagent-mcp-server
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

## Documentation

- [Collaboration Model](docs/coagent/architecture/00-collaboration-model.md)
- [Runtime: State, Policy, Audit](docs/coagent/architecture/01-runtime.md)
- [MCP Server (rmcp)](docs/coagent/architecture/02-mcp-server.md)
- [General Agent Runtime Gaps](docs/coagent/architecture/03-general-agent-runtime-gaps.md)
- [Architecture Backlog](docs/coagent/architecture/04-backlog.md) — All 8 issues resolved
- [Documentation Index](docs/coagent/README.md)