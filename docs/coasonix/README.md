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
11. [02-runtime/06-executable-runtime-details.md](02-runtime/06-executable-runtime-details.md) - canonicalization、matcher、cache、audit、verification 和 approval 的可执行 runtime 细节
12. [02-runtime/07-sqlite-persistence.md](02-runtime/07-sqlite-persistence.md) - repo-local SQLite state、audit、lock 和 cache metadata 持久化契约
13. [03-reasonix/01-tool-contracts-and-wrapper.md](03-reasonix/01-tool-contracts-and-wrapper.md) - Reasonix capability tools、Wrapper 和 JSON-RPC 结果契约
14. [03-reasonix/02-reasonix-concurrency-model.md](03-reasonix/02-reasonix-concurrency-model.md) - Reasonix 并发、结果合并和写入串行化边界
15. [03-reasonix/03-cache-engineering-model.md](03-reasonix/03-cache-engineering-model.md) - cache key、失效、复用和 cache observability
16. [03-reasonix/04-context-projection-threat-model.md](03-reasonix/04-context-projection-threat-model.md) - 上下文投影威胁模型和对抗测试
17. [04-patch-and-verification/01-patch-transaction-model.md](04-patch-and-verification/01-patch-transaction-model.md) - patch transaction、原子性和回滚语义
18. [04-patch-and-verification/02-patch-safety-checker.md](04-patch-and-verification/02-patch-safety-checker.md) - patch safety checker、敏感路径和安全门禁
19. [04-patch-and-verification/03-verification-gate.md](04-patch-and-verification/03-verification-gate.md) - 验证证据、verification gap 和 completion gate
20. [04-patch-and-verification/04-human-approval-gate.md](04-patch-and-verification/04-human-approval-gate.md) - 人工审批触发、阻塞状态和审计要求
21. [05-versioning/01-schema-contract-and-versioning.md](05-versioning/01-schema-contract-and-versioning.md) - 机器可执行 schema contract 和 schema 演进规则
22. [05-versioning/02-versioning-and-compatibility.md](05-versioning/02-versioning-and-compatibility.md) - Codex / Wrapper / Reasonix / Schema 兼容策略
23. [06-roadmap/01-framework-reassessment.md](06-roadmap/01-framework-reassessment.md) - 框架成熟度复审和实现优先级
24. [06-roadmap/02-roadmap-and-defaults.md](06-roadmap/02-roadmap-and-defaults.md) - MVP 默认配置和路线图
25. [06-roadmap/03-implementation-plan.md](06-roadmap/03-implementation-plan.md) - 关键节点定义、工程规格和实现检查点
26. [06-roadmap/04-technology-selection.md](06-roadmap/04-technology-selection.md) - Rust Runtime Core + TypeScript MCP Adapter 技术选型
27. [06-roadmap/05-v1-mvp-scope.md](06-roadmap/05-v1-mvp-scope.md) - v1-core / v1-adapter 第一版实现范围
28. [06-roadmap/06-runtime-core-api.md](06-roadmap/06-runtime-core-api.md) - Rust Runtime Core API 和数据模型边界
29. [06-roadmap/07-v1-implementation-blueprint.md](06-roadmap/07-v1-implementation-blueprint.md) - v1 分层实施蓝图、里程碑和测试矩阵
30. [../implementation/codex-side-gateway-roadmap.md](../implementation/codex-side-gateway-roadmap.md) - Codex 侧 MCP gateway 安装、healthcheck 和后端 worker contract 路线
31. [../../schemas/coasonix-v1.schema.json](../../schemas/coasonix-v1.schema.json) - v1 schema registry

## 目录结构

```text
docs/
  coasonix/
    README.md
    00-executive-summary.md
    01-architecture/
    02-runtime/
      06-executable-runtime-details.md
      07-sqlite-persistence.md
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
02-runtime/06-executable-runtime-details.md
../../schemas/coasonix-v1.schema.json
```

Runtime Kernel:

```text
02-runtime/01-global-task-state-machine.md
02-runtime/02-runtime-enforcement-layer.md
02-runtime/03-policy-engine.md
02-runtime/04-schema-enforcement.md
02-runtime/05-observability-contract.md
02-runtime/06-executable-runtime-details.md
02-runtime/07-sqlite-persistence.md
06-roadmap/06-runtime-core-api.md
06-roadmap/07-v1-implementation-blueprint.md
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
| Executable runtime details | [02-runtime/06-executable-runtime-details.md](02-runtime/06-executable-runtime-details.md) |
| SQLite persistence | [02-runtime/07-sqlite-persistence.md](02-runtime/07-sqlite-persistence.md) |
| Schema registry | [../../schemas/coasonix-v1.schema.json](../../schemas/coasonix-v1.schema.json) |
| Reasonix tools | [03-reasonix/01-tool-contracts-and-wrapper.md](03-reasonix/01-tool-contracts-and-wrapper.md) |
| Patch safety | [04-patch-and-verification/02-patch-safety-checker.md](04-patch-and-verification/02-patch-safety-checker.md) |
| Verification evidence | [04-patch-and-verification/03-verification-gate.md](04-patch-and-verification/03-verification-gate.md) |
| Compatibility | [05-versioning/02-versioning-and-compatibility.md](05-versioning/02-versioning-and-compatibility.md) |
| Implementation sequencing | [06-roadmap/03-implementation-plan.md](06-roadmap/03-implementation-plan.md) |
| Technology selection | [06-roadmap/04-technology-selection.md](06-roadmap/04-technology-selection.md) |
| v1 MVP scope | [06-roadmap/05-v1-mvp-scope.md](06-roadmap/05-v1-mvp-scope.md) |
| Runtime core API | [06-roadmap/06-runtime-core-api.md](06-roadmap/06-runtime-core-api.md) |
| v1 implementation blueprint | [06-roadmap/07-v1-implementation-blueprint.md](06-roadmap/07-v1-implementation-blueprint.md) |

## 当前工程状态

```text
Deterministic Multi-Agent Runtime Spec: complete
Runtime Enforcement Layer design: complete
Global Runtime / Project Controller isolation / Session Pool / session lane mapping: complete
MVP engineering defaults: complete
v1 technology baseline: Rust 2024 core, Bun ESM adapter, JSON-RPC stdio worker, SQLite persistence
v1 implementation blueprint: complete through M13
v1 MVP implementation: complete for Rust-gated reasonix.review_diff through a runnable MCP stdio server
Codex-side gateway productization: M12 setup installer and M13 healthcheck implemented with mock profile validation
Safe autonomous patch operation: still blocked until patch safety, approval, and verification gates are implemented
```

当前实现入口：

```text
../../crates/coasonix-runtime-core/      Rust RuntimeKernel、schema、policy、state、audit、SQLite storage
../../crates/coasonix-runtime-worker/    JSON-RPC stdio Runtime Worker
../../packages/reasonix-expert-mcp/      TypeScript MCP stdio server、adapter、worker client、mock Reasonix runner
../implementation/v1-mvp-execution-plan.md
../implementation/codex-side-gateway-roadmap.md
```

当前 v1 已完成的边界是只读 `reasonix.review_diff` 垂直切片，并且该切片已经可通过本地 MCP stdio server 挂载。下一阶段不应继续扩展工具列表，除非同步补齐对应 schema、runtime gate、denial cases、malformed-output cases、audit events 和 verification tests。明确仍属 post-v1 的能力包括真实 Reasonix credentials、`reasonix.propose_patch`、patch apply / transaction commit、human approval UI、network allow exceptions、remote HTTP transport 和 local daemon。

下一阶段优先做 Codex 侧 gateway 产品化：M12 已实现可复现
`setup:codex-mcp` 安装器、mock profile 和 Codex 注册验证；M13 已实现
`health:codex-mcp`，可分层验证 Codex registration、server startup、
runtime initialize、tools/list、mock review_diff 和 shutdown。下一步是
mock/conformance worker，以及后端中立的 agent worker contract。
Reasonix、MimoCode 和其他智能体应作为后续 backend bridge 接入，
不应直接牵动 Codex MCP shell、Rust runtime gate 或 schema/audit 核心。
