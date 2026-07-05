# Runtime Core API

This document defines the Rust Runtime Core API boundary for v1. It refines the
technology decision in `04-technology-selection.md` and the v1 scope in
`05-v1-mvp-scope.md`.

The API principle is:

```text
Runtime-owned contracts are strongly typed.
Reasonix payloads are schema-validated JSON values until Rust needs their fields.
```

## 1. RuntimeKernel Boundary

`RuntimeKernel` is the only composition point for schema, state, policy, audit,
locks, and artifact gates.

Conceptual shape:

```rust
pub struct RuntimeKernel {
    schema: SchemaRegistry,
    state: StateStore,
    policy: PolicyEngine,
    audit: AuditWriter,
    db: RuntimeDatabase,
    locks: LockTable,
}
```

External callers should not call submodules directly. The TypeScript MCP Adapter
talks to the Rust Runtime Worker, and the worker dispatches requests through
`RuntimeKernel`.

## 2. Public API Surface

The v1 public Rust core API should remain narrow:

```rust
impl RuntimeKernel {
    pub fn initialize(config: RuntimeConfig) -> Result<Self, RuntimeError>;

    pub fn validate_schema(
        &self,
        request: SchemaValidationRequest,
    ) -> SchemaValidationResult;

    pub fn evaluate_operation(
        &mut self,
        request: RuntimeOperationRequest,
    ) -> RuntimeDecision;

    pub fn transition_state(
        &mut self,
        request: TransitionRequest,
    ) -> TransitionResult;

    pub fn evaluate_policy(
        &self,
        request: PolicyEvaluationRequest,
    ) -> PolicyEvaluationResult;

    pub fn write_audit(
        &mut self,
        event: AuditEvent,
    ) -> Result<AuditWriteResult, RuntimeError>;
}
```

Rules:

```text
1. evaluate_operation is the main side-effect gate.
2. validate_schema validates payloads but does not by itself authorize action.
3. transition_state is exposed for explicit state operations and tests, but
   ordinary mutating flows should pass through evaluate_operation.
4. evaluate_policy is a focused policy query, not a full runtime decision.
5. write_audit is owned by RuntimeKernel so audit sequencing remains centralized.
```

## 3. Strongly Typed Runtime-Owned Models

Rust v1 should define strongly typed structs for objects the runtime owns or
must inspect for safety.

Required strong types:

```text
RuntimeConfig
RuntimeOperationRequest
RuntimeDecision
TransitionRequest
TransitionResult
PolicyEvaluationRequest
PolicyEvaluationResult
SchemaValidationRequest
SchemaValidationResult
TaskState
AuditEvent
AuditWriteResult
ErrorResult
RoutingMetadata
ResourceSet
PermissionLevel
RuntimeDecisionValue
TaskStateValue
```

These types are Rust runtime-owned. The root JSON schema fixture currently
tracks only the active Reasonix review_diff input/output contract used in tests.

## 4. JSON Value Reasonix Payloads

Reasonix result payloads do not enter Rust for schema validation in the current
v1 architecture path. The TypeScript adapter checks the result contract before
returning MCP `structuredContent`.

Payloads kept as JSON values in v1:

```text
review_result_v1
security_audit_v1
debug_hypothesis_v1
architecture_options_v1
performance_review_v1
patch_proposal_v1
test_plan_v1
```

Rules:

```text
1. Payloads are parsed with duplicate-key rejection before validation.
2. Reasonix review payloads are validated against the active review-data contract.
3. Rust owns pre-delegation safety gates. Review-result shape checks stay in the adapter unless a future safety requirement proves otherwise.
4. Rust must not rely on unchecked ad hoc payload fields for allow decisions.
5. Add a strong type for a Reasonix payload only when Rust needs to inspect that
   schema's fields for runtime safety or transaction semantics.
```

For v1, `reasonix.review_diff` output needs schema validation and common field
checks only. It does not need a full Rust `ReviewResult` domain model.

## 5. Store Boundaries

v1 uses a repo-local SQLite database plus file-backed artifacts.

```text
RuntimeDatabase:
  opens .agent/coasonix.sqlite

StateStore:
  reads/writes task_state rows

ArtifactGate:
  validates artifact paths and resource access, with metadata in SQLite
```

Rules:

```text
1. StateStore owns task state persistence.
2. AuditWriter owns audit ordering persistence: global row id plus per-task
   task_sequence.
3. ArtifactGate is not a general filesystem abstraction.
4. RuntimeKernel coordinates store writes so state and audit behavior remain
   auditable from one call path.
5. Worker memory is an acceleration layer; .agent/coasonix.sqlite and artifact
   files remain the recovery source.
6. Audit, state, locks, and cache metadata use SQLite transactions.
```

## 6. Audit Ownership

Submodules may produce audit event candidates, but only `RuntimeKernel` writes
audit records.

Allowed:

```text
PolicyEngine returns matched rule data.
StateMachine returns transition allow/deny data.
SchemaRegistry returns validation errors.
RuntimeKernel builds and writes audit_event_v1.
```

Forbidden:

```text
PolicyEngine directly inserts audit rows.
SchemaRegistry directly inserts audit rows.
StateMachine directly mutates audit id or task_sequence.
Worker dispatch writes audit records outside RuntimeKernel.
```

This keeps audit ordering, failure behavior, and append-only semantics
centralized. The global audit id is database order; task_sequence is allocated
per task by RuntimeKernel/AuditWriter inside the same transaction as the runtime
decision.

## 7. Error Model

Rust internal errors may be richer than JSON-RPC errors.

Internal error categories:

```text
InvalidRequest
SchemaInvalid
StateDenied
PolicyDenied
ApprovalRequired
BudgetExceeded
PatchDenied
CacheDenied
SnapshotMismatch
Io
Internal
```

JSON-RPC boundary mapping:

```text
RuntimeError -> JSON-RPC error
JSON-RPC error.data -> error_result_v1 when task_id/request_id are available
```

Rules:

```text
1. Internal errors must not leak secrets or raw environment values.
2. Policy/state denials are expected runtime results, not panics.
3. Parse errors and invalid JSON-RPC frames use JSON-RPC standard errors.
4. Runtime denials use Coasonix -320xx error codes.
5. Worker crash or unavailable worker maps to runtime_unavailable in the adapter
   and side_effect_not_executed.
```

JSON-RPC error code mapping:

```text
-32700  Parse error
-32600  Invalid Request
-32601  Method not found
-32602  Invalid params
-32001  runtime_policy_denied
-32002  runtime_state_denied
-32003  runtime_schema_invalid
-32004  runtime_approval_required
-32005  runtime_budget_exceeded
-32006  runtime_snapshot_mismatch
-32007  runtime_storage_error
-32008  runtime_unavailable
-32009  runtime_unknown_operation
-32010  runtime_internal_error
```

Architecture impact:

```text
No architecture change. This fixes adapter/worker interoperability by making
the existing Coasonix -320xx range executable and testable.
```

## 8. Mutability Rules

Use mutability to expose side-effect potential clearly.

```text
initialize: constructs owned runtime state
validate_schema: &self
evaluate_policy: &self
evaluate_operation: &mut self
transition_state: &mut self
write_audit: &mut self
```

`evaluate_operation` is mutable because it may:

```text
append audit events
update task state counters
acquire or release locks
record runtime decisions
advance per-task audit task_sequence cursors
```

Pure validation functions should remain immutable unless they explicitly emit
audit records through `RuntimeKernel`.

## 9. v1 Minimum API Slice

The first implementation slice should include:

```text
RuntimeKernel::initialize
RuntimeKernel::validate_schema
RuntimeKernel::evaluate_operation
RuntimeKernel::write_audit
```

`transition_state` and `evaluate_policy` may be implemented as internal
subroutines first, then exposed when tests require direct entry points.

The first adapter vertical slice calls:

```text
runtime.initialize
runtime.evaluate_operation
runtime.write_audit
```

through JSON-RPC 2.0 over stdio.
