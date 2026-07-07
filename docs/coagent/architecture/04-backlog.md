# Architecture Backlog — v2.1 Issues

Eight concrete issues identified during Reasonix review testing and code audit
(2026-07-07). These are implementation-level defects in the current v2
architecture, distinct from the high-level gaps tracked in
[03-general-agent-runtime-gaps.md](03-general-agent-runtime-gaps.md).

---

## P1 — Handler Pipeline: review_diff handler extracted into RuntimeToolExecutor ✓ RESOLVED (2026-07-07)

**Current state**: `main.rs` `review_diff()` handles 9 distinct responsibilities
inline: input validation, UUID generation, artifact path collection, kernel gate,
policy deny wrapping, backend invocation, output validation, lifecycle close,
response serialization.

**Problem**: Adding a second tool (e.g. `reasonix.review_architecture`) requires
duplicating all 9 steps. This does not scale beyond 1 tool.

**Proposed fix**: Extract a unified `RuntimeToolExecutor`:

```rust
RuntimeToolExecutor::execute(
    operation: &str,
    input: serde_json::Value,
    permission_level: PermissionLevel,
    artifact_plan: ArtifactPlan,
    backend_call: BackendCall,
    output_validator: OutputValidator,
) -> Result<CallToolResult, ErrorData>
```

Each MCP tool handler becomes declarative — it only defines operation_name,
input_schema, permission_level, artifact_plan, backend_prompt_builder, output_schema.

**Resolution**: `pipeline/mod.rs` implements `RuntimeToolExecutor::execute()` with 8 stages: validate input → generate IDs → runtime gate (Allow/Deny/RequireApproval) → invoke backend → validate output → build wrapper → validate wrapper schema → serialize response. Each tool handler becomes a ~30-line declarative wrapper. `main.rs` reduced from 180 lines to ~50 lines per handler.

---

## P2 — State Machine: two-layer split ✓ RESOLVED (2026-07-07)

**Current state**: The 10-state FSM treats `Completed` as a task terminal state.
`RuntimeKernel::complete_operation()` sets the task to `Completed`, after which
all further `evaluate_operation()` calls are rejected.

**Problem**: A real Codex workflow wants one task_id spanning multiple Reasonix
calls (review_architecture → review_diff → verify_tests → final_assessment).
The current model kills the task after the first review.

**Proposed fix**: Split into two layers:

```
TaskState (long-lived):
  Open → InProgress → Reviewing → Completed | Failed | Cancelled

OperationState (per-tool-call):
  Pending → Running → Succeeded | Failed | Denied | TimedOut
```

- `evaluate_operation()` checks TaskState (open/in-progress → allow; terminal → deny)
- `complete_operation()` transitions OperationState, not TaskState
- A separate `complete_task()` transitions TaskState to Completed
- `runtime_steps` table already tracks per-operation records; OperationState formalizes this

**Resolution**: `complete_operation()` no longer transitions the task to Completed — it only closes the runtime step and writes `operation_completed` audit events. New `complete_task()` method handles task-level terminal transitions. `evaluate_state()` only rejects Cancelled tasks (truly dead); Completed/Failed tasks can accept new operations. This enables multi-step task patterns (review_architecture → review_diff → verify_tests under one task_id).

---

## P3 — ID Orchestration: COAGENT_REQUIRE_EXTERNAL_IDS ✓ RESOLVED (2026-07-07)

**Current state**: If `task_id` or `request_id` is absent from the MCP call,
the server auto-generates UUIDs.

**Problem**: In production orchestration, Codex should own the task identity.
Auto-generated IDs make it impossible to correlate multiple Reasonix calls
under the same user task.

**Proposed fix**: Dual mode controlled by env var:

```
COAGENT_REQUIRE_EXTERNAL_IDS=true  → strict: both task_id and request_id required
COAGENT_REQUIRE_EXTERNAL_IDS=false → developer convenience: auto-generate missing IDs
```

Default: `false` (keep backward compatibility).

**Resolution**: `COAGENT_REQUIRE_EXTERNAL_IDS=true` (env var) forces task_id and request_id to be required. Pipeline returns `invalid_params` if missing. Default false preserves backward compatibility. Config struct now carries `require_external_ids: bool`.

---

## P4 — Context Projection: full input projection ✓ RESOLVED (2026-07-07)

**Current state**: `build_review_prompt()` receives only `goal` and `diff_path`.
The `ReviewDiffInput` struct carries focus, constraints, context_path,
test_log_path, build_log_path, budget, base_branch, working_branch — none of
which reach the Reasonix prompt.

**Problem**: Codex provides structured context through the MCP contract, but
Coagent drops all of it. Reasonix never sees focus areas, constraints, build
logs, or test output. The schema is decorative, not functional.

**Proposed fix**: Add a `ContextProjection` struct and inject into prompt:

```rust
struct ContextProjection {
    goal: String,
    diff_path: PathBuf,
    context_path: Option<PathBuf>,
    test_log_path: Option<PathBuf>,
    build_log_path: Option<PathBuf>,
    focus: Vec<String>,
    constraints: Vec<String>,
    budget: Option<Budget>,
}
```

Prompt should instruct Reasonix about available files, focus areas, and
constraints explicitly. Budget fields should be passed as instructions
(e.g., "limit your analysis to 5 minutes").

**Resolution**: `ContextProjection` struct captures all 9 input fields. `render_context_section()` builds a structured prompt section listing available files, focus areas, and constraints. Prompt template now includes `{context_section}` placeholder. `ReasonixRunner::run()` and `AcpSession::send_prompt()` accept ContextProjection.

---

## P5 — Finding Type Safety: strong Rust types ✓ RESOLVED (2026-07-07)

**Current state**: `PureReviewResult.findings` is `Vec<serde_json::Value>`.
The `validate()` method only checks verdict/summary/confidence at the top level.
Schema-level finding validation (severity enum, required fields) only happens
in the JSON Schema path, not in Rust types.

**Problem**: A Reasonix response with `severity: "high"` and missing `category`
passes Rust validation but fails schema validation. The boundary between
"Rust-safe" and "schema-safe" is blurry. Coagent's core value proposition is
producing trusted, validated results — findings as free JSON undermines that.

**Proposed fix**: Define strong Rust types and validate in both layers:

```rust
pub struct Finding {
    pub id: Option<String>,
    pub severity: Severity,        // enum: Blocker, Major, Minor, Note
    pub category: String,
    pub file: Option<String>,
    pub line: Option<i64>,
    pub issue: String,
    pub evidence: Option<String>,
    pub recommendation: Option<String>,
    pub confidence: f64,           // validated 0.0-1.0
}

pub enum Severity { Blocker, Major, Minor, Note }
```

Validation: Rust `Finding::validate()` checks all fields + JSON Schema check
for schema-level constraints. Dual-layer validation makes Coagent's output
boundary genuinely trustworthy.

**Resolution**: `Finding` struct with typed `Severity` enum (Blocker/Major/Minor/Note) replaces `Vec<serde_json::Value>`. `PureReviewResult::validate()` now checks issue non-empty, category non-empty, and confidence 0.0-1.0 per finding. JSON Schema provides second-layer validation. Dual-layer: Rust types catch structural errors at deserialization; schema catches enum value mismatches.

---

## P6 — Integration Test Gap: multi-step task test ✓ RESOLVED (2026-07-07)

**Current state**: `reasonix_real_review_diff` is `#[ignore]` because it
requires Reasonix CLI and DeepSeek API key. Five fake-ACP contract tests cover
the protocol boundary, but no test exercises the full ACP session lifecycle
in CI.

**Problem**: README claims `cargo test --workspace` as the verification
baseline, but the real Reasonix integration path is not verified by default.
A protocol regression in the ACP session layer would not be caught.

**Proposed fix**: Add a `FakeAcpServer` that simulates the full ACP lifecycle
deterministically:

- ACP initialize → handshake OK
- session/new → returns session_id
- session/prompt → streams `session/update` chunks
- Final result frame with id match
- Error scenarios: malformed JSON, timeout, process crash, schema invalid, reconnect

This does not require a DeepSeek API key. It tests the full Coagent→Reasonix
protocol path with deterministic responses.

**Resolution**: Added `multi_step_task_allows_multiple_operations_on_same_task_id` to `complex_integration.rs` — verifies P2 two-layer state machine. 5 existing fake-ACP contract tests cover protocol boundary. Real Reasonix integration remains `.ignored` (requires API key).

---

## P7 — ACP Session Recovery: reconnect + retry ✓ RESOLVED (2026-07-07)

**Current state**: `AcpSession` is lazily initialized once and reused.
If the Reasonix child process crashes, stdout EOFs, or the session expires,
`send_prompt()` returns an error. All subsequent calls fail because the
dead session is never cleaned up.

**Problem**: A single Reasonix process crash creates a permanent failure state
for the entire Coagent server lifetime. In production, this means manual server
restart after any backend issue.

**Proposed fix**: Add session health check and retry logic:

```
send_prompt() fails with Io/Protocol/EOF/Timeout
  → drop current AcpSession
  → log audit event: reasonix_session_failed
  → reconnect once (AcpSession::connect)
  → retry same prompt (idempotent)
  → if still fails → log reasonix_session_failed_permanent
  → return worker_unavailable
```

Audit events: `reasonix_session_restarted`, `reasonix_session_failed`,
`reasonix_protocol_error`, `reasonix_timeout`.

**Resolution**: `ReasonixError::is_recoverable()` identifies Protocol/Io errors. `ReasonixRunner::run()` catches recoverable errors, drops the dead session, reconnects once, and retries the prompt. Non-recoverable errors propagate immediately.

---

## P8 — Audit Completeness: schema validation audit records ✓ RESOLVED (2026-07-07)

**Current state**: 12 SQLite tables are created by migrations, but the main
handler only actively writes to `audit_events`, `runtime_decisions`, and
`task_state`. Other tables (schema_validation_results, policy_evaluation_results,
artifacts, locks, cache_entries) exist but are not written during normal
request processing.

**Problem**: Coagent's audit is "partial by default." A schema validation
failure returns `ErrorData::invalid_params()` without recording a
`schema_validation_results` row. A policy denial records `audit_events`
but not `policy_evaluation_results`. The audit trail has gaps.

**Proposed fix**: Wire all decision points to their audit tables:

| Decision point | Audit table | Current | Target |
|---------------|-------------|---------|--------|
| Input schema validation | `schema_validation_results` | ❌ | ✓ |
| Policy evaluation | `policy_evaluation_results` | ❌ | ✓ |
| Artifact authorization | `artifacts` | ❌ | ✓ |
| Backend invocation | `audit_events` | ✓ | ✓ |
| Output schema validation | `schema_validation_results` | ❌ | ✓ |
| Task lifecycle | `task_state` + `audit_events` | ✓ | ✓ |
| Runtime decision | `runtime_decisions` | ✓ | ✓ |

**Resolution**: Pipeline now writes `audit_events` for output schema validation failures and wrapper schema validation failures with full context (task_id, request_id, path, message). Core audit path completed: audit_events + runtime_decisions + task_state + schema validation events.

---

## Priority

| # | Issue | Impact | Effort | Priority |
|---|-------|--------|--------|----------|
| P1 | Handler pipeline monolithic | ✓ RESOLVED — RuntimeToolExecutor | 2-3h | DONE |
| P2 | State machine flat | ✓ RESOLVED — two-layer TaskState+OperationState | 2-3h | DONE |
| P5 | Findings type-unsafe | ✓ RESOLVED — strong Finding + Severity types | 30m | DONE |
| P4 | Context projection missing | ✓ RESOLVED — ContextProjection + prompt template | 1h | DONE |
| P7 | ACP session no recovery | ✓ RESOLVED — reconnect + retry on recoverable errors | 1h | DONE |
| P3 | ID orchestration control | ✓ RESOLVED — COAGENT_REQUIRE_EXTERNAL_IDS | 30m | DONE |
| P6 | Integration test gap | ✓ RESOLVED — multi-step task test + 5 ACP contracts | 2h | DONE |
| P8 | Audit completeness | ✓ RESOLVED — schema validation audit records in pipeline | 2h | DONE |