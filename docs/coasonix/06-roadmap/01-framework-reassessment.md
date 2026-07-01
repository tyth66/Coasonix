# 框架复审与成熟度评估

本文在关键节点定义之后，重新审视 Coasonix 框架是否已经形成可实现、可验证、可审计的闭环。

## 1. 复审结论

当前框架已经从“多 Agent 架构设想”推进为“Codex 主控、Reasonix 专家化、Wrapper 边界化、Schema 契约化、Policy 可执行化”的工程规格雏形。

最重要的结论：

```text
1. 主控边界成立：Codex 是唯一最终裁决者。
2. 协议边界成立：Codex 与 Reasonix 不直接互聊，必须经过 reasonix-expert Wrapper。
3. 数据边界成立：MCP 传控制信息，Git/files/logs/artifacts 承载事实材料。
4. 结果边界成立：Reasonix 原始输出不能直接进入 Codex 决策面。
5. 安全边界基本成立：权限、patch checker、禁用 sampling/elicitation/resources 已定义。
6. 上下文边界基本成立：Codex 持有全局上下文，Reasonix 只接收投影上下文。
7. 验证边界成立：Reasonix 建议必须经过 Codex 验证才能升级为事实。
8. 审计边界基本成立：关键事件、决策和验证结果需要 JSONL 记录。
```

剩余工作不在概念层，而在工程实现层：需要把文档中的 state machine runner、schema validator、policy matcher、patch checker、audit writer、observability exporter 和 STDIO Wrapper 转成可运行实现。可执行 runtime 细节已经收束到 `../02-runtime/06-executable-runtime-details.md`，实现时应以该文件作为 matcher、canonicalization、cache、audit、verification 和 approval 的确定性补充契约。

## 2. 控制闭环复审

### 2.1 闭环路径

```text
User / Issue / CI
-> Codex Task Intake
-> Codex Plan / Execute / Test
-> optional Reasonix advisory result
-> Codex Decision Gate
-> Verification Gate
-> Final Summary / Patch / PR
```

### 2.2 成立条件

```text
1. Codex owns task state.
2. Codex owns final decision.
3. Reasonix cannot mutate task scope.
4. Reasonix cannot apply patch directly.
5. Human approval blocks high-risk branches.
```

### 2.3 Current Assessment

| Item | Status | Evidence |
|---|---|---|
| Codex final authority | strong | Role file defines Codex as only primary agent |
| Reasonix advisory status | strong | Tool and safety specs reject Reasonix as instruction source |
| Codex decision record | medium-high | Decision gate and codex_decision_v1 registry entry exist; implementation still needed |
| Human approval hard block | medium | Rule exists; implementation state machine still needed |

### 2.4 Residual Risk

Control remains conceptually sound, but implementation must avoid treating approval as a log-only event. `waiting_for_approval` must be a blocking state.

## 3. Protocol 闭环复审

### 3.1 闭环路径

```text
initialize
-> notifications/initialized
-> tools/list
-> tools/call
-> structuredContent / isError
-> shutdown
```

### 3.2 成立条件

```text
1. STDIO stdout contains JSON-RPC only.
2. tools/list is stable.
3. tools/list declares no empty tools.
4. Protocol errors are distinct from tool execution errors.
5. HTTP mode includes auth, Origin validation, timeout, and rate limit.
```

### 3.3 Current Assessment

| Item | Status | Evidence |
|---|---|---|
| STDIO transport | strong | Rules defined for stdout/stderr and newline JSON-RPC |
| HTTP transport | medium | Security requirements defined; no deployment profile yet |
| tools/list consistency | strong | Seven tools listed and performance_review now defined |
| error separation | medium-high | Concept and error_result_v1 registry entry exist; implementation still needed |

### 3.4 Residual Risk

The protocol path is ready for an MVP Wrapper. `error_result_v1` is present in the v1 registry, but protocol-error versus tool-execution-error mapping still needs conformance tests.

## 4. Tool / Schema 闭环复审

### 4.1 Required Tool Set

```text
reasonix.review_diff
reasonix.security_audit
reasonix.debug_hypothesis
reasonix.architecture_options
reasonix.performance_review
reasonix.propose_patch
reasonix.test_plan
```

### 4.2 Required Schema Set

```text
review_result_v1
security_audit_v1
debug_hypothesis_v1
architecture_options_v1
performance_review_v1
patch_proposal_v1
test_plan_v1
error_result_v1
context_projection_v1
audit_event_v1
patch_safety_report_v1
codex_decision_v1
verification_result_v1
human_approval_request_v1
```

### 4.3 Current Assessment

| Item | Status | Evidence |
|---|---|---|
| Tool inventory | strong | All seven tools have named sections |
| Performance tool | strong | Now defined as performance risk review, not proof |
| Input envelope | medium | Example exists; standalone envelope schema still missing |
| Output envelope | medium-high | Tool result schemas exist; shared envelope semantics still need conformance tests |
| Registry completeness | medium-high | Required schema names are represented in the v1 registry; request/envelope gaps remain |

### 4.4 Residual Risk

The schema layer is specified enough to implement, but not yet machine-enforced. Without a validator and conformance tests, Wrapper behavior can drift even when registry entries exist.

## 5. Context 闭环复审

### 5.1 闭环路径

```text
Codex Global Context
-> Context Projector
-> context_projection_v1
-> Reasonix Tool Input
-> Structured Output
-> Codex Global Context
```

### 5.2 成立条件

```text
1. Projection is explicit, lossy, task-focused, and security-filtered.
2. Redaction happens before compression.
3. Projection includes source artifact paths.
4. Projection can be hashed.
5. Reasonix tools share explicit state only.
```

### 5.3 Current Assessment

| Item | Status | Evidence |
|---|---|---|
| Context ownership | strong | Codex owns global context; Reasonix receives projection |
| Hidden memory ban | strong | Tools do not remember; state object owns continuity |
| Projection process | medium-high | Processing order and minimum redaction catalog defined; projector implementation missing |
| Cache boundary | medium-high | Stable prefix principle and canonical cache key fields defined; cache implementation missing |

### 5.4 Residual Risk

Context Projector is the largest quality risk. It now has minimum redaction rules, but still needs log compression rules and projection tests with adversarial secrets.

## 6. Execution and Safety 闭环复审

### 6.1 闭环路径

```text
tools/call
-> Wrapper Input Gate
-> Reasonix Execution Gate
-> Output Normalization Gate
-> Codex Decision Gate
```

### 6.2 成立条件

```text
1. Permission checked before invocation.
2. Filesystem denylist wins over allowlist.
3. Network default deny.
4. Shell default deny.
5. Reasonix cannot share Codex writable terminal.
6. L4_DIRECT_WRITE is forbidden.
```

### 6.3 Current Assessment

| Item | Status | Evidence |
|---|---|---|
| Permission model | strong | L0-L4 defined; L4 forbidden |
| Filesystem policy | medium-high | allowlist/denylist and matcher semantics defined; implementation missing |
| Shell policy | medium-high | argv-level parser rules defined; implementation missing |
| Network policy | medium-high | default deny and exception shape defined; implementation missing |
| Terminal isolation | strong | Shared writable terminal forbidden |

### 6.4 Residual Risk

The policy model must avoid shell-string matching. Command allowlists are now specified at argv level, but still need implementation and bypass tests.

## 7. Patch 闭环复审

### 7.1 闭环路径

```text
Reasonix patch proposal
-> Output Schema Validation
-> Patch Safety Checker
-> Codex Decision
-> Dry-run apply
-> Actual apply
-> Verification Gate
```

### 7.2 成立条件

```text
1. Reasonix patch remains data until Codex applies it.
2. Patch checker parses unified diff.
3. Denylist wins.
4. Test deletion / skip / assertion weakening is detected.
5. Patch checker pass does not mean correctness.
```

### 7.3 Current Assessment

| Item | Status | Evidence |
|---|---|---|
| Patch proposal status | strong | L2_PATCH_ONLY produces diff but does not write worktree |
| Safety checks | medium-high | Strong checklist exists; implementation still needed |
| Test weakening detection | medium | Mentioned; rule details still needed |
| Dry-run apply | medium-high | Dry-run and transaction command contract defined; implementation missing |

### 7.4 Residual Risk

Patch Safety Checker should be implemented before `reasonix.propose_patch` is enabled in non-experimental use.

## 8. Verification 闭环复审

### 8.1 闭环路径

```text
Codex accepts recommendation
-> defines claim
-> selects verification type
-> runs verification
-> records result
-> updates task state
```

### 8.2 成立条件

```text
1. Claim type maps to evidence type.
2. Verification command and result are recorded.
3. Generic tests do not verify specific performance claims.
4. Missing verification is explicit gap.
5. Completion requires no unresolved required gap.
```

### 8.3 Current Assessment

| Item | Status | Evidence |
|---|---|---|
| CI as validation layer | strong | Framework principle defines CI as verification layer |
| Claim/evidence mapping | medium-high | Defined in critical nodes and executable runtime details; runner and schema integration missing |
| Performance evidence | strong | performance_review explicitly requires benchmark/profiling for proof |
| Completion gate | medium | Principle exists; no state machine yet |

### 8.4 Residual Risk

Verification must become a structured artifact, not just command output in logs.

## 9. Audit 闭环复审

### 9.1 闭环路径

```text
task_started
-> context_projected
-> reasonix_called
-> reasonix_result_received
-> codex_decision_recorded
-> patch_safety_checked
-> verification_finished
-> task_completed
```

### 9.2 成立条件

```text
1. audit_event_v1 is append-only.
2. Every event has task_id.
3. Reasonix-related events have request_id.
4. Audit entries reference artifacts, not embedded secrets.
5. Codex decisions and verification results are recorded.
```

### 9.3 Current Assessment

| Item | Status | Evidence |
|---|---|---|
| Audit principle | strong | JSONL audit defined |
| Required fields | strong | Existing docs list key fields |
| Event taxonomy | medium-high | Minimum event taxonomy defined; schema enum and implementation missing |
| Append-only guarantee | medium-high | Append-only storage rules defined; storage enforcement missing |

### 9.4 Residual Risk

Audit will be useful only if every gate is instrumented. A partial audit log is worse than no audit if it creates false confidence.

## 10. Performance 闭环复审

### 10.1 闭环路径

```text
performance-sensitive diff
-> reasonix.performance_review
-> performance risk findings
-> benchmark/profiling plan
-> Codex runs evidence
-> Codex records performance conclusion
```

### 10.2 成立条件

```text
1. Reasonix may identify risk but not assert verified gain.
2. Benchmark/profiling plan is part of output.
3. Performance claim requires evidence.
4. Benchmark deletion is patch safety issue.
5. Performance evidence is audit logged.
```

### 10.3 Current Assessment

| Item | Status | Evidence |
|---|---|---|
| Tool purpose | strong | performance_review has full tool definition |
| Evidence boundary | strong | Document says only Codex verification can upgrade conclusion |
| Benchmark artifacts | medium | Required conceptually; artifact paths not standardized |
| Profiling safety | medium | Output field exists; command safety policy needed |

### 10.4 Residual Risk

Profiling commands can be unsafe if treated like ordinary test commands. They need the same shell policy enforcement as other commands.

## 11. Implementation Readiness

| Area | Readiness | Reason |
|---|---|---|
| Architecture | high | Control and role boundaries are clear |
| Deterministic runtime spec | high | State, schema, policy, transaction, concurrency, cache, observability, and versioning are defined |
| Runtime enforcement design | high | Runtime Enforcement Layer, three core engines, and executable matcher/canonicalization details are specified |
| MCP MVP | medium-high | STDIO path is concrete |
| Tool contracts | medium-high | Tool list complete; machine-readable v1 schema registry exists |
| Context projection | medium-high | Threat model and redaction order defined; projector implementation missing |
| Patch safety | medium-high | Transaction model and checker rules defined; checker not implemented |
| Audit | medium-high | Event model and schema anchor exist; storage writer missing |
| Verification | medium-high | Gate and structured verification schema exist; runner missing |
| Human approval | medium-high | Triggers and blocking state machine defined; approval UI/storage missing |
| Performance review | medium-high | Tool definition and evidence boundary complete; benchmark runner missing |
| Safe autonomous operation | low | Blocked until Runtime Enforcement Layer implementation and conformance tests exist |
| Production HTTP | medium-low | Security requirements listed; deployment model not specified |

## 12. Revised Framework Shape

The framework should be understood as five nested boundaries:

```text
Boundary 1: Authority
  Codex owns task, state, decisions, execution, and final result.

Boundary 2: Protocol
  MCP Wrapper is the only Codex <-> Reasonix bridge.

Boundary 3: Context
  Context Projector exports minimal explicit context; no hidden memory.

Boundary 4: Execution
  Permission, sandbox, path policy, patch checker, and budget limiter gate actions.

Boundary 5: Evidence
  Verification, audit, and human approval determine what can be called complete.
```

## 13. Priority Next Work

The next implementation work should happen in this order:

```text
1. Implement Draft 2020-12 schema validator using ../schemas/coasonix-v1.schema.json.
2. Implement Global Task State Machine runner.
3. Implement executable canonicalization, path matcher, shell argv parser, network exception matcher, and cache key builder from ../02-runtime/06-executable-runtime-details.md.
4. Implement Patch Safety Checker and dry-run/apply transaction contract.
5. Implement Policy Execution Engine for path, permission, shell, network, patch, approval, and cache gates.
6. Compose Runtime Kernel decision flow.
7. Implement Context Projector redaction, hashing, and adversarial tests.
8. Implement audit_event_v1 writer and trace/metrics exporters.
9. Implement verification runner and benchmark/profiling artifact capture for performance_review.
10. Implement STDIO Wrapper MVP.
11. Add adversarial tests for prompt injection, path traversal, schema mismatch, secret leakage, concurrency merge, cache invalidation, transaction rollback, runtime deny, shell parser bypass, audit corruption, approval mismatch, and loop limit.
```

## 14. Final Reassessment

Coasonix is now best described as:

```text
Codex-centered expert delegation runtime with strict tool contracts,
explicit context projection, policy-bound execution, evidence-gated decisions,
and append-only auditability.
```

The design is coherent and implementable. The remaining uncertainty is no longer architectural or specification-level; it is runtime-construction work. The first implementation milestone should therefore be a Runtime Enforcement Layer with schema validation, state-machine enforcement, policy execution, audit logging, and one read-only Reasonix tool before enabling patch generation.
