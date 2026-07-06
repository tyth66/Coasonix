# Machine-Executable Schema Contract and Schema Versioning

> **Current status**: The schema file at `schemas/coagent-v1.schema.json` is a
> **test contract fixture** for the active `reasonix.review_diff` tool. It is
> not loaded at runtime. The MCP adapter performs a narrow local
> `review_result_v1` contract check in `worker-contract.ts:reviewResultSchemaError()`.
> Rust `schema/mod.rs` exists for standalone validation + duplicate-key detection
> tests but is not in the live call path.

This document defines the Coagent test contract Schema layer.

## 1. Canonical Schema Location

```text
../../../schemas/coagent-v1.schema.json
```

This file is the v1 `reasonix.review_diff` test contract anchor.

## 2. Active Schemas (Implemented)

```text
review_diff_input_v1     — MCP tool input contract (goal, repo, artifacts, focus, constraints, budget)
review_result_v1         — Reasonix output contract (verdict, summary, findings, tests_to_run, risks, assumptions, confidence)
```

Supporting `$defs`:
```text
taskId, requestId, relativePath, confidence, finding, metadata
```

## 3. Strict Validation Rules

```text
1. additionalProperties = false for top-level objects.
2. schema_version uses const (exact match required).
3. status uses enum: ok, partial, error, timeout.
4. verdict uses enum: pass, needs_fix, risky, unknown, not_applicable.
5. severity uses enum: blocker, major, minor, note.
6. confidence is number 0..1.
7. file paths use relativePath $def (no absolute, no traversal).
8. task_id pattern: ^TASK-[A-Za-z0-9_-]+$
9. request_id pattern: ^REQ-[A-Za-z0-9_-]+$
```

## 4. Adapter Behavior (Implemented)

```text
1. inputSchema validation fails -> error code returned, Reasonix not invoked.
2. output contract validation fails -> worker_schema_invalid error.
3. Adapter validates review result in worker-contract.ts:reviewResultSchemaError().
4. Schema fixture validated at build time: python -m json.tool schemas/coagent-v1.schema.json.
```

## 5. Version Evolution Rules (Design Spec — Post-v1)

### 5.1 Patch Version

Patch-level schema revisions may:
```text
add optional fields
tighten descriptions
add enum values only if older consumers ignore unknown enum values explicitly
```

Patch-level revisions must not:
```text
remove required fields
rename fields
change field type
change status or verdict semantics
```

### 5.2 Minor / Major Versions

Post-v1. No multi-version coexistence exists in v1.

## 6. Transitional Note

The current `review_result_v1` schema still requires envelope fields
(`schema_version`, `task_id`, `request_id`, `status`) that are targeted
for removal in the active plan (Task 2 of
`docs/implementation/review-diff-agent-collaboration-plan.md`).
The pure-review target result would not require these fields.
