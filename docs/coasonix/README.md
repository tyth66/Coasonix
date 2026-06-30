# Coasonix 文档索引

Coasonix 是 Codex-Orchestrated Reasonix Runtime：以 Codex 为主控 Agent，以 Reasonix 为 DeepSeek cache-first 专家多 Agent 系统，通过 `reasonix-expert` Wrapper 建立可审计、可验证、可控权限、可回滚的专家委派体系。

核心边界：

```text
Codex = 主控 / 编排 / 执行 / 最终裁决
Reasonix = DeepSeek cache-first 专家多 Agent 系统
Wrapper = MCP Gateway + Runtime Gate + Session Router
同项目 = shared Project Controller + isolated task namespaces + lane sessions
不同项目 = isolated Project Controller
```

## 推荐阅读顺序

1. [00-executive-summary.md](00-executive-summary.md) - 总结当前结论、MVP 默认值和安全边界
2. [01-architecture/01-overview-and-roles.md](01-architecture/01-overview-and-roles.md) - 总览、角色边界和 Codex 编排流程
3. [01-architecture/02-communication-and-mcp.md](01-architecture/02-communication-and-mcp.md) - MCP 生命周期、transport 和 control plane
4. [01-architecture/03-context-architecture.md](01-architecture/03-context-architecture.md) - 上下文所有权、投影、显式状态和缓存边界
5. [01-architecture/04-project-session-tool-mapping.md](01-architecture/04-project-session-tool-mapping.md) - Project Controller、Session Pool、lane 和 tool 映射
6. [02-runtime/01-global-task-state-machine.md](02-runtime/01-global-task-state-machine.md) - 全局任务状态机和迁移规则
7. [02-runtime/02-runtime-enforcement-layer.md](02-runtime/02-runtime-enforcement-layer.md) - Runtime Kernel、状态门禁、schema 门禁和 policy 门禁
8. [02-runtime/03-policy-engine.md](02-runtime/03-policy-engine.md) - 权限、路径、shell、network、approval 和 cache policy
9. [02-runtime/04-schema-enforcement.md](02-runtime/04-schema-enforcement.md) - schema enforce 规则和当前 schema 覆盖缺口
10. [02-runtime/05-observability-contract.md](02-runtime/05-observability-contract.md) - metrics、tracing、debug hooks 和 audit 可观测性
11. [03-reasonix/01-tool-contracts-and-wrapper.md](03-reasonix/01-tool-contracts-and-wrapper.md) - Reasonix capability tools、Wrapper 和 JSON-RPC 结果契约
12. [03-reasonix/02-reasonix-concurrency-model.md](03-reasonix/02-reasonix-concurrency-model.md) - Reasonix 并发、结果合并和写入串行化边界
13. [03-reasonix/03-cache-engineering-model.md](03-reasonix/03-cache-engineering-model.md) - cache key、失效、复用和 cache observability
14. [03-reasonix/04-context-projection-threat-model.md](03-reasonix/04-context-projection-threat-model.md) - 上下文投影威胁模型和对抗测试
15. [04-patch-and-verification/01-patch-transaction-model.md](04-patch-and-verification/01-patch-transaction-model.md) - patch transaction、原子性和回滚语义
16. [04-patch-and-verification/02-patch-safety-checker.md](04-patch-and-verification/02-patch-safety-checker.md) - patch safety checker、敏感路径和安全门禁
17. [04-patch-and-verification/03-verification-gate.md](04-patch-and-verification/03-verification-gate.md) - 验证证据、verification gap 和 completion gate
18. [04-patch-and-verification/04-human-approval-gate.md](04-patch-and-verification/04-human-approval-gate.md) - 人工审批触发、阻塞状态和审计要求
19. [05-versioning/01-schema-contract-and-versioning.md](05-versioning/01-schema-contract-and-versioning.md) - 机器可执行 schema contract 和 schema 演进规则
20. [05-versioning/02-versioning-and-compatibility.md](05-versioning/02-versioning-and-compatibility.md) - Codex / Wrapper / Reasonix / Schema 兼容策略
21. [06-roadmap/01-framework-reassessment.md](06-roadmap/01-framework-reassessment.md) - 框架成熟度复审和实现优先级
22. [06-roadmap/02-roadmap-and-defaults.md](06-roadmap/02-roadmap-and-defaults.md) - MVP 默认配置和路线图
23. [06-roadmap/03-implementation-plan.md](06-roadmap/03-implementation-plan.md) - 关键节点定义、工程规格和实现检查点
24. [schemas/coasonix-v1.schema.json](schemas/coasonix-v1.schema.json) - v1 schema registry

## 目录结构

```text
docs/
  coasonix/
    README.md
    00-executive-summary.md
    01-architecture/
    02-runtime/
    03-reasonix/
    04-patch-and-verification/
    05-versioning/
    06-roadmap/
    schemas/
      coasonix-v1.schema.json
```

## 实现前必读

Wrapper MVP:

```text
01-architecture/02-communication-and-mcp.md
03-reasonix/01-tool-contracts-and-wrapper.md
02-runtime/02-runtime-enforcement-layer.md
02-runtime/03-policy-engine.md
02-runtime/04-schema-enforcement.md
schemas/coasonix-v1.schema.json
```

Runtime Kernel:

```text
02-runtime/01-global-task-state-machine.md
02-runtime/02-runtime-enforcement-layer.md
02-runtime/03-policy-engine.md
02-runtime/04-schema-enforcement.md
02-runtime/05-observability-contract.md
```

Reasonix Project Controller / Session Router:

```text
01-architecture/04-project-session-tool-mapping.md
03-reasonix/02-reasonix-concurrency-model.md
03-reasonix/03-cache-engineering-model.md
03-reasonix/04-context-projection-threat-model.md
```

Patch and verification path:

```text
04-patch-and-verification/01-patch-transaction-model.md
04-patch-and-verification/02-patch-safety-checker.md
04-patch-and-verification/03-verification-gate.md
04-patch-and-verification/04-human-approval-gate.md
```

## 权威来源

| Area | Source of truth |
|---|---|
| Codex / Reasonix roles | [01-architecture/01-overview-and-roles.md](01-architecture/01-overview-and-roles.md) |
| MCP communication | [01-architecture/02-communication-and-mcp.md](01-architecture/02-communication-and-mcp.md) |
| Context projection | [01-architecture/03-context-architecture.md](01-architecture/03-context-architecture.md) |
| Project/session/lane routing | [01-architecture/04-project-session-tool-mapping.md](01-architecture/04-project-session-tool-mapping.md) |
| Runtime gates | [02-runtime/02-runtime-enforcement-layer.md](02-runtime/02-runtime-enforcement-layer.md) |
| Policy rules | [02-runtime/03-policy-engine.md](02-runtime/03-policy-engine.md) |
| Schema registry | [schemas/coasonix-v1.schema.json](schemas/coasonix-v1.schema.json) |
| Reasonix tools | [03-reasonix/01-tool-contracts-and-wrapper.md](03-reasonix/01-tool-contracts-and-wrapper.md) |
| Patch safety | [04-patch-and-verification/02-patch-safety-checker.md](04-patch-and-verification/02-patch-safety-checker.md) |
| Verification evidence | [04-patch-and-verification/03-verification-gate.md](04-patch-and-verification/03-verification-gate.md) |
| Compatibility | [05-versioning/02-versioning-and-compatibility.md](05-versioning/02-versioning-and-compatibility.md) |
| Implementation sequencing | [06-roadmap/03-implementation-plan.md](06-roadmap/03-implementation-plan.md) |

## 当前工程状态

```text
Deterministic Multi-Agent Runtime Spec: complete
Runtime Enforcement Layer design: complete
Global Runtime / Project Controller isolation / Session Pool / session lane mapping: complete
MVP engineering defaults: complete
Safe autonomous operation: blocked until runtime engines and conformance tests are implemented
```

下一阶段应将文档中的 Runtime Kernel、state machine runner、schema validator、policy engine、Reasonix Project Controller、Session Pool、Session Router、patch checker、audit writer 和 STDIO Wrapper 转成可运行实现。
