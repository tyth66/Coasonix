# Coagent

Codex-Orchestrated Reasonix Runtime — single-binary Rust MCP server with a 9-state agent runtime core.

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
      │   ├── StateMachine 9-state FSM + per-operation steps
      │   │                Queued→Running→Blocked/WaitingApproval/Retrying/PartiallyCompleted
      │   │                + subtask dependencies, timeout, cancel propagation
      │   ├── PolicyEngine dynamic ToolRegistry + approval gates + path sandbox
      │   ├── ContextProj  full input-to-prompt projection (9 fields)
      │   ├── Sandbox      execution isolation (env allowlist, resource budgets)
      │   ├── Replay       event-sourcing replay engine with idempotency
      │   └── Audit        SQLite 12 tables, WAL, append-only
      │                    schema validation audit on all 3 stages
      └── Backend          Mock | Reasonix (ACP → DeepSeek, session recovery)
```

## Project Structure

```
crates/
  coagent-runtime-core/     Library: state + policy + audit + sandbox + replay
  coagent-runtime-worker/   [DEPRECATED] JSON-RPC stdio worker
  coagent-mcp-server/       Binary (primary)
    src/
      pipeline/             RuntimeToolExecutor — unified 8-stage execution pipeline
      backends/             Mock, Reasonix ACP (session recovery), ContextProjection
      tools/                Tool-specific input/output types + Finding validation
docs/coagent/               Canonical documentation
schemas/                    JSON Schema 2020-12 contracts
.codex/skills/coagent/      Project-embedded usage skill
```

## Implementation Status

```text
MCP protocol (rmcp):                        implemented (official Rust SDK)
RuntimeToolExecutor pipeline:               implemented (8-stage unified execution)
Two-layer state machine:                    implemented (TaskState long-lived, per-op steps)
9-state task FSM:                           implemented
Multi-operation tasks:                      implemented (complete_task() separate from complete_operation())
Subtask dependencies + timeout/cancel:      implemented
Dynamic tool registry:                      implemented (register/unregister/enable/disable/upgrade)
Approval gate:                              partial (RequireApproval response; approve/resume tools pending)
Context projection:                         implemented (all 9 input fields reach Reasonix prompt)
Finding type safety:                        implemented (Finding struct + Severity enum, dual-layer validation)
ID orchestration:                           implemented (COAGENT_REQUIRE_EXTERNAL_IDS)
ACP session recovery:                       implemented (reconnect + retry on Io/Protocol errors)
ReasonixRunner observability:              implemented (ReasonixRunnerStats: sessions, prompts, reconnects,
                                            errors, tool_call/denied counters; coagent.runtime_status tool)
ACP tool_call policy:                      implemented (deny unsupported tools with TOOL_UNSUPPORTED rejection,
                                            max 5 consecutive denied tool_calls, non-retryable)
Policy engine:                              implemented (operation, permission, path, network, approval)
Execution sandbox:                          implemented (env allowlist/denylist, resource budgets)
Event-sourcing replay:                      implemented (replay_task_state, idempotency check)
Artifact policy:                            implemented (allowlist/denylist, glob, traversal, symlink)
Schema unification:                         implemented (SchemaRegistry single authority, JSON Schema 2020-12)
SQLite audit (full):                        implemented (schema_validation_results + audit_events on all 3 stages:
                                            input=review_diff_input_v1, output=pure_review_result_v1,
                                            wrapper=coagent_review_wrapper_v1)
Pure review result boundary:                implemented (Reasonix returns semantic-only; Coagent wraps)
Mock Reasonix backend:                      implemented (instant mock review)
Real Reasonix ACP backend:                  implemented (DeepSeek models over ACP protocol)
AgentBackend trait:                        implemented (AgentBackend, BackendRequest/Response,
                                            BackendRegistry, AcpBackend, MockBackend)
ToolSpec registry:                          implemented (declarative tool registration + capability tags)
BackendSelector:                           implemented (DefaultBackendSelector, PreferredBackendSelector)
Multi-backend registry:                     implemented (BackendRegistry with capability-based selection)
Task/Operation/Attempt:                     implemented (operation_attempts table, 3-layer state)
ACP contract tests:                         implemented (5 fake stdio + multi-step task + P2 integration)
ACP tool_call integration tests:           implemented (3 fake scenarios: existing-review return,
                                            deny-then-collect, max-tool-calls fail-fast)
```

## Verification

```powershell
cargo test --workspace    # 176 pass, 1 ignored (live Reasonix integration)
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
- [v3.1 Roadmap](docs/coagent/architecture/06-v3.1-roadmap.md) — Runtime behavior gaps
- [Session Management](docs/coagent/architecture/07-session-management.md) — Current single-runner + future SessionKey direction
- [v3 Blueprint](docs/coagent/architecture/05-v3-blueprint.md) — Multi-agent ACP runtime vision
- [Documentation Index](docs/coagent/README.md)
