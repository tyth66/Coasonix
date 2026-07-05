# Human Approval Gate

> **设计规格（Design Specification）**：此文档描述的是 post-v1 人工审批门禁。
> 当前 v1 不涉及高风险变更，无审批流程。代码中无对应实现。

Human Approval Gate is the mandatory blocking state for high-risk changes. It is not a warning.

## 1. Approval Triggers

```text
auth_core_changes
payment_changes
database_migrations
deployment_changes
ci_changes
secret_access
network_access
deleting_tests
security_policy_changes
high_risk_codex_reasonix_conflict
automatic_loop_limit_reached
```

The original safety policy also requires approval for:

```text
modifying authentication / authorization core logic
modifying billing logic
modifying deployment configuration
touching secrets or environment configuration
Reasonix and Codex high-risk disagreement
```

## 2. Waiting State Semantics

`waiting_for_approval` blocks mutation until an approval result is recorded.

Forbidden:

```text
waiting_for_approval -> editing without approval_received
waiting_for_approval -> patch_applying without approval_received
waiting_for_approval -> complete without required decision/evidence
```

## 3. Output Contract

```json
{
  "schema_version": "human_approval_request_v1",
  "task_id": "TASK-001",
  "reason": "database_migration",
  "requested_action": "apply_patch",
  "risk_summary": "...",
  "artifacts": [],
  "status": "pending"
}
```

## 4. Hard Requirements

```text
1. Approval gate is a blocking state, not a warning.
2. Request must include risk summary and artifacts.
3. Approval must be recorded before high-risk action proceeds.
4. Denial must be recorded and must prevent the action.
5. Human approval result must be audit logged.
```
