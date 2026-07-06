# Technology Selection

This document records the implementation technology decisions for Coagent v1.
It complements the node-level design in `03-implementation-plan.md`.

## 1. Decision

Coagent v1 uses:

```text
Rust Runtime Core (crates/coagent-runtime-core)
Rust JSON-RPC stdio Runtime Worker (crates/coagent-runtime-worker)
TypeScript reasonix-expert MCP Adapter (packages/reasonix-expert-mcp)
JSON-RPC 2.0 over stdio between TypeScript and Rust
Rust edition 2021
Bun toolchain for TypeScript workspace, build, and tests
ES Modules for TypeScript package format
JSON Schema 2020-12 for review_diff test contracts
Repo-local SQLite at .agent/coagent.sqlite
Root-level review_diff test contract fixture at schemas/coagent-v1.schema.json
```

The architecture is:

```text
Codex MCP Host
  -> TypeScript reasonix-expert MCP Adapter
      -> managed Rust Runtime Worker (Bun.spawn over stdio)
          -> Rust Runtime Core
      -> Reasonix CLI / mock worker
```

Hard rule:

```text
No side effect is allowed unless the Rust Runtime Worker returns allow.
```

## 2. Technology Baseline Matrix

| Area | v1 choice | Boundary |
|---|---|---|
| MCP adapter language | TypeScript | Owns MCP protocol and process supervision. |
| Runtime core language | Rust edition 2021 | Owns enforceable schema, state, policy, audit gates. |
| TypeScript toolchain | Bun | Package management, scripts, build, tests. |
| TypeScript module format | ES Modules | Bun is runtime/toolchain, not module system. |
| MCP transport | STDIO | Codex starts MCP server; no v1 daemon or HTTP. |
| Internal TS-Rust protocol | JSON-RPC 2.0 over stdio | Internal worker protocol; not an MCP server. |
| Schema dialect | JSON Schema 2020-12 | Root registry is the contract fixture. |
| Runtime database | SQLite | Repo-local: state, audit, locks, decisions, cache metadata. |
| Artifact storage | Files under .agent/ | SQLite stores metadata and hashes. |
| Remote service | None in v1 | Streamable HTTP is post-v1. |

## 3. Rust Runtime Core

Rust owns the enforceable security and correctness boundary.

**Implemented modules:**

```text
kernel/mod.rs     - RuntimeKernel: state + policy orchestration, decision merge, audit persistence
policy/mod.rs     - PolicyEngine: operation registration, permission level, path, argv, network checks
storage/mod.rs    - RuntimeStore: SQLite with 10 migrations, append-only audit (triggers), WAL, FK
schema/mod.rs     - SchemaRegistry: JSON Schema validation + duplicate-key detection
state/mod.rs      - TaskState FSM: Created -> Running -> Completed/Failed
artifact/mod.rs   - ArtifactPolicy: path allowlist/denylist with glob matching
canonical/mod.rs  - Path canonicalization
```

**Post-v1 modules (design only):**

```text
patch/            - Patch safety checker / dry-run / transaction
verification/     - Verification runner
approval/         - Human approval lifecycle
cache/            - Cache reuse engine
```

Rust does not own:

```text
MCP initialize / tools/list / tools/call
Codex-facing conversation
natural-language task interpretation
Reasonix internal agent selection
final user response generation
```

## 4. TypeScript MCP Adapter

TypeScript owns the protocol adapter and process supervision layer.

**Implemented modules:**

```text
mcp/server.ts                - MCP stdio server lifecycle (initialize, tools/list, tools/call)
mcp/adapter.ts               - Tool call orchestration: normalize -> runtime gate -> agent -> validate -> wrap
mcp/tools/review-diff.ts     - review_diff tool handler (input/output schema, normalizeInput, buildRuntimeRequest, invokeAgent, validateOutput)
mcp/types.ts                 - ToolHandler, ToolResult, RuntimeClient interfaces
runtime/RuntimeWorkerClient.ts - JSON-RPC 2.0 stdio client (Bun.spawn, request/response framing, timeout, reconnect)
runtime/protocol.ts          - Frame encode/decode (JSON-RPC 2.0 over newline-delimited stdio)
runtime/errors.ts            - RuntimeWorkerError classification
agent/worker-contract.ts     - Agent worker stdio contract + conformance checks
agent/error-taxonomy.ts      - 14 error codes across 6 layers
agent/backend-profile.ts     - Backend profile configuration (mock, reasonix)
agent/naming.ts              - Internal naming constants
config.ts                    - Environment-based server config
codex/setup.ts               - Codex MCP registration
codex/health.ts              - Healthcheck system
backends/mock/MockRunner.ts  - Mock Reasonix (hardcoded review_result_v1 echo)
backends/reasonix/ReasonixRunner.ts - Real Reasonix CLI bridge (process spawn)
backends/core/interfaces.ts  - AgentRunner interface
backends/core/output-normalizer.ts - JSON extraction from stdout
```

TypeScript must not be the final authority for:

```text
allow / deny / require_approval decisions
path policy, shell policy, network policy
patch safety, cache reuse, task completion
verification completion, approval unblock
```

## 5. JSON-RPC 2.0 over Stdio

Transport:

```text
stdin:  JSON-RPC 2.0 requests (one line = one frame)
stdout: JSON-RPC 2.0 responses only
stderr: structured logs only
```

v1 worker allowed methods (implemented in `coagent-runtime-worker/src/main.rs`):

```text
runtime.initialize           -> RuntimeKernel::initialize(config)
runtime.evaluate_operation   -> RuntimeKernel::evaluate_operation(request)
runtime.write_audit          -> RuntimeKernel::write_audit(event)
runtime.shutdown             -> returns { shutdown: true }
```

Post-v1 candidate methods:

```text
runtime.transition_state
runtime.evaluate_policy
runtime.freeze_snapshot
runtime.evaluate_cache
runtime.check_patch
runtime.run_verification_gate
runtime.request_approval
runtime.resolve_approval
```

## 6. Repository Layout (Actual)

```text
Cargo.toml                         # Rust workspace (2 crates)
Cargo.lock
package.json                       # Bun workspace (1 package)
bun.lock
schemas/
  coagent-v1.schema.json           # review_diff test contract fixture
crates/
  coagent-runtime-core/            # kernel, policy, storage, schema, state, artifact, canonical
    src/
      lib.rs
      kernel/mod.rs                # RuntimeKernel
      policy/mod.rs                # PolicyEngine
      state/mod.rs                 # TaskState
      storage/mod.rs               # RuntimeStore (SQLite, 10 migrations)
      schema/mod.rs                # SchemaRegistry
      artifact/mod.rs              # ArtifactPolicy
      canonical/mod.rs             # Path canonicalization
    tests/                         # 6 test files
  coagent-runtime-worker/          # JSON-RPC stdio worker (thin main.rs)
    src/main.rs                    # Worker: stdin/stdout JSON-RPC loop
    tests/                         # json_rpc_worker.rs
packages/
  reasonix-expert-mcp/             # TypeScript MCP adapter
    package.json
    src/
      index.ts
      config.ts                    # loadServerConfig from env
      mcp/
        server.ts                  # MCP stdio server
        adapter.ts                 # Tool call orchestrator
        types.ts                   # ToolHandler, ToolResult
        tools/
          review-diff.ts           # review_diff tool handler
      runtime/
        RuntimeWorkerClient.ts     # JSON-RPC 2.0 client for Rust worker
        protocol.ts                # Frame encode/decode
        errors.ts                  # RuntimeWorkerError
      agent/
        worker-contract.ts         # Agent worker contract + conformance
        error-taxonomy.ts          # 14 error codes, 6 layers
        backend-profile.ts         # Backend profile config
        naming.ts                  # Internal naming constants
      backends/
        core/
          interfaces.ts            # AgentRunner interface
          output-normalizer.ts     # JSON extraction from stdout
        mock/
          MockRunner.ts            # Mock Reasonix runner
        reasonix/
          ReasonixRunner.ts        # Real Reasonix CLI bridge
      codex/
        setup.ts                   # Codex MCP registration
        health.ts                  # Healthcheck
docs/
  coasonix/                        # Product model + design specs
    00-collaboration-model.md
    00-executive-summary.md
    README.md
    01-architecture/
    02-runtime/
    03-reasonix/
    04-patch-and-verification/
    05-versioning/
    06-roadmap/
  implementation/                  # Execution plans
    review-diff-agent-collaboration-plan.md
    v1-mvp-execution-plan.md
    gaps-to-production.md
```

Security logic belongs in Rust. TypeScript must not grow `policy`, `state`, or
`patchSafety` authority modules.

## 7. Dependencies

Rust (from `crates/coagent-runtime-core/Cargo.toml`):

```text
serde, serde_json, rusqlite (with bundled feature), thiserror, sha2
```

TypeScript (from `packages/reasonix-expert-mcp/package.json`):

```text
@modelcontextprotocol/sdk, zod, typescript
(express, ajv, hono - available in node_modules for historical tooling)
```

## 8. Testing Strategy

Rust owns conformance tests for:

```text
schema validation (schema_registry.rs)
state transitions (state_machine.rs)
path policy and decision merge (policy_engine.rs)
SQLite persistence (sqlite_store.rs)
runtime kernel decision flow (runtime_kernel.rs)
canonical JSON (canonical_json.rs)
artifact policy (artifact_policy.rs)
JSON-RPC worker framing (json_rpc_worker.rs)
```

TypeScript owns adapter tests:

```text
MCP tools/list, tools/call request/response shaping
Rust worker client framing (RuntimeWorkerClient.test.ts)
Server config (config.test.ts)
MCP server error handling (server.test.ts)
MCP operational contract (operational-contract.test.ts)
Worker contract conformance (worker-contract.test.ts)
Error taxonomy (error-taxonomy.test.ts)
Backend profiles (backend-profile.test.ts)
Codex setup and healthcheck (setup.test.ts, health.test.ts)
```

## 9. Non-Goals

Coagent v1 does not include:

```text
local daemon
remote Runtime Service
network-exposed Runtime Kernel
shared runtime across Codex sessions
N-API / native addon integration
HTTP transport between TS and Rust
Reasonix direct write access to Codex worktree
```

## 10. Rejected Alternatives

All TypeScript: rejected because the safety kernel would live in the same
dynamic runtime as the protocol adapter.

All Rust: rejected because the MCP adapter surface is faster and lower-risk
in TypeScript, while Rust stays focused on the enforceable runtime core.
