# Coagent Documentation

## Architecture

- [Collaboration Model](architecture/00-collaboration-model.md) — Roles, boundaries, current scope
- [Runtime](architecture/01-runtime.md) — State machine, policy engine, SQLite audit, pipeline, session recovery
- [MCP Server](architecture/02-mcp-server.md) — rmcp integration, tool definition, pipeline stages, deployment
- [General Agent Runtime Gaps](architecture/03-general-agent-runtime-gaps.md) — Resolved deficits from v1 to v2
- [Architecture Backlog](architecture/04-backlog.md) — All 8 v2.1 issues resolved
- [v3 Blueprint](architecture/05-v3-blueprint.md) — Multi-agent ACP runtime vision

## Architecture Overview (v2.1)

```
Codex MCP Host
  -> coagent-mcp-server.exe (Rust, single binary, ~5 MB)
      ├── Pipeline         RuntimeToolExecutor — 8-stage unified execution
      ├── CoagentServer    MCP tool handler (declarative, ~30 lines per tool)
      ├── RuntimeKernel    same-process runtime gate
      │   ├── 10-state FSM queued/running/blocked/waiting-approval/retrying/
      │   │                partially-completed/completed/failed/cancelled
      │   │               + per-operation steps (multi-op tasks)
      │   ├── PolicyEngine dynamic ToolRegistry + approval gates + path sandbox
      │   ├── ContextProj  full input-to-prompt projection (9 fields)
      │   ├── Sandbox      execution isolation (env allowlist, resource budgets)
      │   ├── Replay       event-sourcing replay with idempotency
      │   └── Audit        SQLite 12 tables, WAL, append-only
      │                    schema validation audit on all 3 stages
      └── Backend          Mock | Reasonix ACP (session recovery)
```

## Key v2.1 Improvements

| Issue | Resolution |
|-------|-----------|
| P1: Handler pipeline monolithic | `RuntimeToolExecutor` — 8-stage pipeline, ~30 lines per handler |
| P2: State machine flat | Two-layer: TaskState (long-lived) + per-operation steps |
| P3: ID orchestration | `COAGENT_REQUIRE_EXTERNAL_IDS` env var |
| P4: Context projection | `ContextProjection` — all 9 input fields reach Reasonix |
| P5: Findings type-unsafe | `Finding` struct + `Severity` enum, dual-layer validation |
| P6: Integration test gap | Multi-step task test + 5 ACP contract tests |
| P7: ACP session recovery | Reconnect + retry on Io/Protocol errors |
| P8: Audit completeness | Schema validation audit on all 3 pipeline stages |

## Development

### Build

```powershell
cargo build -p coagent-mcp-server
```

### Test

```powershell
cargo test --workspace    # 143 pass, 1 ignored (live Reasonix)
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
  coagent-runtime-core/     Runtime state + policy + audit + sandbox + replay
  coagent-runtime-worker/   [DEPRECATED] JSON-RPC stdio worker
  coagent-mcp-server/       Rust MCP server binary (primary)
    src/
      pipeline/             RuntimeToolExecutor — unified execution pipeline
      backends/             Mock, Reasonix ACP (session recovery), ContextProjection
      tools/                Tool input/output types + Finding validation

schemas/
  coagent-v1.schema.json    review_diff contract fixture (JSON Schema 2020-12)
```

## Skill

A [Coagent usage skill](../../.codex/skills/coagent/SKILL.md) is bundled in the project.