# Runtime Enforcement Layer

当前 Coasonix 文档已经定义了规则、schema、状态机、policy、事务、缓存和可观测性。系统真正能否安全运行，取决于 Runtime Enforcement Layer 是否把这些规则变成不可绕过的运行时门禁。

本文件定义 Coasonix 的执行内核。

## 1. Runtime Positioning

Coasonix 当前阶段：

```text
Deterministic Multi-Agent Runtime Spec: complete
Runtime Enforcement Layer: required before safe operation
```

换句话说，系统已经可以实现，但在 Runtime Enforcement Layer 落地之前，不应声称可以安全运行。

MVP deployment:

```text
Runtime Kernel is embedded inside `reasonix-expert` Wrapper.
Transport is local STDIO.
Remote Runtime Service / Reasonix worker pool is deferred.
```

## 2. Runtime Boundary

Runtime Enforcement Layer 位于所有高风险动作之前：

```text
Codex Orchestrator
  -> Runtime Enforcement Layer
      -> State Machine Runtime Engine
      -> Schema Enforcement Layer
      -> Policy Execution Engine
  -> Allowed Side Effect
```

所有以下动作都必须经过 Runtime Enforcement Layer：

```text
state transition
MCP tools/call
Reasonix result acceptance
patch safety check
patch transaction apply
verification completion
human approval unblock
shell command execution
network request
filesystem read/write
cache result reuse
task completion
```

## 3. Core Principle

```text
Rules are not advisory.
Runtime gates enforce rules.
Fail closed on uncertainty.
```

任何模块都不得直接执行高风险动作并在事后记录。审计是证据链，不是授权机制。

## 4. Runtime Kernel

Runtime Kernel 是三个 engine 的组合：

```text
State Machine Runtime Engine
Schema Enforcement Layer
Policy Execution Engine
```

Kernel 接收 operation request，执行三类检查，返回 allow / deny / require_approval。Reasonix Project / Session Lane routing is evaluated as part of policy execution and audit metadata.

## 5. Runtime Operation Flow

```text
operation_request
-> schema_enforcer.validate_request
-> state_machine.assert_transition_or_action_allowed
-> policy_engine.evaluate
-> runtime_decision
-> execute only if decision=allow
-> schema_enforcer.validate_result
-> state_machine.commit_transition
-> audit_event_v1
```

如果任何一步失败：

```text
decision = deny
side_effect = not_executed
audit_event = runtime_denied
```

## 6. Runtime Decision Values

```text
allow
deny
require_approval
retryable_error
fatal_error
```

Rules:

```text
1. allow is the only decision that permits side effects.
2. require_approval moves task to waiting_for_approval and blocks side effects.
3. deny is final for the requested operation.
4. retryable_error may retry only if budget remains and state allows retry.
5. fatal_error moves task to failed unless policy explicitly allows recovery.
```

## 7. Runtime Operation Types

```text
transition_state
call_reasonix_tool
accept_reasonix_result
route_reasonix_project
route_reasonix_session
freeze_snapshot
apply_patch_transaction
run_verification
complete_task
request_human_approval
resolve_human_approval
read_artifact
write_artifact
run_shell
open_network
reuse_cache_result
```

## 8. State Machine Runtime Engine

### 8.1 Responsibility

State Machine Runtime Engine enforces [01-global-task-state-machine.md](01-global-task-state-machine.md).

It owns:

```text
allowed transition table
forbidden transition table
terminal state rules
waiting_for_approval blocking semantics
verification gap completion semantics
limit-triggered stop semantics
```

### 8.2 API

```json
{
  "schema_version": "transition_request_v1",
  "task_id": "TASK-001",
  "from_state": "deciding",
  "to_state": "patch_checking",
  "event": "decision_accept_patch",
  "request_id": "REQ-001"
}
```

Result:

```json
{
  "schema_version": "transition_result_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "decision": "allow",
  "from_state": "deciding",
  "to_state": "patch_checking",
  "reasons": []
}
```

### 8.3 Hard Requirements

```text
1. Transition validation happens before side effects.
2. Illegal transition fails closed.
3. Terminal states reject mutation operations.
4. waiting_for_approval blocks mutation even if CI succeeds.
5. complete is forbidden while required verification gaps exist.
6. Counters are monotonic unless task is explicitly reopened.
```

### 8.4 Enforcement Examples

```text
reasonix_result_pending -> patch_applying: deny
waiting_for_approval -> editing without approval: deny
verifying -> complete with required gap: deny
stopped_by_limit -> delegating_to_reasonix: deny
```

## 9. Schema Enforcement Layer

### 9.1 Responsibility

Schema Enforcement Layer enforces [../schemas/coasonix-v1.schema.json](../schemas/coasonix-v1.schema.json) using JSON Schema Draft 2020-12.

It owns:

```text
input validation
output validation
schema_version matching
strict additionalProperties handling
error_result_v1 shaping
compatibility shim decision
```

### 9.2 API

```json
{
  "schema_version": "schema_validation_request_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "expected_schema": "performance_review_v1",
  "payload": {}
}
```

Result:

```json
{
  "schema_version": "schema_validation_result_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "expected_schema": "performance_review_v1",
  "valid": false,
  "errors": [
    {
      "path": "/confidence",
      "message": "must be <= 1"
    }
  ]
}
```

### 9.3 Hard Requirements

```text
1. Invalid tool input blocks tools/call.
2. Invalid Reasonix output blocks Codex decision.
3. Invalid error result is fatal wrapper error.
4. output_schema must match returned schema_version.
5. Unknown schema_version fails unless explicit shim exists.
6. Shim must emit schema_shim_applied audit event.
```

### 9.4 Fail-Closed Cases

```text
missing task_id
request_id mismatch
confidence outside 0..1
unknown schema_version
unexpected top-level field
patch proposal without files_changed
performance_review without benchmark_plan
```

## 10. Policy Execution Engine

### 10.1 Responsibility

Policy Execution Engine turns `.agent/policy.yaml` and safety rules into runtime gates.

It owns:

```text
path allowlist / denylist
permission level enforcement
network constraints
shell constraints
patch approval rules
human approval triggers
cache reuse eligibility
Reasonix execution mode authorization
```

### 10.2 API

```json
{
  "schema_version": "policy_evaluation_request_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "operation": "run_shell",
  "permission_level": "L1_DIFF_REVIEW",
  "resources": {
    "paths": [".agent/logs/TASK-001.test.log"],
    "command": ["git", "diff"]
  }
}
```

Result:

```json
{
  "schema_version": "policy_evaluation_result_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "decision": "allow",
  "reasons": [],
  "requires_human_approval": false
}
```

### 10.3 Hard Requirements

```text
1. Denylist wins over allowlist.
2. Path normalization happens before policy match.
3. Shell policy evaluates argv, not raw command string.
4. Network default is deny.
5. L2_PATCH_ONLY may produce patch data but not write Codex worktree.
6. L3_ISOLATED_WORKTREE may write only isolated worktree.
7. L4_DIRECT_WRITE is unavailable.
8. High-risk policy matches return require_approval, not allow.
```

### 10.4 Runtime Gates

| Operation | Required Policy Checks |
|---|---|
| `call_reasonix_tool` | allowed tool, permission level, artifact paths, budget |
| `route_reasonix_project` | tenant/user, realpath repo root, worktree, base branch, config hash, policy hash, schema family, runtime version |
| `route_reasonix_session` | project_key, session_key, task_namespace, lane, permission, policy hash, runtime version |
| `freeze_snapshot` | base revision, artifact hashes, read scope |
| `write_worktree` | worktree write lock, task namespace, state, policy |
| `read_artifact` | read allowlist, read denylist, path normalization |
| `write_artifact` | write allowlist, write denylist |
| `run_shell` | shell allowlist, argv parser, permission level |
| `open_network` | network allowlist, approval state |
| `apply_patch_transaction` | patch safety report, approval triggers, transaction state |
| `reuse_cache_result` | schema, policy hash, projection hash, runtime version |

## 11. Runtime Composition Rules

### 11.1 Ordering

```text
1. Schema request validation
2. State gate
3. Policy gate
4. Side effect
5. Schema result validation
6. State commit
7. Audit write
```

State and policy must be checked before side effects. Result schema validation must happen before state commit.

### 11.2 Decision Merge

If engines disagree:

```text
deny beats require_approval
require_approval beats allow
retryable_error beats allow
fatal_error beats all except deny caused by explicit policy
```

### 11.3 Audit

Every runtime decision emits:

```text
runtime_decision_recorded
```

Denied decisions also emit:

```text
runtime_denied
```

## 12. Minimal Runtime Kernel Interface

```json
{
  "schema_version": "runtime_operation_request_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "operation": "call_reasonix_tool",
  "expected_state": "testing",
  "next_state": "delegating_to_reasonix",
  "permission_level": "L1_DIFF_REVIEW",
  "payload_schema": "review_result_v1",
  "routing": {
    "project_key_hash": "sha256:...",
    "session_key_hash": "sha256:...",
    "task_namespace": "TASK-001:CODEX-001:worktree-001:abc123",
    "codex_session_id": "CODEX-001",
    "snapshot_id": "SNAP-001",
    "base_revision": "abc123",
    "lane": "review",
    "static_prefix_hash": "sha256:..."
  },
  "resources": {
    "tool_name": "reasonix.review_diff",
    "paths": [
      ".agent/context/TASK-001.context.md",
      ".agent/diffs/TASK-001.codex.diff"
    ]
  }
}
```

Runtime decision:

```json
{
  "schema_version": "runtime_decision_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "operation": "call_reasonix_tool",
  "decision": "allow",
  "engine_results": {
    "schema": "allow",
    "state": "allow",
    "policy": "allow"
  },
  "reasons": [],
  "audit_event_id": "AUD-001"
}
```

## 13. Runtime Error Codes

```text
runtime_schema_invalid_request
runtime_schema_invalid_result
runtime_state_transition_denied
runtime_policy_denied
runtime_human_approval_required
runtime_budget_exceeded
runtime_patch_transaction_denied
runtime_cache_reuse_denied
runtime_project_route_denied
runtime_session_route_denied
runtime_snapshot_mismatch
runtime_task_namespace_mismatch
runtime_worktree_write_lock_denied
runtime_reasonix_memory_used_as_evidence
runtime_unknown_operation
runtime_engine_failure
```

## 14. Bootstrap Order

Safe runtime construction order:

```text
1. Schema Enforcement Layer
2. State Machine Runtime Engine
3. Policy Execution Engine
4. Audit writer
5. Runtime Kernel composition
6. Read-only reasonix.review_diff path
7. Reasonix Project / Session Lane Router
8. Context Projector integration
9. Parallel read-only fan-out
10. Patch Safety Checker
11. Patch Transaction Model
12. performance_review benchmark/profiling enforcement
```

Patch generation must remain disabled until steps 1-11 are implemented.

## 15. Runtime Conformance Tests

Minimum test cases:

```text
1. illegal transition is denied before side effect
2. invalid Reasonix output blocks Codex decision
3. path traversal is denied before file read
4. denylist beats allowlist
5. shell argv not in allowlist is denied
6. network request denied by default
7. waiting_for_approval blocks patch apply
8. complete blocked by required verification gap
9. schema shim emits audit event
10. cached result rejected after policy_hash change
11. patch apply rejected without patch_safety_report_v1 pass
12. performance claim remains unverified without benchmark/profiling evidence
13. patch tool cannot route to read-only session lane
14. cross-lane result dependency rejected unless explicit artifact/task_state input exists
15. task namespace mismatch invalidates Reasonix result
16. snapshot mismatch invalidates Reasonix result
17. patch transaction denied while worktree write lock is held
18. cross-project session reuse attempt is denied
19. cross-project result cache hit is denied
20. same-worktree write attempts serialize through worktree write lock
21. Reasonix memory/history cannot satisfy verification evidence requirement
22. MVP session_key includes task_id and prevents cross-task lane reuse
```

## 16. Framework Status After This Layer

With this layer specified:

```text
Design direction: complete
Deterministic runtime spec: complete
Runtime enforcement design: complete
Safe operation: blocked until implementation and conformance tests exist
```

The system can be implemented against this runtime contract. It should not be operated in autonomous patch-generating mode until Runtime Enforcement Layer conformance tests pass.
