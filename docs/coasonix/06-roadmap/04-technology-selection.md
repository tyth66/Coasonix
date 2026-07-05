# Technology Selection

This document records the implementation technology decisions for Coasonix v1.
It complements the node-level design in `03-implementation-plan.md`.

## 1. Decision

Coasonix v1 uses:

```text
Rust Runtime Core
TypeScript reasonix-expert MCP Adapter
Official MCP TypeScript SDK stable v1 line
JSON-RPC 2.0 over stdio between TypeScript and Rust
Rust edition 2024
Bun toolchain for TypeScript workspace, build, and tests
ES Modules for TypeScript package format
JSON Schema 2020-12 for review_diff test contracts
Repo-local SQLite at .agent/coasonix.sqlite
Root-level review_diff test contract fixture at schemas/coasonix-v1.schema.json
```

The architecture is:

```text
Codex MCP Host
  -> TypeScript reasonix-expert MCP Adapter
      -> managed Rust Runtime Worker
          -> Rust Runtime Core
      -> Reasonix CLI / future local controller
```

Hard rule:

```text
No side effect is allowed unless the Rust Runtime Worker returns allow.
```

## 2. Technology Baseline Matrix

| Area | v1 choice | Boundary |
|---|---|---|
| MCP adapter language | TypeScript | Owns MCP protocol and process supervision. |
| MCP SDK | Official stable MCP TypeScript SDK v1 | Do not adopt beta SDK majors as baseline. |
| Runtime core language | Rust edition 2024 | Owns enforceable schema, state, policy, audit gates. |
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
kernel/mod.rs     - RuntimeKernel: state + policy orchestration
policy/mod.rs     - PolicyEngine: operation, permission, path, argv, network checks
storage/mod.rs    - RuntimeStore: SQLite with 10 migrations, append-only audit, WAL
schema/mod.rs     - SchemaRegistry: JSON Schema validation + duplicate-key detection
state/mod.rs      - TaskState FSM: Created -> Running -> Completed/Failed
artifact/mod.rs   - ArtifactPolicy: path allowlist/denylist
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
mcp/server.ts               - MCP stdio server lifecycle
mcp/tools.ts                - review_diff core logic + input/output validation
worker/client.ts             - RuntimeWorkerClient (JSON-RPC over Bun.spawn stdio)
worker/protocol.ts           - Frame encoding/decoding
worker/errors.ts             - RuntimeWorkerError classification
reasonix/runner.ts           - ReasonixProcessRunner (spawn + timeout)
reasonix/output-normalizer.ts - JSON extraction from stdout
reasonix/mock-worker.ts      - Mock Reasonix for testing
agent/error-taxonomy.ts      - 14 error codes across 6 layers
agent/backend-profile.ts     - Backend profile configuration
config.ts                    - Environment-based server config
codex/setup.ts               - Codex MCP registration
codex/health.ts              - Healthcheck system
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
stdin:  JSON-RPC 2.0 requests
stdout: JSON-RPC 2.0 responses only
stderr: structured logs only
one line: one complete JSON-RPC frame
```

v1 worker allowed methods:

```text
runtime.initialize
runtime.evaluate_operation
runtime.write_audit
runtime.shutdown
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
Cargo.toml                    # Rust workspace (2 crates)
package.json                  # Bun workspace (1 package)
schemas/
  coasonix-v1.schema.json     # review_diff test contract fixture
crates/
  coasonix-runtime-core/      # kernel, policy, storage, schema, state, artifact, canonical
  coasonix-runtime-worker/    # JSON-RPC stdio worker (thin main.rs)
packages/
  reasonix-expert-mcp/        # TypeScript MCP adapter
    src/mcp/                  # server, adapter, tools/
    tools/
      review-diff.ts      # pluggable tool handler (strategy pattern)
    src/worker/               # client, protocol, errors
    src/reasonix/             # runner, output-normalizer, mock-worker
    src/agent/                # error-taxonomy, backend-profile, naming, worker-contract
    src/codex/                # setup, health
docs/
  coasonix/                   # Product model + design specs
  implementation/             # Execution plans
```

Security logic belongs in Rust. TypeScript must not grow `policy`, `state`, or
`patchSafety` authority modules.

## 7. Dependencies

Rust:

```text
serde, serde_json, jsonschema, rusqlite, sha2, thiserror
```

TypeScript:

```text
@modelcontextprotocol/sdk, ajv, zod, express, typescript
```

## 8. Testing Strategy

Rust owns conformance tests for:

```text
schema validation, state transitions, path policy, shell argv policy,
network policy, SQLite persistence, audit writer, runtime kernel decision merge
```

TypeScript owns adapter tests:

```text
MCP tools/list, tools/call request shaping, Rust worker client framing,
runtime_unavailable behavior, Reasonix invocation only after Rust allow,
structuredContent response mapping
```

## 9. Non-Goals

Coasonix v1 does not include:

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

