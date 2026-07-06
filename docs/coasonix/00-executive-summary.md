# Executive Summary
The current implemented state and forward plan.

```text
Codex   = assigns tasks, owns workspace execution, and makes final decisions
Coagent = safely translates protocol, gates side effects, and records audit evidence
Reasonix = performs the delegated expert task
Codex   = consumes the result and decides what to do next
```

## Current Product Boundary

One tool only:

```text
reasonix.review_diff
```

The intended behavior is simple:

```text
Codex asks for a diff review
Coagent checks whether the call is safe and well-formed
Reasonix reviews the diff and returns only review information
Codex decides whether to act on that review
```

Reasonix must not return Coagent runtime state, schema validation payloads,
worker diagnostics, backend profile data, task routing metadata, or MCP
transport details. Those are Coagent internals.

## Architecture

```text
Codex MCP Host
  -> TypeScript reasonix-expert MCP Adapter (packages/reasonix-expert-mcp)
      -> managed Rust Runtime Worker (crates/coagent-runtime-worker)
          -> Rust Runtime Core (crates/coagent-runtime-core)
      -> Reasonix CLI / mock worker
```

Two crates (Rust) + one package (TypeScript/Bun). The adapter calls the Rust
Runtime Worker over JSON-RPC 2.0 stdio before delegating to Reasonix.
SQLite stores append-only audit records under `.agent/coagent.sqlite`.

## Implementation Status

### Completed

```text
MCP registration/setup                   (codex/setup.ts, codex/health.ts)
MCP stdio server                         (mcp/server.ts)
inline tools/list inputSchema            (mcp/tools/review-diff.ts)
Pluggable tool handler architecture      (strategy pattern, mcp/adapter.ts)
Multi-operation PolicyEngine registry    (policy/mod.rs)
Rust pre-Reasonix runtime gate           (crates/coagent-runtime-core)
  - State engine (Created -> Running -> Completed/Failed)
  - Policy engine (operation, permission, path, argv, network)
  - Artifact policy (path allowlist/denylist with glob matching)
  - SQLite append-only audit (10 tables, WAL, FK, triggers reject UPDATE/DELETE)
  - JSON Schema validation + duplicate-key detection (schema/mod.rs)
  - Canonical JSON/path normalization (canonical/mod.rs)
Rust JSON-RPC stdio Runtime Worker       (crates/coagent-runtime-worker, 4 methods)
TypeScript Runtime Worker client         (runtime/RuntimeWorkerClient.ts)
mock review_diff vertical slice          (621-byte echo worker via MockRunner)
healthcheck / conformance / error taxonomy (14 codes across 6 layers)
backend profiles                         (mock, reasonix)
```

### Active Transition

The current `review_result_v1` contract still includes system envelope fields
(`schema_version`, `task_id`, `request_id`, `status`) that belong in Coagent
wrapper metadata, not in Reasonix review answer. The active plan moves
these fields out of the Reasonix result payload.

### Out of Scope

```text
additional tools beyond review_diff
patch application / write autonomy
human approval UI
remote transport / HTTP / daemon
real (non-mock) Reasonix backend bridge
context projection
cache reuse (SQLite cache_entries table exists but reuse_enabled always 0)
performance/security/architecture review tools
```

## Forward Plan

```text
../implementation/review-diff-agent-collaboration-plan.md
```

Historical implementation evidence is archived under that same directory.

## Verification

```powershell
cargo test --workspace
bun test
python -m json.tool schemas/coagent-v1.schema.json > $null
cargo fmt --all -- --check
git diff --check
```
