# Coagent Documentation

## Architecture

- [Collaboration Model](architecture/00-collaboration-model.md) — Roles, boundaries, current scope
- [Runtime](architecture/01-runtime.md) — State machine, policy engine, SQLite audit
- [MCP Server](architecture/02-mcp-server.md) — rmcp integration, tool definition, backend pluggability
- [General Agent Runtime Gaps](architecture/03-general-agent-runtime-gaps.md) — Resolved deficits from v1 to v2
- [Architecture Backlog](architecture/04-backlog.md) — All 8 v2.1 implementation issues resolved

## Architecture Overview (v2.1)

```
Codex MCP Host
  -> coagent-mcp-server.exe (Rust, single binary, ~5 MB)
      ├── Pipeline         RuntimeToolExecutor — 8-stage unified execution
      ├── CoagentServer    MCP tool handler (declarative, ~50 lines per tool)
      ├── RuntimeKernel    same-process runtime gate
      │   ├── 10-state FSM queued/running/blocked/waiting-approval/retrying/
      │   │                partially-completed/completed/failed/cancelled
      │   │               + per-operation steps (TaskState + OperationState)
      │   ├── PolicyEngine dynamic ToolRegistry + approval gates + path sandbox
      │   ├── ContextProj  full input-to-prompt projection
      │   ├── Sandbox      execution isolation (env allowlist, resource budgets)
      │   ├── Replay       event-sourcing replay with idempotency
      │   └── Audit        SQLite 12 tables, WAL, append-only
      └── Backend          Mock | Reasonix ACP (session recovery)
```

## Key v2.1 Improvements (backlog resolution)

| Issue | Resolution |
|-------|-----------|
| P1: Handler pipeline monolithic | `RuntimeToolExecutor` — 8-stage unified pipeline, 180→50 lines per handler |
| P2: State machine too flat | Two-layer: TaskState (long-lived) + operation steps per task |
| P3: ID orchestration control | `COAGENT_REQUIRE_EXTERNAL_IDS` env var |
| P4: Context projection missing | `ContextProjection` — all 9 input fields reach Reasonix |
| P5: Findings type-unsafe | `Finding` struct + `Severity` enum with dual-layer validation |
| P6: Integration test gap | Multi-step task test + 5 ACP contract tests |
| P7: ACP session recovery | Reconnect + retry on recoverable Protocol/Io errors |
| P8: Audit completeness | Schema validation audit records in pipeline |

## Development

### Build

```powershell
cargo build -p coagent-mcp-server
```

### Test

```powershell
cargo test --workspace    # 143 pass, 1 ignored (live Reasonix integration)
```

### Verification

```powershell
cargo build -p coagent-mcp-server
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

## Project Structure

```
crates/
  coagent-runtime-core/     Runtime state + policy + audit + sandbox + replay (library)
  coagent-runtime-worker/   [DEPRECATED] JSON-RPC stdio worker
  coagent-mcp-server/       Rust MCP server binary (primary)
    src/
      pipeline/             RuntimeToolExecutor — unified execution pipeline
      backends/             Mock, Reasonix ACP, ContextProjection
      tools/                Tool input/output types + validation

schemas/
  coagent-v1.schema.json    review_diff contract fixture (JSON Schema 2020-12)
```

## Skill

A [Coagent usage skill](../../.codex/skills/coagent/SKILL.md) is bundled in the project
for Codex auto-discovery. It teaches Codex how to prepare diffs, call
`reasonix.review_diff`, interpret results, and troubleshoot failures.