# General Agent Runtime Gaps (v2 — partially resolved)

Coagent v1 was a constrained v1 gateway: one MCP tool, one primary operation, a
narrow runtime gate, and an audit-backed review workflow. The v2 architecture
refactor (2026-07-07) closed many of those gaps. This document records the
status of each deficit.

## Resolved Deficits ✓

### Task Model Is Too Flat ✓ RESOLVED

The task lifecycle has been expanded from 5 states to 10:

```
Queued → Running → Completed | Failed | Cancelled
                  + Blocked, WaitingApproval, Retrying, PartiallyCompleted
```

Added:
- **Subtask dependencies**: tasks declare subtask IDs + required states; completion blocked until all resolved
- **Timeout configuration**: per-task max_duration, max_blocked_duration, max_approval_duration
- **Retry support**: `max_retries` counter, `Retrying` state, `MaxRetriesExceeded` error
- **Cancel propagation**: cascading cancellation to subtask dependencies
- **Auto-progression**: `PartiallyCompleted` auto-transitions to `Completed` when all subtasks resolve

### Tool And Capability Model Is Hard-Coded ✓ RESOLVED

`ToolRegistry` is now a thread-safe, runtime-mutable registry (`Arc<RwLock<HashMap>>`):

- `register_dynamic()` — add tools at runtime
- `unregister()` — remove tools at runtime
- `enable()` / `disable()` — toggle without losing definition
- `upgrade()` — version-bump replacement with schema migration
- `list_enabled()` — enumerate active tools
- `snapshot()` — read-consistent view for PolicyEngine initialization

Each `ToolDefinition` carries: operation name, permission level, backend binding,
approval policy, input/output schema names, capabilities (read/write/deny/network),
enabled flag, and version number.

### Policy And Approval Are Not Composable Enough ✓ PARTIALLY RESOLVED

- **Approval gates**: `ApprovalPolicy::Required` now gates execution through the state machine.
  When enforced, the policy engine returns `RequireApproval`; the MCP server returns
  `{"status":"approval_required"}` without invoking the backend.
  Caller must transition the task from `WaitingApproval` → `Running` to proceed.

Remaining: dry-run/explanation modes, approval provenance tracking.

### Schema Enforcement Has Two Tracks ✓ RESOLVED

`SchemaRegistry` is now the single validation authority:

- Handwritten `ReviewDiffInput::validate()` removed — delegates to SchemaRegistry
- MCP input validation, output validation, and wrapper validation all route through the same schema
- JSON Schema 2020-12 via embedded `schemas/coagent-v1.schema.json`
- Duplicate-key detection (`parse_json_no_duplicate_keys`)
- Schema migration path via `$defs` registry

### Execution Isolation Is Still Shallow ✓ PARTIALLY RESOLVED

`SandboxConfig` added:

- **Working directory** control for backend processes
- **Environment variable allowlist/denylist** — empty allowlist = deny all
- **`filtered_env()`** — produces sanitized env from actual process environment
- **Resource budgets**: `max_wall_clock`, `max_output_bytes`, `max_tokens`, `max_cpu_time`

Remaining: per-task worktrees, command sandboxing, secret redaction, quarantine.

### Audit Is Not Yet Full Event Sourcing ✓ PARTIALLY RESOLVED

`replay` module added:

- `replay_task_state()` — rebuilds task execution summary from append-only event log
- `check_idempotency()` — prevents duplicate event emission for same logical operation
- `ReplayedTaskState` — steps_started, steps_completed, policy_decisions, last_decision

Remaining: full replay that reconstructs the exact FSM state (not just summary),
artifact creation/promotion events, retry/cancellation events.

### Scheduling And Concurrency Are Early

No change. Lock and cache metadata exist but no scheduler yet.
Multi-task queues, resource locks, deadlock avoidance, priority/fairness
remain future work.

### Agent Identity And Provenance Are Incomplete

No change. Caller identity, agent identity, approval provenance are not yet
machine-enforced.

### Observability Is Minimal

No change. Structured logs, tracing spans, task timelines, metrics remain
future work.

### Real Backend Reliability Still Needs Recovery Coverage

The mock backend and 5 ACP contract tests (fake stdio) remain the primary
reliability evidence. Timeout/cancellation, invalid-frame, process crash,
and long-lived session recovery tests are still needed for production hardening.

## Current Shape (v2)

```text
Codex MCP Host
  -> coagent-mcp-server (~5 MB)
      -> RuntimeKernel
          ├── 10-state FSM (Queued→Running→Completed|Failed|Cancelled)
          │   + Blocked, WaitingApproval, Retrying, PartiallyCompleted
          │   + subtask dependencies, timeout, cancel propagation
          ├── PolicyEngine
          │   + dynamic ToolRegistry (register/unregister/enable/disable/upgrade)
          │   + approval gates (RequireApproval → WaitingApproval)
          │   + path sandbox
          ├── Sandbox (env allowlist/denylist, resource budgets)
          ├── Replay (event-sourcing replay, idempotency check)
          └── Audit (SQLite 12 tables, WAL, append-only)
      -> Backend: Mock | Reasonix ACP
```

## Summary

| Gap | Status |
|-----|--------|
| Task model too flat | ✓ RESOLVED — 10-state FSM |
| Tool model hard-coded | ✓ RESOLVED — dynamic registry |
| Approval not composable | ✓ PARTIAL — RequireApproval gate |
| Schema dual-track | ✓ RESOLVED — single authority |
| Execution isolation | ✓ PARTIAL — SandboxConfig |
| Audit not event sourcing | ✓ PARTIAL — replay + idempotency |
| Scheduling/concurrency | Unchanged |
| Identity/provenance | Unchanged |
| Observability | Unchanged |
| Backend reliability | Unchanged (5 contract tests) |
