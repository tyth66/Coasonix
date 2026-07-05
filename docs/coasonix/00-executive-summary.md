# Executive Summary
The current implemented state and forward plan.

```text
Codex   = assigns tasks, owns workspace execution, and makes final decisions
Coasonix = safely translates protocol, gates side effects, and records audit evidence
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
Coasonix checks whether the call is safe and well-formed
Reasonix reviews the diff and returns only review information
Codex decides whether to act on that review
```

Reasonix must not return Coasonix runtime state, schema validation payloads,
worker diagnostics, backend profile data, task routing metadata, or MCP
transport details. Those are Coasonix internals.

## Implementation Status

### Completed

```text
MCP registration/setup                   (codex/setup.ts, codex/health.ts)
MCP stdio server                         (mcp/server.ts)
inline tools/list inputSchema           (mcp/tools.ts)
Rust pre-Reasonix runtime gate           (crates/coasonix-runtime-core)
  - State engine (Created to Running to Completed/Failed)
  - Policy engine (operation, permission, path, argv, network)
  - SQLite append-only audit (10 tables, WAL, FK)
  - JSON Schema validation + duplicate-key detection (schema/mod.rs)
Rust JSON-RPC stdio Runtime Worker       (crates/coasonix-runtime-worker)
TypeScript Runtime Worker client         (worker/client.ts)
mock review_diff vertical slice          (reasonix/mock-worker.ts)
healthcheck / conformance / error taxonomy / backend profiles
```

### Active Transition

The current review_result_v1 contract still includes system envelope fields
(schema_version, task_id, request_id, status) that belong in Coasonix
wrapper metadata, not in Reasonix review answer. The active plan moves
these fields out of the Reasonix result payload.

### Out of Scope

```text
additional tools beyond review_diff
patch application / write autonomy
human approval UI
remote transport / HTTP / daemon
real (non-mock) backend bridge
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
python -m json.tool schemas/coasonix-v1.schema.json > $null
cargo fmt --all -- --check
git diff --check
```
