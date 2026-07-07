# Coagent Documentation

## Architecture

- [Collaboration Model](architecture/00-collaboration-model.md) — Roles, boundaries, current scope
- [Runtime](architecture/01-runtime.md) — State machine, policy engine, SQLite audit
- [MCP Server](architecture/02-mcp-server.md) — rmcp integration, tool definition, backend pluggability
- [General Agent Runtime Gaps](architecture/03-general-agent-runtime-gaps.md) — Resolved deficits from v1 → v2 architecture refactor

## Architecture Overview (v2)

```
Codex MCP Host
  -> coagent-mcp-server.exe (Rust, single binary, ~5 MB)
      ├── rmcp               MCP protocol via official Rust SDK
      ├── CoagentServer      MCP tool handler (reasonix.review_diff)
      ├── RuntimeKernel      same-process runtime gate
      │   ├── 10-state FSM   Queued/Running/Blocked/WaitingApproval/
      │   │                   Retrying/PartiallyCompleted/Completed/Failed/Cancelled
      │   │                  + subtask dependencies, timeout, cancel propagation
      │   ├── PolicyEngine   dynamic ToolRegistry + approval gates + path sandbox
      │   ├── Sandbox        execution isolation (env allowlist, resource budgets)
      │   ├── Replay         event-sourcing replay with idempotency
      │   └── Audit          SQLite 12 tables, WAL, append-only, foreign keys
      └── Backend            Mock | Reasonix (ACP → DeepSeek models)
```

## Key v2 Improvements (from v1 gaps document)

| Gap | Resolution |
|-----|-----------|
| Task model too flat | 10-state FSM with subtask deps, timeout, cancel propagation |
| Tool model hard-coded | Dynamic registry (register/unregister/enable/disable/upgrade) |
| Approval not composable | `ApprovalPolicy::Required` → `RequireApproval` decision gate |
| Schema dual-track | Single SchemaRegistry authority, handwritten `validate()` removed |
| Execution isolation shallow | `SandboxConfig` env allowlist/denylist, `ResourceBudgets` |
| Audit not full event sourcing | `replay_task_state()` + `check_idempotency()` |
| Tool model static | Thread-safe `Arc<RwLock<HashMap>>` runtime registry |

## Development

### Build

```powershell
# Rust MCP server (primary)
cargo build -p coagent-mcp-server
```

### Test

```powershell
cargo test --workspace    # 128 pass, 1 ignored (live Reasonix integration)
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

schemas/
  coagent-v1.schema.json    review_diff contract fixture (JSON Schema 2020-12)
```
