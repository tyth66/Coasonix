# Patch Transaction Model

> **设计规格（Design Specification）**：此文档描述的是 post-v1 patch 事务模型。
> 当前 v1 只支持只读 `reasonix.review_diff`，不涉及任何 patch 生成、
> 安全检查、dry-run、apply、rollback。代码中无 patch 相关实现。

本文件定义 Reasonix patch 从 proposal 到 apply、verify、rollback 的事务语义，防止半应用状态。

## 1. Transaction States

```text
proposed
safety_checked
dry_run_passed
applying
applied
verifying
committed
rolled_back
failed
rejected
```

## 2. Atomicity Rule

Patch application must be atomic at the task level.

Allowed atomic strategies:

```text
1. apply to temporary worktree, then promote diff
2. create pre-apply git snapshot, apply patch, rollback on failure
3. apply patch as isolated commit, revert commit on verification failure
```

Forbidden:

```text
partial file application
manual compensation without audit record
continuing after apply failure with dirty unknown state
mixing multiple Reasonix patches in one transaction
```

## 3. Transaction Flow

```text
patch_proposal_v1
-> patch_safety_report_v1
-> dry_run_apply
-> apply
-> verification_result_v1
-> commit_or_rollback
```

## 4. Rollback Semantics

Rollback mode must be explicit:

```text
reset_to_snapshot
revert_transaction_commit
discard_isolated_worktree
manual_repair_required
```

Rules:

```text
1. rollback must restore pre-transaction tracked file state.
2. untracked files created by patch must be removed or recorded.
3. rollback failure moves task to failed and requires human inspection.
4. verification failure defaults to rollback unless policy says keep_dirty_for_debug.
5. rollback emits audit event patch_rolled_back.
```

## 5. Partial Apply

```text
partial_apply_allowed: false
```

If a patch cannot apply cleanly, Codex may use it as reference, but must create a new Codex-authored change and separate transaction.

## 6. Transaction Record

```json
{
  "schema_version": "patch_transaction_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "transaction_id": "PATCH-TXN-001",
  "state": "applied",
  "patch_hash": "sha256:...",
  "base_revision": "abc123",
  "files_changed": [],
  "rollback_mode": "revert_transaction_commit",
  "verification_required": true
}
```

## 7. Failure Modes

```text
patch applies partially
verification fails but dirty state remains
rollback deletes user changes
two patch transactions overlap
patch safety checked against different base revision
```

## 8. Hard Requirements

```text
1. Only one patch transaction may be applying per task.
2. patch_hash and base_revision must be recorded before apply.
3. safety check and dry-run must use the same base_revision as apply.
4. successful apply does not imply complete.
5. commit is allowed only after required verification passes.
```

