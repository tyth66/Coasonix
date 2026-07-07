# Coagent v3 Blueprint — Multi-Agent ACP Runtime

This document captures the architectural vision for upgrading Coagent from a
Reasonix-specific adapter into a general-purpose multi-agent ACP runtime.

**Status: Blueprint — not yet implemented.**

---

## Target Identity

```
Coagent v3 = Codex-facing multi-agent runtime for ACP-compatible expert agents.
```

Coagent is the local runtime layer between Codex and external intelligent agents.
It owns: tool registration, task/operation/attempt state, permission gating,
context projection, ACP session management, backend selection, audit, and recovery.

Reasonix becomes **one backend plugin among many**, not the architectural center.

## Core Abstractions (v3)

```
Tool        = capability entry point (e.g. coagent.review_diff)
Task        = Codex-initiated goal, long-lived
Operation   = one tool invocation within a task
Attempt     = one actual backend execution of an operation
Backend     = ACP-compatible agent that executes operations
Session     = long-lived ACP connection, scoped by backend+project+task
Artifact    = file/resource authorized for read or write
Policy      = what a tool may request AND what a backend is trusted to do
Audit       = full event-sourcing trail across all layers
```

## Target Architecture

```
Codex MCP Host
  |
  v
Coagent MCP Gateway
  |
  +-- ToolRegistry
  |     +-- coagent.review_diff
  |     +-- coagent.review_architecture
  |     +-- coagent.review_tests
  |     +-- coagent.security_audit
  |
  +-- RuntimeToolExecutor (already unified pipeline)
  |     +-- schema validation
  |     +-- ID enforcement
  |     +-- runtime gate
  |     +-- backend selection
  |     +-- backend invoke
  |     +-- output validation
  |     +-- lifecycle close
  |
  +-- RuntimeKernel
  |     +-- TaskState
  |     +-- OperationState
  |     +-- AttemptState
  |     +-- PolicyEngine
  |     +-- AuditStore
  |     +-- ArtifactStore
  |
  +-- BackendRegistry
  |     +-- Reasonix ACP Backend
  |     +-- Other ACP Backend
  |     +-- Mock Backend
  |
  +-- AcpSessionPool
        +-- session per backend
        +-- session per project
        +-- session per task
        +-- reconnect / retry / health check
```

## Key Design Decisions

### 1. Backend Trait (not Reasonix-specific)

```rust
#[async_trait]
pub trait AgentBackend {
    async fn invoke(&self, request: BackendRequest) -> Result<BackendResponse, BackendError>;
    fn backend_id(&self) -> &str;
    fn capabilities(&self) -> BackendCapabilities;
}

pub struct AcpBackend {
    backend_id: String,
    profile: AgentProfile,
    session_pool: AcpSessionPool,
}
```

### 2. Tool-Backend Decoupling

Tools are capability entry points, not backend bindings:

```
coagent.review_diff → can be executed by Reasonix, OtherACP, or Mock
coagent.security_audit → can be executed by SecurityAgent
```

MCP tool names should use the `coagent.` prefix, not `reasonix.`.

### 3. ToolSpec Declarative Registration

```rust
pub struct ToolSpec {
    pub name: String,
    pub version: String,
    pub input_schema: String,
    pub output_schema: String,
    pub permission_level: PermissionLevel,
    pub artifact_policy: ArtifactPolicySpec,
    pub context_projector: Box<dyn ContextProjector>,
    pub backend_selector: BackendSelector,
    pub response_wrapper: ResponseWrapperSpec,
}
```

Adding a new tool = adding a ToolSpec, not copying 180 lines of handler.

### 4. AgentProfile Per Backend

```toml
[backends.reasonix]
protocol = "acp"
command = "reasonix"
args = ["acp", "--model", "deepseek-v4-flash"]
capabilities = ["code.review.diff", "architecture.review", "test.review"]
session_policy = "per_project_task"
trust_level = "review_only"
```

### 5. Session Pool

```
Key = backend_id + project_id + task_id
Policies: per_backend_global | per_project | per_task | per_project_task
```

Default: `per_project_task` — reuses sessions within same project+task,
isolates across projects and tasks.

### 6. Task / Operation / Attempt (3-layer)

```
TASK-001: Fix runtime audit design
  OP-001: coagent.review_architecture
    ATTEMPT-1: reasonix → success
  OP-002: coagent.review_diff
    ATTEMPT-1: reasonix → protocol error
    ATTEMPT-2: reasonix → reconnect → success
  OP-003: coagent.security_audit
    ATTEMPT-1: security_agent → success
```

### 7. BackendSelector

```rust
pub trait BackendSelector {
    fn select(
        &self,
        tool: &ToolSpec,
        task: &TaskContext,
        available_backends: &[AgentProfile],
    ) -> Result<BackendId, SelectionError>;
}
```

Selection criteria: required capability → project policy → backend health →
trust level → session availability → cost/latency → fallback order.

### 8. Unified Output Protocol

All backends must return structured `BackendResponse`, not free text:

```rust
pub struct BackendResponse {
    pub output_schema: String,
    pub payload: serde_json::Value,
    pub evidence: Vec<EvidenceRef>,
    pub usage: BackendUsage,
    pub backend_metadata: BackendMetadata,
}
```

### 9. Dual-Layer Permissions

- **Tool permission**: what the tool is allowed to request
- **Backend trust level**: what the backend is trusted to execute

```
backend.reasonix      → trust_level: review_only
backend.patch_agent   → trust_level: patch_proposal (cannot apply)
backend.build_agent   → trust_level: isolated_execution (sandboxed)
```

Coagent is a capability gate, not a transparent proxy.

### 10. Context Isolation

Three context layers:

- **Canonical Task Context**: Coagent-owned facts, operation results, artifact refs
- **Projected Tool Context**: what this tool+operation projects to the backend
- **Backend Session Context**: ACP agent's private long-lived session

Rule: different backends never share raw sessions. Different tasks default to
isolated sessions. Shared context is Coagent's canonical context only.

## Proposed Module Layout (v3)

```
crates/
  coagent-runtime-core/
    src/
      task/
      operation/
      attempt/
      policy/
      audit/
      artifact/
      schema/
      scheduler/
      store/

  coagent-backend-acp/
    src/
      client.rs
      session.rs
      session_pool.rs
      profile.rs
      backend.rs

  coagent-backend-mock/
    src/

  coagent-mcp-server/
    src/
      gateway/
      tool_router/
      tool_specs/
      executor/

  coagent-tools/
    src/
      review_diff/
      review_architecture/
      review_tests/
      security_audit/
```

Core principle:

```
runtime-core knows nothing about Reasonix.
backend-acp knows nothing about review_diff.
mcp-server does not manage sessions directly.
tools do not write audit directly.
executor orchestrates everything.
```

## Recommended Evolution Path

| Phase | Scope | Dependencies |
|-------|-------|-------------|
| Phase 1 | `AgentBackend` trait, `BackendRequest/Response`, `BackendRegistry`, `AgentProfile`, `AcpBackend` | P1 pipeline already done |
| Phase 2 | `ToolSpec` declarative registration, decouple `reasonix.review_diff` → `coagent.review_diff` | Phase 1 |
| Phase 3 | `AcpSessionPool`, multi-key session management, session policies | Phase 1 |
| Phase 4 | `Task/Operation/Attempt` 3-layer state, `operation_attempts` table | P2 two-layer done |
| Phase 5 | `BackendSelector`, multi-backend fallback, health scoring | Phase 1+2+3 |

## Core Principle

> Do not make Coagent an ACP proxy forwarder.
> Make Coagent an ACP Agent Runtime.

```
Proxy forwarder:
  Codex → Coagent → Agent → raw output back

Agent Runtime:
  Codex → Coagent
    → select tool
    → select backend
    → create operation
    → project context
    → permission check
    → ACP invoke
    → schema validate
    → retry / fallback
    → audit
    → return structured result
```
