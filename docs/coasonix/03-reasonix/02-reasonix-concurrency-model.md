# Reasonix Concurrency Model

> **设计规格（Design Specification）**：此文档描述的是 post-v1 并行调用策略。
> 当前 v1 实现仅支持单次串行 `reasonix.review_diff` 调用，不存在 fan-out、
> snapshot、merge、lane routing 等机制。代码中无对应实现。

本文件定义 Reasonix 调用的并发策略。默认模型是 controlled parallel fan-out with serial commit：分析可以并行，状态变更和 patch 事务必须串行。

## 1. Concurrency Modes

```text
strict_serial
controlled_parallel_readonly
serial_mutation
```

Default:

```text
analysis_mode: controlled_parallel_readonly
mutation_mode: serial_mutation
```

## 2. Parallel-Allowed Tools

The following tools may run in parallel if all use read-only permission and route to compatible session lanes under the same Reasonix Project:

```text
reasonix.review_diff
reasonix.security_audit
reasonix.performance_review
reasonix.test_plan
```

Conditional:

```text
reasonix.debug_hypothesis may run in parallel only if it reads fixed artifacts and does not depend on fresh test attempts.
reasonix.architecture_options should usually run alone because it shapes higher-level direction.
```

Forbidden parallel mutation:

```text
reasonix.propose_patch
patch_safety_check
patch_apply
verification state transition
human approval state transition
```

Parallel read-only calls should normally use separate lanes:

```text
review lane
security lane
performance lane
```

`reasonix.review_diff` and `reasonix.test_plan` may share the `review` lane when they use the same model, permission level, schema family, and static prefix. Security, performance, debug, architecture, and patch flows should not share a session lane by default.

## 3. Snapshot Rule

Parallel Reasonix calls must read the same immutable task snapshot:

```json
{
  "snapshot_id": "SNAP-001",
  "task_id": "TASK-001",
  "base_revision": "abc123",
  "artifact_hashes": {
    "context": "sha256:...",
    "diff": "sha256:...",
    "test_log": "sha256:..."
  }
}
```

They may reuse the same `project_key`, but each lane must record its own `session_key`, `request_id`, and result artifact path.

## 4. Merge Semantics

Parallel outputs are merged by Codex, not by Reasonix.

Merge order:

```text
1. schema validation
2. request_id matching
3. artifact snapshot matching
4. finding ID namespace normalization
5. duplicate finding merge
6. severity max
7. conflict detection
8. Codex decision gate
```

## 5. Conflict Rules

```text
1. security blocker overrides performance note.
2. patch proposal cannot be generated from conflicting parallel findings without Codex decision.
3. high-risk Codex/Reasonix disagreement moves to Human Approval Gate.
4. conflicting findings must remain visible in audit.
```

## 6. Task State Consistency

```text
1. Parallel calls may not mutate task_state.
2. Each call may write result artifact only under its request_id.
3. Codex performs one serial merge transition after all parallel calls settle or timeout.
4. Timeout of one optional parallel call does not invalidate other successful calls.
5. Timeout of a required parallel call moves result set to partial.
```

Each parallel result path must be task/request scoped:

```text
.agent/results/TASK-001/REQ-001.review_diff.json
.agent/results/TASK-001/REQ-002.security_audit.json
.agent/results/TASK-001/REQ-003.performance_review.json
```

### 6.1 Task Namespace Isolation

Multiple Codex sessions may share one Reasonix Project Controller, but each task must use a separate namespace:

```text
task_namespace =
  task_id
  + codex_session_id
  + branch_or_worktree_id
  + base_revision
```

Rules:

```text
1. A result from one task namespace cannot be consumed by another task unless Codex explicitly projects it.
2. Every Coasonix result artifact and audit record must include task_id, request_id, snapshot_id, lane, and base_revision. Reasonix review content should not be forced to carry routing metadata.
3. Codex merge must reject task_id, request_id, snapshot_id, lane, or base_revision mismatches.
```

## 7. Fan-Out Budget

```yaml
max_parallel_reasonix_calls: 3
parallel_call_timeout_policy: settle_all_or_timeout
parallel_results_status: ok | partial | timeout | error
```

## 8. Locking Rules

```text
Project-level read lock:
  multiple read-only lanes may run concurrently.

Task-level mutation lock:
  patch lane, patch safety check, and patch transaction are exclusive per task namespace.

Worktree write lock:
  any write, patch apply, isolated worktree promotion, or rollback is exclusive per worktree.
```

Simple rule:

```text
review/security/performance/test_plan may run together.
propose_patch waits for unresolved read-only fan-out to settle or timeout.
patch_apply is performed only by Codex / Runtime, never directly by Reasonix.
```

## 9. Hard Requirements

```text
1. No two Reasonix calls may write the same artifact path.
2. No patch transaction may run while parallel analysis calls are unresolved.
3. Snapshot mismatch invalidates the call result.
4. Codex merge decision must be audit logged.
5. Parallel lane routing must not create hidden cross-tool memory dependencies.
6. Patch-capable tools must not share a session lane with read-only review tools unless policy explicitly allows it.
7. Task namespace mismatch invalidates the call result.
8. Worktree write lock is required before any patch transaction can start.
```
