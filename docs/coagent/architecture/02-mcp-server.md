# MCP Server (rmcp) — v2.1

The MCP server is built with `rmcp` (official Rust MCP SDK, 14.7M downloads).

## Tool Definition (declarative, ~30 lines)

```rust
#[tool_router]
impl CoagentServer {
    #[tool(name = "coagent.review_diff", description = "...")]
    async fn review_diff(
        &self,
        Parameters(input): Parameters<ReviewDiffInput>,
    ) -> Result<CallToolResult, ErrorData> {
        let artifact_paths = ArtifactPaths::collect_read(&input.artifacts.diff_path, &[...]);
        let context = ContextProjection::from_input(/* 9 fields */);
        let goal = input.goal.clone();
        let diff_path = input.artifacts.diff_path.clone();

        self.executor.execute(
            input.task_id, input.request_id, &input, artifact_paths,
            |backend| async move { /* backend call */ },
            |review| review.validate().map_err(ValidationError::from),
            |review| CoagentReviewWrapper { review, metadata: ... },
        ).await
    }
}
```

## Pipeline Stages (RuntimeToolExecutor)

```
1. Validate input schema   → SchemaRegistry, audit on failure
2. Generate/enforce IDs    → UUID or COAGENT_REQUIRE_EXTERNAL_IDS
3. Runtime gate            → evaluate_operation (Allow/Deny/RequireApproval)
4. Invoke backend          → Mock | Reasonix ACP (with session recovery)
5. Validate output         → Finding-level + pure_review_result_v1 audit
6. Validate wrapper schema → coagent_review_wrapper_v1 via SchemaRegistry
7. Complete lifecycle      → complete_operation; review_diff also complete_task_on_success
8. Serialize response      → MCP CallToolResult JSON
```

Schema validation stages 1, 5, and 6 write `schema_validation_results` and
paired `audit_events` for success and failure. Stage 5 records the pure backend
schema (`pure_review_result_v1`); stage 6 records the final MCP wrapper schema
(`coagent_review_wrapper_v1`).

## Backend Pluggability

```rust
enum Backend {
    Mock,                        // PureReviewResult::mock_pass()
    Reasonix(ReasonixRunner),   // ACP → subprocess → DeepSeek, session recovery
}
```

`COAGENT_BACKEND=mock|reasonix` overrides capability-based backend selection.

## Runtime Status Tool

`coagent.runtime_status` is a read-only MCP tool. It does not call Reasonix,
does not invoke an `AgentBackend`, and does not enter the review execution
pipeline. It reports the server's selected backend, repo root, and the current
single-runner Reasonix stats when the selected backend is Reasonix.

Current Reasonix status is intentionally single-runner scoped:

```json
{
  "backend": "reasonix",
  "repo_root": "D:/repo",
  "reasonix": {
    "has_session": true,
    "session_created_count": 1,
    "prompt_count": 2,
    "reconnect_count": 0,
    "timeout_count": 0,
    "protocol_error_count": 0,
    "io_error_count": 0,
    "spawn_error_count": 0,
    "last_error": null
  }
}
```

## Reasonix ACP Session Recovery

The `ReasonixRunner` implements one Reasonix-specific persistent ACP session.
It is intentionally serial: concurrent calls to the same runner queue behind
the session mutex, so only one prompt uses the stdin/stdout ACP stream at a
time. It currently drives the `reasonix acp --model ...` path, not arbitrary
`AgentProfile.command` / `AgentProfile.args` execution.

```
send_prompt() → Ok → return result
send_prompt() → Err(Io|Protocol) → drop session → reconnect → retry same prompt
send_prompt() → Err(Timeout) → drop session → propagate without retry
send_prompt() → Err(Spawn) → propagate immediately
tool_call after valid review JSON → return collected review immediately
tool_call before valid review JSON → deny tool (TOOL_UNSUPPORTED), increment counters, continue collecting`n≥5 consecutive denied tool_calls → max tool calls protocol error, drop session without retry
```

This ensures a single Reasonix child process crash does not permanently
disable the Coagent server. The runner does not execute Reasonix-requested ACP
tool calls; tool execution belongs to a later multi-tool design.
Denied tool calls are recorded in `tool_call_count` and `denied_tool_call_count`.

`ReasonixRunnerStats` records `has_session`, session creations, prompt
attempts, reconnects, timeout/protocol/I/O/spawn error counts, and the last
error string, plus `tool_call_count` and `denied_tool_call_count`.
These counters are observability only; they do not change routing
or lifecycle decisions.

## Context Projection

`ContextProjection` captures all `ReviewDiffInput` fields (goal, diff_path,
context_path, test_log_path, build_log_path, focus, constraints, base_branch,
working_branch) and renders them as a structured prompt section for Reasonix.

## Finding Type Safety

`Finding` struct with `Severity` enum (`Blocker|Major|Minor|Note`).
`PureReviewResult::validate()` checks per-finding: issue non-empty,
category non-empty, confidence 0.0-1.0. JSON Schema provides second-layer
enum value enforcement.

## Schema Authority

`SchemaRegistry` is the single validation authority (JSON Schema 2020-12).
Embedded `schemas/coagent-v1.schema.json` defines:

- `review_diff_input_v1` — MCP request schema
- `pure_review_result_v1` — Reasonix output schema
- `coagent_review_wrapper_v1` — Coagent wrapped response
- `runtime_status_input_v1` — read-only status tool input schema
- `runtime_status_v1` — read-only status response schema

## ID Orchestration

`COAGENT_REQUIRE_EXTERNAL_IDS=true` forces callers to provide both `task_id`
and `request_id`. Pipeline returns `invalid_params` if missing. Default
`false` preserves backward compatibility with auto-generated UUIDs.


## Tool Name (v3)

The MCP tool is registered as `coagent.review_diff`. The legacy name
`reasonix.review_diff` is not exposed by the current MCP router.

## Tool Specification (v3)

Tools are defined declaratively via `ToolSpec`. The default `ToolSpecRegistry`
contains:

```
coagent.review_diff
  input_schema: review_diff_input_v1
  output_schema: review_result_v1
  permission: L1_DIFF_REVIEW
  required_capability: code.review.diff
  default_backend: mock

coagent.runtime_status
  input_schema: runtime_status_input_v1
  output_schema: runtime_status_v1
  permission: L0_READONLY
  required_capability: runtime.status
  default_backend: mock
```

## Backend Registry (v3)

Backends implement the `AgentBackend` trait and are registered in
`BackendRegistry`:

```rust
backend_registry.register(Box::new(MockBackend::new("mock")));
backend_registry.register(Box::new(AcpBackend::new("reasonix", model, cwd)));
```

Selection is capability-based via `BackendSelector`.

## Attempt Recording (v3)

Each backend invocation is tracked in the `operation_attempts` table.
The kernel API (`start_attempt` / `complete_attempt` / `fail_attempt`)
is wired into the pipeline for per-operation attempt lifecycle management.
## Deployment

```powershell
cargo build --release -p coagent-mcp-server

codex mcp add coagent `
  --env COAGENT_REPO_ROOT=D:\your-repo `
  --env COAGENT_BACKEND=mock `
  -- D:\Coagent\target\release\coagent-mcp-server.exe
```
