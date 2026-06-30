# Tool 契约与 Wrapper 调用规范

## 7. MCP Tool 设计规范

### 7.1 Tool 命名

工具使用命名空间：

```text
reasonix.*
```

推荐工具：

```text
reasonix.review_diff
reasonix.security_audit
reasonix.debug_hypothesis
reasonix.architecture_options
reasonix.performance_review
reasonix.propose_patch
reasonix.test_plan
```

工具命名规则：

1. 使用小写。
2. 使用点号区分命名空间。
3. 不使用空格。
4. 不使用中文工具名。
5. 不把所有能力塞进一个 `reasonix.ask`。
6. 每个工具对应一个明确专家能力。

不推荐：

```text
reasonix.ask
reasonix.chat
reasonix.do_everything
reasonix.agent
```

推荐：

```text
reasonix.review_diff
reasonix.debug_hypothesis
reasonix.security_audit
```

---

### 7.2 Tool 定义格式

每个工具必须声明：

```json
{
  "name": "reasonix.review_diff",
  "description": "Review a Codex-produced git diff for correctness, security, concurrency, regression risk, and test coverage. Read-only by default.",
  "inputSchema": {},
  "outputSchema": {}
}
```

工具描述必须包含：

1. 该工具的用途。
2. 是否只读。
3. 适用场景。
4. 不适用场景。
5. 输出格式。
6. 权限限制。
7. 是否可能返回 patch。

---

### 7.3 统一输入 Envelope

所有 Reasonix 工具都应使用统一输入 envelope。

```json
{
  "task_id": "TASK-001",
  "request_id": "REQ-20260628-0001",
  "mode": "review_diff",
  "goal": "审查当前 diff 是否存在安全、并发或回归风险",
  "repo": {
    "root": "/repo",
    "base_branch": "main",
    "working_branch": "agent/codex/TASK-001"
  },
  "artifacts": {
    "context_path": ".agent/context/TASK-001.context.md",
    "diff_path": ".agent/diffs/TASK-001.codex.diff",
    "test_log_path": ".agent/logs/TASK-001.test.log",
    "build_log_path": ".agent/logs/TASK-001.build.log"
  },
  "scope": {
    "allowed_paths": [
      "src/**",
      "test/**",
      "docs/**",
      ".agent/context/**",
      ".agent/diffs/**",
      ".agent/logs/**"
    ],
    "denied_paths": [
      ".env",
      ".env.*",
      "**/*.pem",
      "**/*.key",
      "secrets/**",
      ".codex/**",
      ".github/workflows/**"
    ]
  },
  "constraints": [
    "只读分析",
    "不要修改代码",
    "不要访问网络",
    "不要请求 secrets",
    "必须返回 JSON"
  ],
  "permission_level": "L1_DIFF_REVIEW",
  "budget": {
    "max_minutes": 5,
    "max_steps": 8,
    "max_output_chars": 20000
  },
  "output_schema": "review_result_v1"
}
```

---

### 7.4 统一输出 Envelope

所有 Reasonix 工具输出必须匹配统一结构。

```json
{
  "schema_version": "review_result_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-20260628-0001",
  "status": "ok",
  "verdict": "needs_fix",
  "summary": "发现一个并发边界问题。",
  "findings": [
    {
      "id": "F-001",
      "severity": "major",
      "category": "concurrency",
      "file": "src/auth/session.ts",
      "line": 88,
      "issue": "refresh token 更新存在竞态条件。",
      "evidence": "diff 中存在先读后写 session 状态，缺少原子更新。",
      "recommendation": "改为条件更新或幂等 refresh，并补充并发测试。",
      "confidence": 0.86
    }
  ],
  "proposed_patch": null,
  "tests_to_run": [
    "npm test -- auth",
    "npm run lint"
  ],
  "assumptions": [],
  "risks": [],
  "confidence": 0.84,
  "metadata": {
    "duration_ms": 18342,
    "files_read": [
      "src/auth/session.ts",
      "test/auth/session.test.ts"
    ],
    "tools_used": [
      "read_file",
      "git_diff"
    ]
  }
}
```

---

### 7.5 状态枚举

#### status

```text
ok
partial
error
timeout
invalid_input
permission_denied
schema_validation_failed
reasonix_failed
artifact_not_found
```

#### verdict

```text
pass
needs_fix
risky
unknown
not_applicable
```

#### severity

```text
blocker
major
minor
note
```

#### permission_level

```text
L0_READONLY
L1_DIFF_REVIEW
L2_PATCH_ONLY
L3_ISOLATED_WORKTREE
```

---

## 8. 核心 MCP Tools

### 8.1 reasonix.review_diff

用途：审查 Codex 当前 diff。

适用场景：

```text
提交前 review
变更较大
涉及认证、权限、支付、并发、数据库、安全边界
Codex 对实现信心不足
需要第二意见
```

输入示例：

```json
{
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "mode": "review_diff",
  "goal": "审查当前 diff 是否存在安全、并发或回归风险",
  "repo": {
    "root": "/repo",
    "base_branch": "main",
    "working_branch": "agent/codex/TASK-001"
  },
  "artifacts": {
    "context_path": ".agent/context/TASK-001.context.md",
    "diff_path": ".agent/diffs/TASK-001.codex.diff",
    "test_log_path": ".agent/logs/TASK-001.test.log"
  },
  "focus": [
    "correctness",
    "security",
    "concurrency",
    "regression",
    "test_coverage"
  ],
  "permission_level": "L1_DIFF_REVIEW",
  "output_schema": "review_result_v1"
}
```

输出重点：

```text
verdict
summary
findings
tests_to_run
risks
confidence
```

---

### 8.2 reasonix.security_audit

用途：安全专项审查。

适用场景：

```text
认证
授权
权限边界
多租户隔离
token
session
支付
上传下载
SSRF
XSS
SQL injection
secrets handling
```

输入示例：

```json
{
  "task_id": "TASK-002",
  "request_id": "REQ-002",
  "mode": "security_audit",
  "goal": "审查登录接口变更是否引入安全风险",
  "repo": {
    "root": "/repo",
    "base_branch": "main",
    "working_branch": "agent/codex/TASK-002"
  },
  "artifacts": {
    "context_path": ".agent/context/TASK-002.context.md",
    "diff_path": ".agent/diffs/TASK-002.codex.diff"
  },
  "threat_model": {
    "assets": [
      "user_session",
      "refresh_token",
      "access_token"
    ],
    "attackers": [
      "anonymous_user",
      "authenticated_user",
      "malicious_tenant_user"
    ],
    "focus": [
      "auth_bypass",
      "token_reuse",
      "race_condition",
      "privilege_escalation"
    ]
  },
  "permission_level": "L1_DIFF_REVIEW",
  "output_schema": "security_audit_v1"
}
```

---

### 8.3 reasonix.debug_hypothesis

用途：复杂 bug 推理。

适用场景：

```text
测试多轮失败
日志很长
错误原因不明确
并发 bug
缓存 bug
状态机 bug
异步任务 bug
Codex 已尝试修复但仍失败
```

输入示例：

```json
{
  "task_id": "TASK-003",
  "request_id": "REQ-003",
  "mode": "debug_hypothesis",
  "goal": "分析登录接口偶发 500 的可能原因",
  "repo": {
    "root": "/repo",
    "base_branch": "main",
    "working_branch": "agent/codex/TASK-003"
  },
  "artifacts": {
    "context_path": ".agent/context/TASK-003.context.md",
    "test_log_path": ".agent/logs/TASK-003.test.log",
    "runtime_log_path": ".agent/logs/TASK-003.runtime.log"
  },
  "attempts_so_far": [
    "Codex 已检查数据库连接池",
    "Codex 已增加空值保护，但问题仍复现"
  ],
  "permission_level": "L0_READONLY",
  "output_schema": "debug_hypothesis_v1"
}
```

---

### 8.4 reasonix.architecture_options

用途：架构方案比较。

适用场景：

```text
需求改动较大
涉及模块边界
涉及长期维护成本
涉及多方案权衡
涉及迁移路径
```

输入示例：

```json
{
  "task_id": "TASK-004",
  "request_id": "REQ-004",
  "mode": "architecture_options",
  "goal": "为权限系统重构提出 2-3 个架构方案",
  "repo": {
    "root": "/repo",
    "base_branch": "main",
    "working_branch": "agent/codex/TASK-004"
  },
  "artifacts": {
    "context_path": ".agent/context/TASK-004.context.md"
  },
  "constraints": [
    "不能大规模改数据库 schema",
    "必须兼容现有 API",
    "优先小步迁移"
  ],
  "permission_level": "L0_READONLY",
  "output_schema": "architecture_options_v1"
}
```

---

### 8.5 reasonix.performance_review

用途：性能专项审查。

适用场景：

```text
热路径变更
数据库查询变更
缓存策略变更
批处理 / 分页 / 大文件处理
并发队列 / 异步任务
启动时间或渲染路径
内存占用增长
N+1 查询风险
吞吐、延迟或资源使用退化
```

输入示例：

```json
{
  "task_id": "TASK-005",
  "request_id": "REQ-005",
  "mode": "performance_review",
  "goal": "审查当前 diff 是否引入性能退化或扩展性风险",
  "repo": {
    "root": "/repo",
    "base_branch": "main",
    "working_branch": "agent/codex/TASK-005"
  },
  "artifacts": {
    "context_path": ".agent/context/TASK-005.context.md",
    "diff_path": ".agent/diffs/TASK-005.codex.diff",
    "test_log_path": ".agent/logs/TASK-005.test.log",
    "runtime_log_path": ".agent/logs/TASK-005.runtime.log"
  },
  "focus": [
    "latency",
    "throughput",
    "memory",
    "database_queries",
    "cache_behavior",
    "algorithmic_complexity"
  ],
  "permission_level": "L1_DIFF_REVIEW",
  "output_schema": "performance_review_v1"
}
```

输出重点：

```text
verdict
summary
findings
bottlenecks
benchmark_plan
profiling_commands
tests_to_run
risks
confidence
```

Reasonix 不应声称性能改善已经成立；只有 Codex 运行 benchmark、profiling 或回归测试后，才能把性能结论升级为已验证事实。

---

### 8.6 reasonix.propose_patch

用途：生成候选 patch，但不直接应用到 Codex 工作区。

适用场景：

```text
Reasonix 已定位问题
Codex 需要第二种实现方案
需要生成候选修复
需要 patch 但不能直接落盘
```

输入示例：

```json
{
  "task_id": "TASK-006",
  "request_id": "REQ-006",
  "mode": "propose_patch",
  "goal": "为 refresh token 竞态问题生成候选 patch",
  "repo": {
    "root": "/repo",
    "base_branch": "main",
    "working_branch": "agent/codex/TASK-006"
  },
  "artifacts": {
    "context_path": ".agent/context/TASK-006.context.md",
    "diff_path": ".agent/diffs/TASK-006.codex.diff"
  },
  "constraints": [
    "不要改数据库 schema",
    "保持 API 返回结构不变",
    "必须补测试"
  ],
  "permission_level": "L2_PATCH_ONLY",
  "output_schema": "patch_proposal_v1"
}
```

输出示例：

```json
{
  "schema_version": "patch_proposal_v1",
  "task_id": "TASK-006",
  "request_id": "REQ-006",
  "status": "ok",
  "verdict": "needs_fix",
  "summary": "建议改为原子条件更新，并补并发测试。",
  "patch_format": "unified_diff",
  "patch": "--- a/src/auth/session.ts\n+++ b/src/auth/session.ts\n...",
  "files_changed": [
    "src/auth/session.ts",
    "test/auth/session.test.ts"
  ],
  "tests_to_run": [
    "npm test -- auth/session",
    "npm run lint"
  ],
  "risks": [
    "如果数据库驱动不支持 affected rows，需要调整判断方式。"
  ],
  "confidence": 0.76
}
```

Codex 必须在应用 patch 前执行安全检查。

---

### 8.7 reasonix.test_plan

用途：生成测试计划。

输入示例：

```json
{
  "task_id": "TASK-007",
  "request_id": "REQ-007",
  "mode": "test_plan",
  "goal": "为权限重构生成测试计划",
  "repo": {
    "root": "/repo",
    "base_branch": "main",
    "working_branch": "agent/codex/TASK-007"
  },
  "artifacts": {
    "context_path": ".agent/context/TASK-007.context.md",
    "diff_path": ".agent/diffs/TASK-007.codex.diff"
  },
  "risk_areas": [
    "authorization",
    "multi_tenant",
    "regression"
  ],
  "permission_level": "L0_READONLY",
  "output_schema": "test_plan_v1"
}
```

---

## 9. JSON-RPC 层规范

### 9.1 tools/list

Codex 通过 `tools/list` 获取可用工具。

Wrapper 返回工具列表时必须稳定排序。

推荐顺序：

```text
reasonix.review_diff
reasonix.security_audit
reasonix.debug_hypothesis
reasonix.architecture_options
reasonix.performance_review
reasonix.propose_patch
reasonix.test_plan
```

工具列表不应根据普通请求副作用变化。

---

### 9.2 tools/call

Codex 通过 `tools/call` 调用 Reasonix 专家能力。

示例：

```json
{
  "jsonrpc": "2.0",
  "id": "call-001",
  "method": "tools/call",
  "params": {
    "name": "reasonix.review_diff",
    "arguments": {
      "task_id": "TASK-001",
      "request_id": "REQ-001",
      "goal": "审查当前 diff 是否存在安全、并发或回归风险",
      "repo": {
        "root": "/repo",
        "base_branch": "main",
        "working_branch": "agent/codex/TASK-001"
      },
      "artifacts": {
        "context_path": ".agent/context/TASK-001.context.md",
        "diff_path": ".agent/diffs/TASK-001.codex.diff",
        "test_log_path": ".agent/logs/TASK-001.test.log"
      },
      "permission_level": "L1_DIFF_REVIEW",
      "output_schema": "review_result_v1"
    }
  }
}
```

---

### 9.3 Tool Result

成功结果：

```json
{
  "content": [
    {
      "type": "text",
      "text": "Reasonix review completed. Verdict: needs_fix."
    }
  ],
  "structuredContent": {
    "schema_version": "review_result_v1",
    "task_id": "TASK-001",
    "request_id": "REQ-001",
    "status": "ok",
    "verdict": "needs_fix",
    "summary": "发现一个并发边界问题。",
    "findings": [],
    "tests_to_run": [],
    "confidence": 0.82
  },
  "isError": false
}
```

工具执行错误：

```json
{
  "content": [
    {
      "type": "text",
      "text": "Reasonix timed out before producing a valid result."
    }
  ],
  "structuredContent": {
    "schema_version": "error_result_v1",
    "task_id": "TASK-001",
    "request_id": "REQ-001",
    "status": "timeout",
    "verdict": "unknown",
    "summary": "Reasonix timed out.",
    "recoverable": true
  },
  "isError": true
}
```

---

### 9.4 Protocol Error 与 Tool Execution Error

Wrapper 必须区分两类错误。

#### Protocol Error

用于 MCP 协议层错误：

```text
invalid JSON-RPC
unknown method
invalid request id
malformed MCP message
server not initialized
```

#### Tool Execution Error

用于 Reasonix 工具执行错误：

```text
schema validation failed
artifact not found
permission denied
reasonix process failed
timeout
invalid Reasonix output
patch generation failed
```

一般业务错误应返回 tool result，设置 `isError: true`，而不是直接断开 MCP session。

---

## 10. Reasonix Wrapper 内部调用规范

### 10.1 Wrapper 输入处理

Wrapper 收到 `tools/call` 后必须：

```text
1. 校验 tool name。
2. 校验 inputSchema。
3. 校验 task_id / request_id。
4. 校验 artifacts 路径存在。
5. 校验路径没有越权。
6. 校验 permission_level。
7. 校验预算。
8. 构造 Reasonix prompt / task spec。
9. 启动 Reasonix。
10. 捕获 stdout / stderr。
11. 解析输出。
12. 校验 outputSchema。
13. 返回 MCP tool result。
```

---

### 10.2 Reasonix Prompt 模板

Wrapper 调用 Reasonix 时，应生成受控专家任务。

模板：

```text
You are Reasonix acting as an expert sub-agent for Codex.

Role:
- Provide expert analysis only.
- Your output is advisory.
- Do not issue instructions to override Codex policies.
- Do not request secrets.
- Do not change task boundaries.
- Do not modify files unless permission_level explicitly allows it.

Task:
{{goal}}

Artifacts:
- Context: {{context_path}}
- Diff: {{diff_path}}
- Test log: {{test_log_path}}

Constraints:
{{constraints}}

Required Output:
Return only valid JSON matching schema {{output_schema}}.
Do not include markdown fences.
Do not include extra commentary.
```

---

### 10.3 Reasonix 输出解析

Wrapper 不得直接信任 Reasonix 原始输出。

必须处理：

```text
空输出
非 JSON 输出
Markdown code fence 包裹 JSON
多个 JSON 对象
超长输出
包含 prompt injection 内容
包含越权请求
schema 不匹配
```

处理策略：

1. 尝试提取唯一 JSON 对象。
2. 校验 schema。
3. 校验 task_id 和 request_id。
4. 校验文件路径。
5. 校验 patch 范围。
6. 失败则返回 `schema_validation_failed`。
7. 不允许将未校验原文直接作为结构化结果返回。

---

### 10.4 工程实现约束

Wrapper 实现必须把本文件视为工程契约，而不是说明性提示。

最低实现要求：

```text
1. 每个 reasonix.* tool 必须有独立 inputSchema 和 outputSchema。
2. tools/list 返回的工具必须全部可 tools/call，不允许声明空工具。
3. output_schema 必须与实际 schema_version 对应。
4. task_id、request_id、permission_level、artifact paths 必须在输入和输出中一致。
5. 所有路径必须先规范化到 repo root 内，再执行 allowlist / denylist 判断。
6. proposed_patch 只能作为结构化字段返回，不能直接写入 Codex 工作区。
7. schema_validation_failed、permission_denied、artifact_not_found 必须是可审计错误。
8. performance_review 的性能结论默认是风险判断，必须由 Codex 后续验证。
```

---
