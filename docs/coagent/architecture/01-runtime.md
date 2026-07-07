# Runtime: State, Policy, Audit (v2)

The Rust RuntimeKernel runs in-process inside the MCP server binary.
No JSON-RPC subprocess.

## State Machine (10-state FSM)

```
                ┌──────────────────────────┐
                │         Queued           │ ← entry point
                └────────────┬─────────────┘
                             │
    ┌────────────────────────┼────────────────────────┐
    │                        ▼                        │
    │              ┌─────────────────┐                │
    │              │    Running      │◄───────────┐   │
    │              └───────┬─────────┘            │   │
    │                      │                      │   │
    │         ┌────────────┼────────────┐         │   │
    │         ▼            ▼            ▼         │   │
    │   ┌──────────┐ ┌──────────────┐ ┌────────┐ │   │
    │   │ Blocked  │ │WaitingApproval│ │Retrying│─┼───┘ (retry dispatched)
    │   └────┬─────┘ └──────┬───────┘ └────────┘ │
    │        │              │                     │
    │        │ (unblock)    │ (approved/rejected) │
    │        ▼              ▼                     │
    │   Running ◄── Running | Failed              │
    │                                             │
    │   ┌──────────────────────┐                  │
    │   │ PartiallyCompleted   │                  │
    │   └──────────┬───────────┘                  │
    │              │ (all subtasks done)          │
    │              ▼                              │
    │   ┌──────────────────┐                     │
    └──►│    Completed      │ (terminal)          │
        └──────────────────┘                     │
                                                 │
        ┌─────────────────┐                      │
        │     Failed      │ (terminal) ◄─────────┘
        └─────────────────┘
        ┌─────────────────┐
        │    Cancelled    │ (terminal) ◄── any alive state
        └─────────────────┘
```

### Subtask Dependencies

Tasks can declare subtask dependencies that block completion:

```rust
state.add_subtask("SUB-1", TaskStateValue::Completed);
state.add_subtask("SUB-2", TaskStateValue::Completed);
// transition_to(Completed) rejected until all subtasks resolved
state.resolve_subtask("SUB-1");
state.resolve_subtask("SUB-2");
state.transition_to(TaskStateValue::Completed).unwrap();
```

### Timeout & Retry

```rust
state.set_timeout(TaskTimeout {
    max_duration: Duration::from_secs(3600),
    max_blocked_duration: Duration::from_secs(600),
    max_approval_duration: Duration::from_secs(1800),
    max_retries: 3,
});
```

### Cancel Propagation

Cancelling a parent task cascades to subtasks when `cancel_propagation` is enabled (default true).

## Policy Engine (v2)

### Dynamic Tool Registry

Thread-safe runtime registry supports:

- `register()` — compile-time registration (builder pattern)
- `register_dynamic()` — runtime addition
- `unregister()` — runtime removal
- `enable()` / `disable()` — toggle without removing
- `upgrade()` — version-bump in-place replacement
- `list_enabled()` — enumerate active tools
- `snapshot()` — read-consistent view for PolicyEngine init

### Approval Gates

`ApprovalPolicy::Required` causes `PolicyEngine::evaluate()` to return `RequireApproval`.
The MCP server gates execution: returns `{"status": "approval_required", ...}` before backend invocation.
Caller must transition the task from `WaitingApproval` back to `Running` to proceed.

### Permission Levels

- `L0_READONLY` — read-only observation
- `L1_DIFF_REVIEW` — read diffs + context, write results
- `L2_PATCH_ONLY` — generate patches
- `L3_ISOLATED_WORKTREE` — full worktree access

### Runtime Decisions

`Allow | Deny | RequireApproval | RetryableError | FatalError`

Merge priority: `Deny > FatalError > RequireApproval > RetryableError > Allow`

## Execution Sandbox

`SandboxConfig` provides:

- **Working directory** control for backend processes
- **Environment variable allowlist/denylist** — empty allowlist = deny all
- **Resource budgets**: wall-clock duration, output bytes, token budget, CPU time

```rust
let sandbox = SandboxConfig::new()
    .with_working_directory("./task-workspace")
    .with_env_allowlist(vec!["PATH".into(), "HOME".into()])
    .with_budgets(ResourceBudgets {
        max_wall_clock: Some(Duration::from_secs(60)),
        max_output_bytes: Some(1_048_576),
        ..Default::default()
    });
```

## Event-Sourcing Replay

`replay_task_state()` rebuilds task state from the append-only event log:

```rust
let replayed = replay_task_state(&store, "TASK-1")?;
// replayed.steps_started, replayed.steps_completed,
// replayed.policy_decisions, replayed.last_decision
```

`check_idempotency()` prevents duplicate event emission for the same logical operation.

## Schema Authority

`SchemaRegistry` is the single validation authority. The handwritten `ReviewDiffInput::validate()` has been replaced with a passthrough that always returns `Ok(())` — all validation is routed through JSON Schema 2020-12 via the embedded `schemas/coagent-v1.schema.json`.

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

Each `evaluate_operation` creates a `runtime_steps` row and emits append-only `runtime_events`:

- `step_started`
- `policy_evaluated`
- `lifecycle_closed`

Denied policy decisions close their step immediately with the runtime decision value.
