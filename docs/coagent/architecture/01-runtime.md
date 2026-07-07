# Runtime: State, Policy, Audit

The Rust RuntimeKernel runs in-process inside the MCP server binary.
No JSON-RPC subprocess.

## State Machine

```
Created ────→ Running ────→ Completed
    │             │              │
    │             └──→ Failed ←──┘
    │
    └──→ Cancelled
```

Terminal states (Completed, Failed, Cancelled) reject all subsequent
`evaluate_operation` calls.

## Policy Engine

Registered tools are described by `ToolRegistry`. The default registry currently
contains `reasonix.review_diff` → `L1_DIFF_REVIEW`.

Permission levels:
- L0_READONLY — read-only observation
- L1_DIFF_REVIEW — read diffs + context, write results
- L2_PATCH_ONLY — generate patches (not implemented)
- L3_ISOLATED_WORKTREE — full worktree access (not implemented)

Tool capabilities define input/output schema names, path allowlists, write
allowlists, deny patterns, and network permission. `PolicyEngine` builds
per-tool artifact policies from this registry, including `..` traversal
rejection, symlink escape detection, and case-insensitive path checks on
Windows.

## Audit (SQLite)

12 tables in `.agent/coagent.sqlite`:
- `tasks`, `task_state` — task lifecycle
- `audit_events` — append-only event log (UPDATE/DELETE triggers reject mutations)
- `runtime_steps` — per-operation/request runtime step records
- `runtime_events` — append-only step/task event stream
- `runtime_decisions` — each evaluate_operation result
- `schema_validation_results` — schema check outcomes
- `policy_evaluation_results` — policy check outcomes
- `locks`, `artifacts`, `cache_entries`, `runtime_metadata`

WAL mode, FULL synchronous, 5s busy timeout.

## Lifecycle API

```rust
// Permission gate (called before every backend invocation)
kernel.evaluate_operation(request) -> RuntimeDecision { allow | deny | ... }

// Lifecycle closure (called after backend invocation)
kernel.complete_operation(task_id, request_id, operation) -> Completed
kernel.fail_operation(task_id, request_id, operation, error_code, message) -> Failed
```

## Step/Event Model

Each `evaluate_operation` creates a `runtime_steps` row for the operation and
emits append-only `runtime_events`:

- `step_started`
- `policy_evaluated`
- `lifecycle_closed`

Allowed steps remain `running` until `complete_operation` or `fail_operation`
closes them. Denied policy decisions close their step immediately with the
runtime decision value.
