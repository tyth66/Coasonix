# Implementation Plan and Critical Node Definitions

本文保留原“关键节点定义与工程规格”的完整语义，并将其作为实现计划的节点级检查清单。它定义 Coasonix 中必须稳定、可审计、可验证的关键节点。

这些节点不是概念说明，而是实现边界。任何 `reasonix-expert` Wrapper、Codex 编排器、策略引擎、审计系统或 CI Adapter 的实现，都必须能映射到这些节点。

## 1. 节点总览

| ID | 节点 | 核心责任 | 失败后果 |
|---|---|---|---|
| N01 | Task Intake and Task State | 建立任务身份、目标、范围、状态对象 | 调用不可追踪，审计链断裂 |
| N02 | Codex Primary Control | 保持 Codex 最终裁决和执行权 | 双 Agent 平权失控 |
| N03 | Context Projector | 将 Codex 全局上下文裁剪为 Reasonix 投影上下文 | 泄露、误判、不可复现 |
| N04 | MCP Session and Transport | 建立 MCP 生命周期、传输和初始化边界 | 协议状态错乱 |
| N05 | Tool and Schema Registry | 声明可调用工具及严格 schema | 空工具、弱契约、结果不可验证 |
| N06 | Wrapper Input Gate | 校验 `tools/call` 输入、路径、权限、预算 | 越权读取、错误工具执行 |
| N07 | Reasonix Execution Gate | 受控启动 Reasonix 并隔离执行能力 | 终端/网络/文件系统失控 |
| N08 | Output Normalization Gate | 将 Reasonix 原始输出转成可信结构化结果 | prompt injection 进入 Codex |
| N09 | Codex Decision Gate | Codex 对 Reasonix 建议作采纳/拒绝/部分采纳 | 建议未经判断直接执行 |
| N10 | Patch Safety Checker | 检查候选 patch 是否可进入执行面 | 越权改动、删除测试、泄露 secrets |
| N11 | Verification Gate | 用测试、构建、benchmark 或人工证据验证结果 | 未验证结论被当成事实 |
| N12 | Audit Event Model | 记录完整事实链和决策链 | 无法复盘、无法归责 |
| N13 | Loop and Budget Limiter | 限制调用次数、轮数、时间、patch 尝试 | 自动循环失控 |
| N14 | Human Approval Gate | 高风险事项强制人工审批 | 高风险变更自动落地 |
| N15 | Performance Review Gate | 约束性能审查结论和验证方式 | 性能猜测被当作已验证提升 |
| N16 | Reasonix Project Controller and Session Router | 复用项目级控制器、隔离 task namespace、路由 cache-stable lanes | 跨 Codex 会话串扰、cache 失效、隐式记忆污染 |

## 2. 全局不变量

以下不变量跨越全部节点：

```text
1. Codex owns final decision.
2. Reasonix output is advisory, never authoritative.
3. Wrapper is the only protocol and security boundary between Codex and Reasonix.
4. MCP is control plane only; repository facts move through explicit artifacts.
5. Every Reasonix result must be schema-valid before Codex sees it as structuredContent.
6. Every patch proposal must remain a proposal until Codex validates and applies it.
7. Every accepted recommendation must have verification evidence or an explicit verification gap.
8. Every high-risk branch must stop at Human Approval Gate.
9. Every important transition must be audit-logged with task_id and request_id.
10. Hidden memory is forbidden; continuity is explicit task state.
11. Same project boundary routes to one Reasonix Project Controller, not one isolated project per Codex session.
12. Session lanes are cache boundaries, not memory boundaries.
13. Different project boundaries never share Reasonix sessions, task state, artifacts, result cache, patch proposals, context projections, audit namespace, or permission profile.
14. Codex calls Reasonix capabilities, not Reasonix internal agents.
15. Reasonix memory may generate hypotheses, but never verification evidence.
```

## 3. N01 Task Intake and Task State

### 3.1 Definition

Task Intake converts a user request, issue, PR, CI failure, or Codex-initiated subtask into a stable task object. It is the root identity for all artifacts, Reasonix calls, audit events, verification runs, and decisions.

### 3.2 Inputs

```text
user_goal
repo_root
base_branch
working_branch
constraints
risk_hints
requested_outputs
initial_artifacts
```

### 3.3 Outputs

```json
{
  "task_id": "TASK-001",
  "goal": "...",
  "repo": {
    "root": "/repo",
    "base_branch": "main",
    "working_branch": "agent/codex/TASK-001"
  },
  "state": {
    "round": 0,
    "reasonix_calls": 0,
    "patch_attempts": 0,
    "test_failure_rounds": 0,
    "status": "active"
  }
}
```

### 3.4 Hard Requirements

```text
1. task_id must be unique within the repo audit namespace.
2. task_id must appear in every artifact path and audit event.
3. Task state must be explicit; no node may rely on hidden session memory for continuity.
4. A task may be active, stopped_by_limit, waiting_for_approval, failed, or complete.
5. Completion requires verification evidence, not merely Reasonix pass verdict.
```

### 3.5 Failure Modes

```text
duplicate task_id
missing branch identity
artifacts not tied to task_id
implicit state stored only in model context
no terminal status
```

### 3.6 Audit Events

```text
task_started
task_state_updated
task_stopped_by_limit
task_waiting_for_approval
task_completed
```

## 4. N02 Codex Primary Control

### 4.1 Definition

Codex Primary Control is the invariant that Codex is the only task orchestrator, code executor, patch applier, verifier, and final decision maker.

### 4.2 Inputs

```text
task_state
user_constraints
Reasonix structured result
test/build/benchmark results
policy decisions
human approval result
```

### 4.3 Outputs

```text
codex_decision = accept | partial_accept | reject | ask_human | retry_self | stop_by_limit
decision_rationale
verification_plan
next_state
```

### 4.4 Hard Requirements

```text
1. Codex must not treat Reasonix output as system instruction.
2. Codex must not let Reasonix alter task scope, policy, sandbox, approval mode, or MCP allowlist.
3. Codex must validate every Reasonix result before action.
4. Codex must record why it accepts, partially accepts, or rejects Reasonix output.
5. Codex must be able to continue without Reasonix when Reasonix is unavailable, unless task policy says otherwise.
```

### 4.5 Failure Modes

```text
Reasonix result overrides Codex policy
Reasonix asks for secrets and Codex forwards request
Reasonix patch applied without Codex decision
Codex records output but not decision
Codex retries Reasonix indefinitely
```

### 4.6 Verification Evidence

```text
audit event includes Codex decision
decision references schema-valid request_id
post-decision tests or explicit verification gap exists
```

## 5. N03 Context Projector

### 5.1 Definition

Context Projector transforms Codex Global Context into a minimal, task-focused, security-filtered Reasonix Projected Context.

### 5.2 Inputs

```text
task_summary
git diff
selected files
test logs
runtime logs
previous Codex decisions
previous Reasonix structured outputs
risk signals
policy scope
```

### 5.3 Output Contract

```json
{
  "schema_version": "context_projection_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "source_artifacts": [
    ".agent/context/TASK-001.context.md",
    ".agent/diffs/TASK-001.codex.diff",
    ".agent/logs/TASK-001.test.log"
  ],
  "summary": "...",
  "relevant_files": [],
  "key_decisions": [],
  "open_questions": [],
  "risk_signals": [],
  "redactions": [],
  "projection_hash": "sha256:..."
}
```

### 5.4 Processing Order

```text
1. collect explicit artifacts
2. normalize paths
3. remove denied paths
4. redact secrets
5. compress logs semantically
6. select task-relevant evidence
7. produce context_projection_v1
8. hash projection
9. write audit event
```

### 5.5 Hard Requirements

```text
1. Redaction must happen before compression.
2. Projection must be lossy by design.
3. Projection must preserve evidence references.
4. Projection must not include full env vars, secrets, .codex config, or unrelated repo files.
5. If context is insufficient, Reasonix must return unknown/assumptions rather than request hidden context.
```

### 5.6 Failure Modes

```text
compressed secrets
irrelevant full logs passed through
missing source artifact references
projection changes without hash change
Reasonix relies on hidden memory
```

## 6. N04 MCP Session and Transport

### 6.1 Definition

MCP Session and Transport defines how Codex initializes, lists, and calls `reasonix-expert` tools through STDIO or Streamable HTTP.

### 6.2 Lifecycle

```text
initialize
-> notifications/initialized
-> tools/list
-> tools/call
-> shutdown
```

### 6.3 STDIO Requirements

```text
1. stdout contains JSON-RPC only.
2. stderr contains logs.
3. each JSON-RPC message is newline-delimited.
4. no debug text on stdout.
5. server handles no business request before initialization, except ping.
```

### 6.4 HTTP Requirements

```text
1. single /mcp endpoint
2. POST JSON-RPC
3. MCP-Protocol-Version header
4. bearer token or OAuth for remote service
5. Origin validation
6. timeout and rate limit
7. local service binds 127.0.0.1 unless authenticated and isolated
```

### 6.5 Failure Modes

```text
server prints logs to stdout
tools/list changes due to ordinary call side effect
HTTP binds 0.0.0.0 without auth
business request accepted before initialized
protocol errors mixed with tool execution errors
```

## 7. N05 Tool and Schema Registry

### 7.1 Definition

Tool and Schema Registry is the authoritative list of available Reasonix capabilities and their input/output contracts.

### 7.2 Tool Set

```text
reasonix.review_diff
reasonix.security_audit
reasonix.debug_hypothesis
reasonix.architecture_options
reasonix.performance_review
reasonix.propose_patch
reasonix.test_plan
```

### 7.3 Schema Set

```text
review_result_v1.json
security_audit_v1.json
debug_hypothesis_v1.json
architecture_options_v1.json
performance_review_v1.json
patch_proposal_v1.json
test_plan_v1.json
error_result_v1.json
context_projection_v1.json
audit_event_v1.json
```

### 7.4 Hard Requirements

```text
1. Every listed tool must have inputSchema and outputSchema.
2. tools/list order must be stable.
3. tools/list must not include unimplemented tools.
4. output_schema in input must match schema_version in output.
5. outputSchema failure must return schema_validation_failed, not unstructured text.
```

### 7.5 Failure Modes

```text
declared tool has no implementation
schema_version differs from requested output_schema
tool result omits request_id
free-text output accepted as structuredContent
```

## 8. N06 Wrapper Input Gate

### 8.1 Definition

Wrapper Input Gate validates every `tools/call` before Reasonix is invoked.

### 8.2 Validation Order

```text
1. validate MCP method and initialized state
2. validate tool name
3. validate inputSchema
4. validate task_id and request_id format
5. normalize repo root and artifact paths
6. verify artifact existence
7. enforce read allowlist and denylist
8. enforce permission_level
9. enforce budget
10. construct Reasonix task spec
```

### 8.3 Hard Requirements

```text
1. Denylist always wins over allowlist.
2. Absolute paths are rejected unless they normalize inside repo root and policy allows them.
3. Paths containing .., symlink escape, or case-folding bypass are rejected.
4. Missing artifacts return artifact_not_found.
5. Permission mismatch returns permission_denied.
```

### 8.4 Failure Modes

```text
path traversal
symlink escape
tool name alias bypass
budget ignored
policy checked after Reasonix invocation
```

## 9. N07 Reasonix Execution Gate

### 9.1 Definition

Reasonix Execution Gate controls how the Wrapper invokes Reasonix CLI, local process, API, or future controller.

### 9.2 Execution Modes

```text
readonly_process
patch_proposal_process
isolated_worktree_process
remote_worker
```

### 9.3 Hard Requirements

```text
1. Default execution is read-only.
2. Reasonix terminal must not share Codex writable terminal.
3. Network is denied unless explicit policy allows it.
4. Shell is denied except allowlisted read-only commands.
5. L2_PATCH_ONLY may produce patch text but must not write Codex worktree.
6. L3_ISOLATED_WORKTREE may write only isolated worktree.
7. L4_DIRECT_WRITE is forbidden.
```

### 9.4 Failure Modes

```text
Reasonix writes source files directly
Reasonix reads .env
Reasonix launches network request
Reasonix runs deployment command
Reasonix modifies .codex or policy files
```

## 10. N08 Output Normalization Gate

### 10.1 Definition

Output Normalization Gate converts Reasonix raw output into a schema-valid MCP tool result.

### 10.2 Processing Order

```text
1. capture stdout and stderr separately
2. reject empty output
3. extract one JSON object only
4. reject multiple JSON objects
5. remove markdown fence only as parsing tolerance
6. validate schema
7. validate task_id and request_id
8. validate file paths and patch paths
9. detect override / secret / policy-bypass requests
10. return structuredContent or isError tool result
```

### 10.3 Hard Requirements

```text
1. Raw Reasonix text must never become trusted structuredContent.
2. Invalid JSON returns schema_validation_failed.
3. Prompt-injection requests are recorded as risk and ignored.
4. Tool execution errors return tool result with isError=true.
5. Protocol errors are distinct from tool execution errors.
```

### 10.4 Failure Modes

```text
markdown text accepted as result
multiple JSON objects merged
schema mismatch ignored
Reasonix asks Codex to disable sandbox
stderr leaked into structuredContent
```

## 11. N09 Codex Decision Gate

### 11.1 Definition

Codex Decision Gate evaluates schema-valid Reasonix results and decides whether to accept, partially accept, reject, request human approval, self-debug, or stop.

### 11.2 Decision Values

```text
accept
partial_accept
reject
ask_human
retry_self
retry_reasonix_once
stop_by_limit
```

### 11.3 Required Decision Record

```json
{
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "codex_decision": "partial_accept",
  "accepted_findings": ["F-001"],
  "rejected_findings": ["F-002"],
  "rationale": "...",
  "verification_required": true
}
```

### 11.4 Hard Requirements

```text
1. Codex must not apply recommendations without decision record.
2. Rejecting Reasonix must be allowed and auditable.
3. Partial acceptance must list accepted and rejected findings.
4. High-risk conflicts go to Human Approval Gate.
5. Performance conclusions go to Verification Gate before being treated as true.
```

## 12. N10 Patch Safety Checker

### 12.1 Definition

Patch Safety Checker determines whether a Reasonix-proposed unified diff is eligible for Codex-controlled application.

### 12.2 Inputs

```text
patch text
files_changed
permission_level
scope allowlist
scope denylist
sensitive path rules
task policy
```

### 12.3 Checks

```text
1. parse unified diff
2. verify patch applies cleanly in dry-run
3. normalize every path under repo root
4. reject absolute paths and traversal
5. enforce denylist before allowlist
6. reject secrets and credential-like additions
7. reject production deploy / publish commands
8. detect CI / policy / security config changes
9. detect test deletion, skip, assertion weakening, benchmark removal
10. produce patch_safety_report_v1
```

### 12.4 Output Contract

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

### 12.5 Hard Requirements

```text
1. Patch checker must not rely on ad hoc string slicing.
2. Denylist always wins.
3. Checker failure blocks patch application.
4. Checker pass does not prove correctness; it only permits Codex to attempt application.
5. Checker output must be audit logged.
```

## 13. N11 Verification Gate

### 13.1 Definition

Verification Gate upgrades a claim from advisory or inferred status to verified status using tests, lint, build, benchmark, profiling, static analysis, or approved human evidence.

### 13.2 Verification Types

```text
unit_test
integration_test
lint
typecheck
build
security_scan
benchmark
profiling
manual_approval
static_review
```

### 13.3 Claim Mapping

| Claim Type | Minimum Evidence |
|---|---|
| bug fixed | relevant failing test now passes or repro no longer fails |
| security risk addressed | targeted security test or human approval |
| performance improved | benchmark/profiling before-after evidence |
| patch safe to apply | patch_safety_report_v1 pass |
| architecture acceptable | Codex decision record plus explicit constraints |
| test plan adequate | tests mapped to risk areas |

### 13.4 Hard Requirements

```text
1. Verification must reference command, artifact, timestamp, and result.
2. Passing generic tests cannot verify a specific performance claim.
3. If verification cannot run, Codex must record a verification gap.
4. Final completion requires no unresolved required verification gaps.
5. Reasonix memory/history cannot satisfy a required verification gap.
```

## 14. N12 Audit Event Model

### 14.1 Definition

Audit Event Model records the complete chain from task start to final decision.

### 14.2 Required Base Fields

```json
{
  "schema_version": "audit_event_v1",
  "ts": "2026-06-28T10:01:00+08:00",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "actor": "codex",
  "event": "reasonix_called",
  "status": "ok"
}
```

### 14.3 Event Types

```text
task_started
context_projected
reasonix_called
reasonix_result_received
schema_validation_failed
codex_decision_recorded
patch_safety_checked
patch_applied
verification_started
verification_finished
human_approval_requested
human_approval_received
task_stopped_by_limit
task_completed
```

### 14.4 Hard Requirements

```text
1. Every Reasonix call must have reasonix_called and reasonix_result_received or timeout event.
2. Every Codex accept/partial_accept/reject decision must be logged.
3. Audit entries must not include secrets or full env vars.
4. Audit entries must include artifact paths, not embedded full artifacts.
5. Audit log is append-only.
```

## 15. N13 Loop and Budget Limiter

### 15.1 Definition

Loop and Budget Limiter prevents automatic Codex/Reasonix loops from running indefinitely.

### 15.2 Limits

```yaml
max_reasonix_calls_per_task: 3
max_total_rounds: 6
max_patch_attempts: 3
max_test_failure_rounds: 3
max_runtime_minutes_per_task: 30
max_runtime_minutes_per_reasonix_call: 10
```

### 15.3 Hard Requirements

```text
1. Limits are checked before every Reasonix call and before every patch attempt.
2. Reaching a limit changes task state to stopped_by_limit or waiting_for_approval.
3. Codex must summarize current state after limit stop.
4. Codex must not silently reset counters.
5. User or policy must explicitly reopen a stopped-by-limit task.
```

## 16. N14 Human Approval Gate

### 16.1 Definition

Human Approval Gate is the mandatory stop point for high-risk changes.

### 16.2 Approval Triggers

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

### 16.3 Output Contract

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

### 16.4 Hard Requirements

```text
1. Approval gate is a blocking state, not a warning.
2. The request must include risk summary and artifacts.
3. Approval must be recorded before high-risk action proceeds.
4. Denial must be recorded and must prevent the action.
```

## 17. N15 Performance Review Gate

### 17.1 Definition

Performance Review Gate governs `reasonix.performance_review` and prevents performance-oriented language from being treated as verified measurement.

### 17.2 Inputs

```text
diff
runtime logs
benchmark logs
profiling logs
hot path context
database query context
cache behavior context
```

### 17.3 Outputs

```json
{
  "schema_version": "performance_review_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "status": "ok",
  "verdict": "risky",
  "summary": "...",
  "findings": [],
  "bottlenecks": [],
  "benchmark_plan": [],
  "profiling_commands": [],
  "tests_to_run": [],
  "confidence": 0.72
}
```

### 17.4 Hard Requirements

```text
1. Reasonix may identify performance risks, not assert verified performance gains.
2. A performance improvement claim requires benchmark or profiling evidence.
3. Benchmark deletion or weakening is a patch safety concern.
4. performance_review should prefer measurable hypotheses over vague optimization advice.
5. Codex must record whether performance evidence was run, skipped, or unavailable.
```

### 17.5 Failure Modes

```text
unmeasured speedup claim
benchmark plan missing
profiling command unsafe
performance finding not tied to file or evidence
generic "optimize cache" recommendation
```

## 18. N16 Reasonix Project Controller and Session Router

### 18.1 Definition

Reasonix Project Controller and Session Router owns project-level reuse and lane-level cache routing for Reasonix calls that pass through `reasonix-expert`.

It ensures:

```text
multiple Codex sessions -> same compatible Reasonix Project Controller
different Coasonix tasks -> isolated task namespaces
different expert lanes -> cache-stable session lanes
cross-tool continuity -> explicit task_state and artifacts only
```

### 18.2 Inputs

```text
repo_root
repo_root_realpath
worktree_id
tenant_id_or_user_id
base_branch
reasonix_config_hash
coasonix_policy_hash
schema_family
reasonix_runtime_version
task_id
codex_session_id
base_revision
tool_name
permission_level
schema_family
static_prefix_hash
snapshot_id
```

### 18.3 Outputs

```json
{
  "project_key_hash": "sha256:...",
  "session_key_hash": "sha256:...",
  "task_namespace": "TASK-001:CODEX-001:worktree-001:abc123",
  "snapshot_id": "SNAP-001",
  "base_revision": "abc123",
  "lane": "review",
  "decision": "allow"
}
```

### 18.4 Hard Requirements

```text
1. Project key includes repo root, worktree, Reasonix config hash, policy hash, and Reasonix runtime version.
1a. Project key should also include tenant/user boundary, repo realpath, base branch, and schema family when available.
2. Task namespace includes task_id, codex_session_id, branch/worktree, and base_revision.
3. Session hit requires matching project_key, task_id, lane, permission, schema family, static prefix, policy hash, and runtime version.
4. Patch lane is isolated and cannot share a read-only lane by default.
5. Snapshot mismatch invalidates result before Codex decision.
6. Task namespace mismatch invalidates result before Codex decision.
7. Routing decision must be audit logged.
8. MVP session_key includes task_id; project-level lane reuse is disabled until conformance tests prove isolation.
9. Codex must not select or control Reasonix internal subagents.
10. Reasonix project memory may be projected as hypothesis context, not evidence.
```

### 18.5 Failure Modes

```text
one Reasonix project per Codex session for the same worktree
all tools routed into one giant session
patch tool routed to read-only review lane
policy hash changes but session is reused
Reasonix output from TASK-A consumed by TASK-B without explicit projection
snapshot mismatch merged into task_state
session reused across different repo/worktree/project_key
result cache served across Project Controllers
Codex selects Reasonix internal planner/reviewer/security agent directly
Reasonix memory accepted as verified evidence
```

### 18.6 Audit Events

```text
reasonix_project_controller_selected
reasonix_project_route_denied
reasonix_session_route_selected
reasonix_session_route_denied
reasonix_snapshot_frozen
reasonix_snapshot_mismatch
reasonix_task_namespace_mismatch
```

---

## 19. Cross-Node Completion Criteria

The system is ready for implementation only when each node has:

```text
1. a named owner component
2. explicit inputs
3. explicit outputs
4. status / verdict semantics
5. failure modes
6. hard requirements
7. audit events
8. verification evidence
```

Current document status:

| Node | Defined Here | Needs JSON Schema File |
|---|---:|---:|
| N01 Task Intake and Task State | yes | yes |
| N02 Codex Primary Control | yes | no |
| N03 Context Projector | yes | yes |
| N04 MCP Session and Transport | yes | no |
| N05 Tool and Schema Registry | yes | yes |
| N06 Wrapper Input Gate | yes | no |
| N07 Reasonix Execution Gate | yes | no |
| N08 Output Normalization Gate | yes | yes |
| N09 Codex Decision Gate | yes | yes |
| N10 Patch Safety Checker | yes | yes |
| N11 Verification Gate | yes | yes |
| N12 Audit Event Model | yes | yes |
| N13 Loop and Budget Limiter | yes | no |
| N14 Human Approval Gate | yes | yes |
| N15 Performance Review Gate | yes | yes |
| N16 Reasonix Project Controller and Session Router | yes | no |

## 20. Reassessment Inputs

The framework reassessment must evaluate:

```text
1. whether Codex remains the sole final authority
2. whether every Reasonix path crosses Wrapper gates
3. whether every output is schema-validated
4. whether every patch path crosses Patch Safety Checker
5. whether every accepted recommendation crosses Verification Gate
6. whether every high-risk branch crosses Human Approval Gate
7. whether every important transition emits audit events
8. whether context projection is minimal, explicit, and reproducible
9. whether performance claims are evidence-gated
10. whether budget limits prevent automatic loops
11. whether Reasonix Project Controller is shared across compatible Codex sessions
12. whether task namespace and snapshot mismatches fail closed
13. whether session lane reuse is cache-only and never hidden memory
14. whether different project boundaries isolate sessions, task state, artifacts, result cache, patch proposals, context projections, audit namespace, and permission profile
15. whether Reasonix memory/history is limited to hypothesis input and never treated as verification evidence
16. whether Codex invokes Reasonix capabilities only, not internal agents
```
