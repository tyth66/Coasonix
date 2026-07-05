# 上下文架构规范

> **设计规格（Design Specification）**：此文档描述的是 post-v1 上下文投影架构。
> 当前 v1 的 MCP 工具参数直接传给 Reasonix，不存在 Context Projector。
> 代码中无三层上下文模型、redaction、projection hash 的实现。

## 20. Codex × Reasonix 上下文设计规范（Context Architecture Spec）

### 20.1 上下文设计总原则

本系统的上下文设计遵循四个核心原则：

1. Codex owns global context (全局状态所有者)
2. Reasonix receives projected context (上下文投影，不是访问)
3. MCP Wrapper enforces context boundary (上下文边界执行器)
4. Cache optimization is based on stable prompt prefix (缓存基于前缀稳定性)
### 20.2 上下文分层模型（核心架构）

系统上下文分为三层：

┌──────────────────────────────┐
│ L1: Global Context (Codex)    │  ← 全局状态层
├──────────────────────────────┤
│ L2: Projected Context         │  ← Reasonix输入层（裁剪后）
├──────────────────────────────┤
│ L3: Execution Context         │  ← 单次tool运行上下文（delta）
└──────────────────────────────┘
#### L1：Codex Global Context（全局上下文）

Codex 持有完整系统状态：

repo 全量状态
git history
CI / test logs
multi-step task state
tool execution history
Reasonix outputs history

特点：

✔ persistent
✔ authoritative
✔ mutable
✔ full fidelity
#### L2：Reasonix Projected Context（投影上下文）

Reasonix 不能访问 L1 全量上下文，只能接收：

{
  "context_projection": {
    "task_summary": "...",
    "key_state": "...",
    "relevant_history": [...],
    "constraints": [...]
  }
}

特点：

✔ lossy (信息损失是设计要求)
✔ task-focused
✔ minimal sufficient context
✔ security-filtered
#### L3：Execution Context（工具执行上下文）

每个 Reasonix tool 调用时的输入：

{
  "tool_name": "reasonix.review_diff",
  "task_id": "...",
  "delta_context": {
    "diff_excerpt": "...",
    "log_excerpt": "...",
    "focus": "..."
  }
}

特点：

✔ stateless
✔ minimal
✔ cache-friendly
✔ deterministic input target
### 20.3 Codex ↔ Reasonix 上下文流动模型
flowchart TD
    A[Codex Global Context] --> B[Context Projector]
    B --> C[Reasonix Input Context]
    C --> D[Tool Execution]
    D --> E[Structured Output]
    E --> A
### 20.4 Context Projector（上下文投影器）

Context Projector 是 MCP Wrapper 的核心能力之一。

作用：

将 Codex 全局上下文转换为 Reasonix 可消费的最小信息集。

输入：
repo diff
CI logs
task history
Codex hypothesis
输出：
{
  "summary": "...",
  "relevant_files": [],
  "key_decisions": [],
  "open_questions": [],
  "risk_signals": []
}
投影原则：
1. remove irrelevant history
2. compress logs semantically
3. keep only task-relevant signals
4. eliminate secrets
5. normalize structure

工程约束：

```text
1. Context Projector 必须输出显式 context_projection 对象。
2. 投影结果必须记录 source artifact paths，便于审计和复现。
3. 投影时必须先脱敏，再压缩，再裁剪。
4. 如果上下文不足，Reasonix 应返回 unknown / assumptions，而不是请求隐藏上下文。
5. 投影输出必须可 hash，用于 cache key 和审计比对。
```

### 20.5 Reasonix Tool Context 模型

Reasonix 内部工具遵循统一上下文模型：

✔ Tool Input = Base Prefix + Delta Context
[STATIC BASE PREFIX]
    +
[TOOL DELTA INPUT]
#### Base Prefix（缓存核心）

特点：

✔ stable
✔ identical across calls
✔ long-lived
✔ cache key anchor

包含：

system rules
repo schema summary
constraints
tool definitions
security rules
#### Delta Context（动态部分）

每次 tool 调用变化：

{
  "task_id": "...",
  "focus": "...",
  "diff_excerpt": "...",
  "log_excerpt": "..."
}
### 20.6 Reasonix Tools 是否共享上下文？
❌ 禁止隐式共享（No implicit memory）

Reasonix tools：

不共享 hidden memory
不共享 session state
不依赖 execution order memory
✔ 允许共享（explicit state only）

共享方式：

{
  "task_state": {
    "hypothesis_pool": [],
    "findings": [],
    "risk_register": [],
    "confidence": 0.0
  }
}
核心原则：
Tools do NOT remember.
Tools only READ and WRITE explicit state.

### 20.6.1 Reasonix Project Memory Evidence Boundary

Reasonix project memory / history / project knowledge may enter Coasonix only through explicit projection and structured output.

Allowed role:

```text
hypothesis source
triage hint
context recall candidate
```

Forbidden role:

```text
verification evidence
security proof
performance proof
completion evidence
patch safety evidence
```

If Reasonix memory contributes to a finding, the finding must remain an unverified hypothesis until Codex validates it against code evidence, tests, build/lint, benchmark/profiling, audit records, or human approval.

### 20.7 Reasonix Session 模型（关键设计）
❗Session 的真实定义：
Session = cache optimization boundary
NOT memory boundary
Session 用途：

✔ KV cache reuse
✔ prompt prefix stability
✔ latency optimization

Session 禁止用途：

❌ storing tool memory
❌ implicit reasoning chain
❌ cross-tool hidden state

### 20.7.1 Reasonix Project / Session Lane / Tool Call Mapping

Coasonix 区分三层边界：

```text
Global Reasonix Runtime = binary/provider/tool-schema/global-prefix boundary
Reasonix Project = repo/worktree/config/memory/plugin/sandbox boundary
Reasonix Session Lane = cache-stable inference boundary
Reasonix Tool Call = Codex-visible MCP invocation
```

同一个 Coasonix task 在兼容的 repo_root / worktree / Reasonix config / policy / runtime version 上的多个 `reasonix.*` 调用，必须路由到同一个 Reasonix Project Controller；但不应全部混进一个单一 Reasonix session。

不同项目必须路由到不同 Reasonix Project Controller。它们不能共享 session、task_state、artifact paths、result cache、patch proposals、context projection、audit namespace 或 permission profile。

推荐 lane：

```text
review lane: reasonix.review_diff, reasonix.test_plan
security lane: reasonix.security_audit
debug lane: reasonix.debug_hypothesis
performance lane: reasonix.performance_review
architecture lane: reasonix.architecture_options
patch lane: reasonix.propose_patch
```

核心约束：

```text
1. Same compatible repo/worktree/config/policy/runtime SHOULD reuse the same Reasonix Project Controller.
2. Same role/model/permission/static prefix SHOULD reuse the same session lane.
3. Different role/model/permission/policy/mutation surface SHOULD use different lanes.
4. Cross-tool continuity MUST use explicit task_state and artifacts.
5. Session lane reuse is a cache optimization, not a correctness dependency.
6. Different project boundaries MUST NOT reuse Reasonix sessions.
```

Detailed routing rules live in [04-project-session-tool-mapping.md](04-project-session-tool-mapping.md).

### 20.8 Reasonix 输出上下文规范

Reasonix 输出必须严格结构化：

✔ 标准输出结构
{
  "verdict": "needs_fix",
  "summary": "...",
  "findings": [],
  "risks": [],
  "confidence": 0.0
}
❌ 禁止输出：
- chain-of-thought
- hidden reasoning trace
- step-by-step internal deliberation
✔ 允许输出：
- compressed reasoning summary
- evidence-based findings
- risk analysis
- decision rationale (non-stepwise)
### 20.9 Cache 优化策略（关键工程点）

Reasonix 的性能优化依赖：

✔ Cache Key 结构：
cache_key =
  hash(STATIC_BASE_PREFIX)
  + tool_name
  + hash(delta_context)
✔ Cache 命中关键：
1. prefix must be byte-identical
2. tool schema stable
3. ordering deterministic
### 20.10 最终设计总结

系统上下文本质是：

Codex:
    full world state (authoritative memory)

Reasonix:
    projected local reasoning context (ephemeral)

MCP Wrapper:
    context transformer + security boundary

Tools:
    stateless executors with shared explicit state

Cache:
    driven by stable prompt prefix, not session memory
### 20.11 一句话定义（最终规范）
Codex owns memory,
Reasonix owns inference,
MCP owns context boundary,
Cache owns performance,
State object owns continuity.
