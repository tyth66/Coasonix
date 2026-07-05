# Global Task State Machine Spec

> **实现状态**：此文档描述的任务状态机已在 `crates/coasonix-runtime-core/src/state/mod.rs`
> 中实现。状态为 Created → Running → Completed/Failed。
> `waiting_for_approval` 和 `stopped_by_limit` 状态是 post-v1 设计，
> 当前代码未使用。

本文件定义 Coasonix 的全局任务状态机。它统一 task state、Codex decision、human approval、verification、loop limiter 和 patch transaction 的状态语义，避免各模块“局部正确、全局不一致”。

## 1. State Model

每个任务必须且只能处于一个 top-level state。

```text
created
planning
editing
testing
delegating_to_reasonix
reasonix_result_pending
deciding
patch_checking
patch_applying
verifying
waiting_for_approval
stopped_by_limit
failed
complete
cancelled
```

## 2. Terminal States

```text
complete
failed
cancelled
stopped_by_limit
```

`waiting_for_approval` 不是 terminal state。它是 blocking state，必须等待 approval result 后才能继续或转入 failed/cancelled。

## 3. Allowed Transitions

| From | To | Required Event |
|---|---|---|
| created | planning | task_started |
| planning | editing | plan_recorded |
| editing | testing | codex_change_recorded |
| testing | delegating_to_reasonix | delegation_required |
| testing | verifying | tests_passed_without_delegation |
| testing | editing | tests_failed_self_debug |
| delegating_to_reasonix | reasonix_result_pending | reasonix_called |
| reasonix_result_pending | deciding | reasonix_result_received |
| reasonix_result_pending | deciding | reasonix_timeout |
| deciding | editing | decision_accept_without_patch |
| deciding | patch_checking | decision_accept_patch |
| deciding | verifying | decision_reject_or_noop |
| deciding | waiting_for_approval | human_approval_required |
| patch_checking | patch_applying | patch_safety_pass |
| patch_checking | deciding | patch_safety_fail |
| patch_applying | testing | patch_applied |
| patch_applying | deciding | patch_apply_failed |
| verifying | complete | required_verification_passed |
| verifying | editing | verification_failed_recoverable |
| verifying | waiting_for_approval | verification_gap_requires_approval |
| any non-terminal | stopped_by_limit | limit_reached |
| any non-terminal | failed | unrecoverable_error |
| any non-terminal | cancelled | user_cancelled |

## 4. Forbidden Transitions

```text
created -> complete
delegating_to_reasonix -> patch_applying
reasonix_result_pending -> complete
deciding -> complete
patch_checking -> complete
patch_applying -> complete
waiting_for_approval -> editing without approval_received
stopped_by_limit -> editing without explicit reopen
failed -> editing without explicit reopen
complete -> editing
```

## 5. Decision Semantics

Codex decisions are sub-state events inside `deciding`.

```text
accept
partial_accept
reject
ask_human
retry_self
retry_reasonix_once
stop_by_limit
```

Rules:

```text
1. accept and partial_accept require a decision record.
2. partial_accept must list accepted and rejected finding IDs.
3. reject must record rejection rationale.
4. retry_reasonix_once is allowed only if budget remains.
5. ask_human moves task to waiting_for_approval.
```

## 6. Verification Gap Semantics

A verification gap is allowed only as non-terminal state metadata.

```text
allowed_terminal_with_required_verification_gap: false
allowed_terminal_with_optional_verification_gap: true
```

Rules:

```text
1. complete is forbidden if any required verification gap remains open.
2. optional verification gaps may exist in complete only if explicitly recorded as non-blocking.
3. performance claims always require benchmark/profiling evidence or remain unverified.
4. security-sensitive claims require targeted evidence or human approval.
```

## 7. Waiting for Approval Semantics

`waiting_for_approval` is blocking for mutating actions.

Allowed while waiting:

```text
read-only audit inspection
read-only log collection
read-only status reporting
non-mutating CI result polling for already-started jobs
```

Forbidden while waiting:

```text
new code edits
new patch application
new Reasonix calls that expand scope
new CI jobs that mutate state
deployment
merge
publish
```

## 8. CI Concurrency Rule

CI jobs started before entering `waiting_for_approval` may finish and be recorded. Their result must not automatically unblock the task. Approval remains required.

## 9. State Object

```json
{
  "schema_version": "task_state_v1",
  "task_id": "TASK-001",
  "state": "deciding",
  "round": 2,
  "reasonix_calls": 1,
  "patch_attempts": 0,
  "test_failure_rounds": 1,
  "required_verification_gaps": [],
  "optional_verification_gaps": [],
  "approval": {
    "status": "not_required",
    "request_id": null
  },
  "last_transition": {
    "from": "reasonix_result_pending",
    "to": "deciding",
    "event": "reasonix_result_received"
  }
}
```

## 10. Enforcement Requirements

```text
1. Every state transition must emit audit_event_v1.
2. Transition validation happens before side effects.
3. Illegal transition must fail closed.
4. State counters must be monotonic unless task is explicitly reopened.
5. Completion requires state=complete and required verification gaps empty.
```

