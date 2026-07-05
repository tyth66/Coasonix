# Patch Safety Checker

> **设计规格（Design Specification）**：此文档描述的是 post-v1 patch 安全检查器。
> 当前 v1 不涉及 patch。代码中无对应实现。

Patch Safety Checker determines whether a Reasonix-proposed unified diff is eligible for Codex-controlled application. It does not prove correctness; it only permits Codex to attempt a patch transaction.

## 1. Inputs

```text
patch text
files_changed
permission_level
scope allowlist
scope denylist
sensitive path rules
task policy
base_revision
snapshot_id
```

## 2. Required Checks

```text
1. parse unified diff
2. verify patch applies cleanly in dry-run
3. normalize every path under repo root
4. reject absolute paths and traversal
5. reject symlink escape and case-folding bypass
6. enforce denylist before allowlist
7. reject secrets and credential-like additions
8. reject production deploy / publish commands
9. detect CI / policy / security config changes
10. detect test deletion, skip, assertion weakening, benchmark removal
11. require benchmark/profiling plan before performance claims become evidence
12. produce patch_safety_report_v1
13. audit result with task_id and request_id
```

## 3. Sensitive Paths

Sensitive files are denied by default:

```text
.env
.env.*
secrets.*
*.pem
*.key
.github/workflows/*
.codex/config.toml
.agent/policy.yaml
package publishing config
deployment config
terraform state
kubernetes production manifests
```

## 4. Output Contract

```json
{
  "schema_version": "patch_safety_report_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "status": "ok",
  "verdict": "pass",
  "files_checked": [],
  "blocked_reasons": [],
  "requires_human_approval": false
}
```

## 5. Hard Requirements

```text
1. Patch checker must not rely on ad hoc string slicing.
2. Denylist always wins.
3. Checker failure blocks patch application.
4. Checker pass does not prove correctness.
5. Checker output must be audit logged.
6. Reasonix must not write Codex worktree, run git apply, commit, push, or merge.
7. Future L3_ISOLATED_WORKTREE may allow Reasonix experiments only in isolated worktree; Codex still controls adoption.
```

## 6. Relationship to Patch Transaction

Patch Safety Checker runs before `01-patch-transaction-model.md`. A transaction may start only after a schema-valid `patch_safety_report_v1` with `verdict=pass` or an explicit human approval path.
