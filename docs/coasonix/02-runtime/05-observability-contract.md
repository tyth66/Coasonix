# Observability Contract

> **设计规格（Design Specification）**：此文档描述的是 post-v1 可观测性系统。
> 当前 v1 的唯一可观测性机制是 SQLite 审计日志（`audit_events` 表，append-only）。
> 文档中描述的 metrics counters、tracing spans、debug hooks、SLO 阈值均未实现。
> Rust 代码中无 `tracing` crate，无 metrics 导出，无 span 构造。

Audit log explains what happened. Observability explains how the agent system behaves over time. Coasonix requires metrics, traces, and debugging hooks in addition to audit events.

## 1. Metrics

Required counters:

```text
tasks_started_total
tasks_completed_total
tasks_failed_total
tasks_stopped_by_limit_total
reasonix_calls_total
reasonix_timeouts_total
reasonix_schema_failures_total
reasonix_permission_denials_total
reasonix_project_controller_reuse_total
reasonix_session_lane_hits_total
reasonix_session_lane_misses_total
reasonix_session_route_denials_total
reasonix_snapshot_mismatches_total
patch_proposals_total
patch_rejections_total
patch_rollbacks_total
human_approval_requests_total
verification_failures_total
```

Required distributions:

```text
task_duration_seconds
reasonix_call_duration_seconds
reasonix_session_route_duration_seconds
codex_decision_latency_seconds
context_projection_size_bytes
context_projection_redaction_count
patch_safety_check_duration_seconds
verification_duration_seconds
cache_lookup_duration_seconds
```

Required ratios:

```text
reasonix_acceptance_rate
partial_acceptance_rate
patch_rejection_rate
verification_pass_rate
cache_hit_rate
session_lane_hit_rate
human_approval_rate
schema_failure_rate
```

## 2. Tracing

Every task trace must include:

```text
trace_id
task_id
request_id
codex_session_id
task_namespace
snapshot_id
base_revision
span_id
parent_span_id
component
operation
start_ts
end_ts
status
artifact_refs
```

Minimum spans:

```text
task_intake
context_projection
mcp_tools_call
reasonix_session_route
reasonix_execution
output_normalization
codex_decision
patch_safety_check
patch_transaction
verification
audit_write
```

## 3. Debugging Hooks

```text
dump_task_state(task_id)
dump_task_namespace(task_namespace)
dump_session_route(request_id)
dump_session_pool(project_key)
dump_snapshot(snapshot_id)
dump_projection(request_id)
dump_schema_errors(request_id)
dump_patch_transaction(transaction_id)
dump_decision_record(request_id)
dump_budget_state(task_id)
```

Hooks must redact secrets by default.

## 4. Cardinality Controls

Metrics labels may include:

```text
tool_name
status
verdict
permission_level
error_code
transport
schema_version
lane
project_key_hash
session_key_hash
```

Metrics labels must not include:

```text
file path
user prompt
raw error text
secret-like value
full request_id if high-cardinality backend cannot handle it
raw project_key or session_key
```

## 5. Observability Events vs Audit Events

```text
audit_event_v1: authoritative decision/fact chain
metric: aggregate health signal
trace span: latency and causality signal
debug hook: scoped diagnostic view
```

Observability data must not replace audit logs.

## 6. SLO Signals

Initial operational thresholds:

```text
schema_failure_rate > 2% -> investigate schema drift
reasonix_timeout_rate > 5% -> investigate timeout/budget
cache_hit_rate < 50% after warmup -> investigate prefix instability
session_lane_hit_rate < 70% after warmup -> investigate routing key drift
reasonix_snapshot_mismatches_total > 0 -> inspect concurrent task snapshot handling
patch_rejection_rate > 40% -> inspect patch prompt or checker strictness
human_approval_rate spike -> inspect scope classification
```
