# 落地路线与默认配置

## 17. 落地路线

### 17.1 Phase 1：MCP STDIO Wrapper

目标：最小可行。

MVP 工程默认：

```text
1. local single-machine STDIO transport.
2. Runtime Kernel embedded inside `reasonix-expert` Wrapper.
3. one Coasonix task should use one isolated git worktree by default.
4. same worktree write operations are serialized.
5. Reasonix session lanes are task-scoped: session_key includes task_id.
6. Reasonix project memory may generate hypotheses, not verification evidence.
7. `reasonix.propose_patch` returns patch_proposal_v1 only and never writes the Codex worktree.
8. remote Reasonix worker / shared Gateway is deferred to a later deployment profile.
```

实现：

```text
tools/reasonix-mcp-server/
  package.json
  src/
    index.ts
    tools/
      review_diff.ts
      security_audit.ts
      debug_hypothesis.ts
      architecture_options.ts
      performance_review.ts
      propose_patch.ts
      test_plan.ts
    schemas/
      review_result_v1.json
      security_audit_v1.json
      debug_hypothesis_v1.json
      architecture_options_v1.json
      performance_review_v1.json
      patch_proposal_v1.json
      test_plan_v1.json
```

文档侧 canonical schema registry：

```text
../schemas/coasonix-v1.schema.json
```

实现时可以选择拆分为多个 schema 文件，但必须保持与 canonical registry 等价，并通过 Draft 2020-12 校验。

Codex 配置：

```toml
[mcp_servers.reasonix_expert]
command = "node"
args = ["./tools/reasonix-mcp-server/dist/index.js"]
cwd = "."
startup_timeout_sec = 10
tool_timeout_sec = 300
default_tools_approval_mode = "prompt"
```

---

### 17.2 Phase 2：权限与审计

新增：

```text
.agent/policy.yaml
.agent/audit/*.jsonl
schema validation
path allowlist / denylist
Reasonix Project Controller
Reasonix Session Pool
Reasonix session lane router
patch safety checker
timeout manager
```

---

### 17.3 Phase 3：Streamable HTTP

适用：

```text
团队共享
CI 调用
远程 Reasonix worker
多仓库统一服务
```

新增：

```text
Bearer token
Origin validation
rate limit
request queue
central audit store
worker pool
```

---

### 17.4 Phase 4：生产级 Orchestrator

组件：

```text
Task DB
Queue
Git Worktree Manager
Codex Session Manager
Reasonix MCP Gateway
Policy Engine
Audit Store
Dashboard
CI Adapter
```

---

## 18. 推荐默认配置

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
  may_merge_code: false
  may_request_secrets: false
  output_format: structured_json
  project_session_routing:
    project_controller_scope: repo_worktree_config_policy_runtime
    gateway_may_be_global: true
    different_projects_use_isolated_project_controllers: true
    multiple_codex_sessions_share_project_controller: true
    task_namespace_isolation: true
    same_repo_worktree_reuses_project: true
    session_is_cache_boundary_not_memory_boundary: true
    cross_project_session_reuse: false
    cross_project_result_cache_reuse: false
    global_static_prefix_cache_allowed_when_project_neutral: true
    cross_tool_continuity: explicit_task_state_and_artifacts
    mvp_session_lane_scope: task
    future_session_lane_scope: project_when_conformance_tests_pass
    reasonix_memory_evidence_level: hypothesis_only
    reasonix_internal_agents_visible_to_codex: false
    patch_direct_write_allowed: false
    runtime_kernel_deployment_mvp: embedded_in_wrapper
    transport_mvp: stdio
    transport_future: streamable_http
    default_lanes:
      review:
        - reasonix.review_diff
        - reasonix.test_plan
      security:
        - reasonix.security_audit
      debug:
        - reasonix.debug_hypothesis
      performance:
        - reasonix.performance_review
      architecture:
        - reasonix.architecture_options
      patch:
        - reasonix.propose_patch

mcp:
  server_name: reasonix_expert
  phase_1_transport: stdio
  phase_2_transport: streamable_http
  expose_tools_only: true
  expose_sampling: false
  expose_elicitation: false
  expose_tasks: false
  expose_resources_initially: false

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
  require_tests_after_patch: true
  require_audit_log: true

human_approval:
  required_for:
    - auth_core_changes
    - payment_changes
    - database_migrations
    - deployment_changes
    - ci_changes
    - secret_access
    - network_access
    - deleting_tests
```

---

## 19. 最终原则

本系统的最终原则是：

```text
Codex 是主控。
Reasonix 是专家。
Wrapper 是协议适配层。
MCP 是控制面。
Git / diff / files / logs 是事实载体。
JSON Schema 是结果契约。
CI 是验证层。
Policy 是边界。
Audit 是可追溯性。
Human approval 是高风险兜底。
Reasonix Project 是 repo/worktree/config 边界。
Reasonix Session Lane 是 cache-stable 推理边界，不是隐藏记忆。
不同项目之间隔离 Project Controller、Session Pool、Task Registry、Artifact Registry、Policy Runtime 和非全局 cache。
```

一句话总结：

```text
让 Codex 像调用高质量专家工具一样调用 Reasonix：
按需委派、最小上下文、结构化返回、严格验证、可审计执行、失败可回滚、风险可审批。
```
