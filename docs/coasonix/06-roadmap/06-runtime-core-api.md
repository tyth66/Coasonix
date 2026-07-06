# Runtime Core API

This document defines the Rust Runtime Core API boundary for v1. It refines the
technology decision in `04-technology-selection.md`.

The API principle is:

```text
Runtime-owned contracts are strongly typed.
Reasonix payloads are schema-validated JSON values until Rust needs their fields.
```

## 1. RuntimeKernel Boundary

`RuntimeKernel` is the only composition point for state, policy, and audit
gates. It is the sole authority for allow/deny decisions.

Actual implementation (`crates/coagent-runtime-core/src/kernel/mod.rs`):

```rust
pub struct RuntimeKernel {
    store: RuntimeStore,
    policy_engine: PolicyEngine,
}
```

External callers should not call submodules directly. The TypeScript MCP Adapter
talks to the Rust Runtime Worker, and the worker dispatches requests through
`RuntimeKernel`.

## 2. Public API Surface (Implemented)

The v1 Rust core API surface:

```rust
impl RuntimeKernel {
    pub fn initialize(config: RuntimeConfig) -> Result<Self, RuntimeError>;

    pub fn evaluate_operation(
        &mut self,
        request: RuntimeOperationRequest,
    ) -> RuntimeDecision;

    pub fn write_audit(
        &mut self,
        event: AuditEvent,
    ) -> Result<AuditWriteResult, RuntimeError>;
}
```

Worker-exposed JSON-RPC methods (`coagent-runtime-worker/src/main.rs`):

```text
runtime.initialize          -> RuntimeKernel::initialize
runtime.evaluate_operation  -> RuntimeKernel::evaluate_operation
runtime.write_audit         -> RuntimeKernel::write_audit
runtime.shutdown            -> { shutdown: true }
```

Rules:

```text
1. evaluate_operation is the main side-effect gate. It runs state + policy
   checks, merges decisions, persists audit, and advances Created->Running.
2. write_audit is owned by RuntimeKernel so audit sequencing remains centralized.
3. No validate_schema, transition_state, or evaluate_policy are exposed at
   the worker level in v1. Schema validation happens in the TypeScript adapter.
```

## 3. Strongly Typed Runtime-Owned Models (Implemented)

Rust v1 defines strongly typed structs for objects the runtime owns or
must inspect for safety.

Implemented strong types (`policy/mod.rs`, `state/mod.rs`, `kernel/mod.rs`):

```text
RuntimeConfig              { repo_root: PathBuf }
RuntimeOperationRequest    { task_id, request_id, operation, permission_level, resources }
RuntimeDecision            { task_id, request_id, operation, decision, engine_results, reasons, audit_event_id }
RuntimeDecisionValue       Allow | Deny | RequireApproval | RetryableError | FatalError
EngineResults              { state, policy }
TaskState                  { task_id, value, agent_calls, required_verification_gaps }
TaskStateValue             Created | Running | Completed | Failed
PermissionLevel            L0Readonly | L1DiffReview | L2PatchOnly | L3IsolatedWorktree
ResourceSet                { read_paths, write_paths, network }
PolicyEngine               with review_diff factory method
PolicyEvaluationResult     { decision, reasons }
ArtifactPolicy             with allow_read, allow_write, deny builder methods
AuditEvent                 { task_id, event_type, summary, payload_json }
AuditWriteResult           AuditEventRecord { id, task_sequence }
RuntimeStore               SQLite with 10 tables, WAL, FK, triggers
```

These types are Rust runtime-owned. The root JSON schema fixture currently
tracks only the active Reasonix review_diff input/output contract used in tests.

## 4. Decision Merge Logic (Implemented)

From `RuntimeKernel::merge_decisions()`:

```rust
policy=Deny       -> Deny
any=FatalError    -> FatalError
any=Deny          -> Deny
any=RequireApproval -> RequireApproval
any=RetryableError  -> RetryableError
otherwise         -> Allow
```

## 5. State Evaluation Logic (Implemented)

From `RuntimeKernel::evaluate_state()`:

```text
- Task in terminal state (Completed | Failed) -> Deny
  reason: "task {task_id} is in terminal state {state}"
- Task exists in non-terminal state -> Allow
- Task not found (fresh) -> Allow with new TaskState(task_id) in Created state
- Storage error -> FatalError
```

If the decision is Allow and the task is in Created state, the kernel
transitions it to Running via `Store::upsert_task_state`.

## 6. Policy Evaluation Logic (Implemented)

From `PolicyEngine::evaluate()` (`policy/mod.rs`):

```text
1. Check operation is registered       -> deny if unknown
2. Check permission level matches       -> add reason if mismatch
3. Check network                        -> add reason if true (default deny)
4. Check each read_path against artifact_policy.authorize(Read, path)  -> add reason if denied
5. Check each write_path against artifact_policy.authorize(Write, path) -> add reason if denied
6. If any reasons -> Deny, else Allow
```

Default policy from `RuntimeKernel::initialize()`:

```rust
artifact_policy
    .allow_read([".agent/context/**", ".agent/diffs/**", ".agent/logs/**",
                 "docs/**", "crates/**", "packages/**", "schemas/**"])
    .allow_write([".agent/results/**", ".agent/logs/**"])
    .deny([".agent/secrets/**", ".git/**"])
```

## 7. RuntimeDecision Payload Shape (Actual)

From `RuntimeDecision::to_payload()`:

```json
{
  "schema_version": "runtime_decision_v1",
  "task_id": "TASK-review-diff-1",
  "operation": "reasonix.review_diff",
  "decision": "allow",
  "engine_results": {
    "state": "allow",
    "policy": "allow"
  },
  "reasons": []
}
```

Optional fields: `request_id` (when present), `audit_event_id` (when audit persisted).

## 8. Store Boundaries (Implemented)

v1 uses a repo-local SQLite database at `.agent/coagent.sqlite`.

```text
RuntimeStore:
  - initialize(repo_root)           -> opens/creates SQLite, runs 10 migrations
  - upsert_task_state(TaskState)    -> tasks + task_state tables
  - load_task_state(task_id)        -> TaskState or TaskStateNotFound
  - commit_runtime_decision_with_audit(decision, audit) -> SQLite transaction
  - write_audit_event(event)        -> audit_events insert
  - foreign_keys_enabled()          -> PRAGMA check
  - journal_mode()                  -> WAL check
  - migration_tables()              -> runtime_metadata inspection
```

SQLite pragmas applied on every open:
```text
PRAGMA foreign_keys = ON
PRAGMA journal_mode = WAL
PRAGMA synchronous = FULL
busy_timeout = 5000ms
```

Tables (10 migrations):
1. `runtime_metadata` - migration tracking
2. `tasks` - task identity
3. `audit_events` - append-only (UPDATE/DELETE triggers raise ABORT)
4. `task_state` - per-task state + agent_calls
5. `runtime_decisions` - decision history with audit_event_id FK
6. `schema_validation_results` - schema validation records
7. `policy_evaluation_results` - policy evaluation records (table exists, limited writes)
8. `locks` - distributed lock table
9. `artifacts` - artifact path + hash records
10. `cache_entries` - cache metadata (reuse_enabled always 0 in v1)

## 9. Audit Ownership (Implemented)

Submodules produce data; only `RuntimeKernel` writes audit records.

```text
PolicyEngine.evaluate() -> PolicyEvaluationResult { decision, reasons }
RuntimeKernel.evaluate_state() -> (RuntimeDecisionValue, reasons, TaskState)
RuntimeKernel.persist_runtime_decision_with_audit() -> SQLite transaction:
  1. INSERT audit_events
  2. INSERT runtime_decisions (FK audit_event_id)
  3. COMMIT
```

Forbidden:
```text
PolicyEngine directly inserts audit rows.
SchemaRegistry directly inserts audit rows.
StateMachine directly mutates audit id or task_sequence.
Worker dispatch writes audit records outside RuntimeKernel.
```

## 10. Error Model

Rust internal errors:

```text
RuntimeError::Artifact(ArtifactPolicyError)
RuntimeError::Store(StoreError)
```

StoreError variants:
```text
Filesystem(io::Error)
Sqlite(rusqlite::Error)
MigrationFailed(String)
AppendOnlyViolation
TaskStateNotFound(String)
InvalidTaskState(String)
```

JSON-RPC boundary mapping (`coagent-runtime-worker/src/main.rs`):

```text
-32700  Parse error
-32600  Invalid Request
-32601  Method not found
-32602  Invalid params
-32008  runtime_unavailable
-32010  runtime_internal_error
```

## 11. Mutability Rules (Implemented)

```text
initialize:               constructs owned kernel state (store + policy_engine)
evaluate_operation:       &mut self (appends audit, updates state, advances cursors)
write_audit:              &mut self (appends audit event)
```

`evaluate_operation` is mutable because it may:
```text
append audit events (store.write_audit_event)
update task state counters (store.upsert_task_state)
record runtime decisions (store.commit_runtime_decision_with_audit)
advance per-task audit task_sequence cursors
transition Created -> Running
```

## 12. v1 API Slice (Actual)

The v1 implementation includes:

```text
RuntimeKernel::initialize           -> store + policy_engine setup
RuntimeKernel::evaluate_operation   -> state + policy + merge + audit + state advance
RuntimeKernel::write_audit          -> standalone audit event write
```

The adapter vertical slice calls:

```text
runtime.initialize({ repo_root })
runtime.evaluate_operation({ task_id, operation, permission_level, resources })
runtime.shutdown()
```

through JSON-RPC 2.0 over stdio via `RuntimeWorkerClient`.
