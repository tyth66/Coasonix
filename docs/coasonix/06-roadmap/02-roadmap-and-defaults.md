# Roadmap and Defaults (Historical Design Reference)

This document records the original roadmap and configuration defaults from the
design phase. It is preserved for reference but does not represent the current
implementation scope. See
[../../implementation/review-diff-agent-collaboration-plan.md](../../implementation/review-diff-agent-collaboration-plan.md)
for the active forward plan.

## v1 MVP (Implemented)

The v1 MVP is implemented with:

```text
Rust Runtime Core (state + policy + audit)
Rust JSON-RPC stdio Runtime Worker
TypeScript MCP Adapter
mock Reasonix review_diff vertical slice
repo-local SQLite at .agent/coasonix.sqlite
```

MVP engineering defaults (all implemented):

```text
1. local single-machine STDIO transport.
2. TypeScript MCP Adapter manages one Rust Runtime Worker.
3. v1 exposes reasonix.review_diff only.
4. patch proposal is disabled until patch safety gates exist.
```

## Post-v1 (Not Implemented)

```text
reasonix.propose_patch
patch safety checker
verification runner
human approval gate
context projector
remote transport / HTTP
local daemon
```

## Recommended Configuration Defaults (Design Reference)

These YAML defaults are the design-level configuration model. They are not a
runtime configuration file.

```yaml
system:
  architecture: codex_centered_expert_delegation
  primary_agent: codex
  expert_agent: reasonix
  communication:
    control_plane: mcp
    mcp_server: reasonix_expert
    data_plane: git_files_logs
    result_contract: json_schema_structured_content

codex:
  role: orchestrator_executor
  owns_final_decision: true
  must_validate_reasonix_output: true
  may_apply_patch: true
  may_run_tests: true

reasonix:
  role: expert_tool
  invoked_by: codex
  directly_invoked_by_user: false
  default_permission: readonly
  may_directly_modify_codex_worktree: false
  output_format: structured_json
  project_session_routing:
    project_controller_scope: repo_worktree_config_policy_runtime
    task_namespace_isolation: true
    mvp_session_lane_scope: task
    reasonix_memory_evidence_level: hypothesis_only
    transport_mvp: stdio
    default_lanes:
      review:     [reasonix.review_diff, reasonix.test_plan]
      security:   [reasonix.security_audit]
      debug:      [reasonix.debug_hypothesis]
      performance:[reasonix.performance_review]
      architecture:[reasonix.architecture_options]
      patch:      [reasonix.propose_patch]

mcp:
  server_name: reasonix_expert
  transport: stdio
  expose_tools_only: true
  expose_sampling: false
  expose_elicitation: false

limits:
  max_reasonix_calls_per_task: 3
  max_total_rounds: 6
  max_patch_attempts: 3
  max_test_failure_rounds: 3
  max_runtime_minutes_per_task: 30
  max_runtime_minutes_per_reasonix_call: 10

validation:
  require_input_schema_validation: true
  require_output_schema_validation: true
  require_patch_safety_check: true
  require_audit_log: true
```

## Final Principles

```text
Codex is the controller.
Reasonix is the expert.
Wrapper is the protocol adapter.
MCP is the control plane.
Git / diff / files / logs carry facts.
JSON Schema is the result contract.
CI is the verification layer.
Policy is the boundary.
Audit is traceability.
Human approval is the high-risk safety net.
```
