# Roadmap: from MVP to real agent-to-agent delegation

This document records what stands between the current v1 MVP and a fully
operational system where Codex delegates tasks and Reasonix executes them.

Last updated: 2026-07-06.

---

## What is Done

```text
MCP server stdio lifecycle                 implemented (mcp/server.ts)
reasonix.review_diff tool registration     implemented (mcp/tools/review-diff.ts)
Pluggable tool handler architecture        implemented (strategy pattern, mcp/adapter.ts)
Multi-operation PolicyEngine registry      implemented (policy/mod.rs)
Rust pre-Reasonix runtime gate             implemented (kernel/mod.rs)
  - State engine (Created->Running->Completed/Failed)
  - Policy engine (operation, permission, path, argv, network)
  - Artifact policy (path allowlist/denylist, glob matching)
  - SQLite append-only audit (10 tables, WAL, FK, no-UPDATE/DELETE triggers)
  - JSON Schema validation + duplicate-key detection
  - Canonical JSON/path normalization
Rust JSON-RPC stdio Runtime Worker         implemented (coagent-runtime-worker, 4 methods)
TypeScript Runtime Worker client           implemented (RuntimeWorkerClient.ts)
mock Reasonix vertical slice               implemented (MockRunner, 621-byte echo worker)
healthcheck / conformance / error taxonomy implemented (14 codes, 6 layers)
backend profiles                           implemented (mock, reasonix)
docs: implementation status annotated      implemented
```

---

## Gap 1: Reasonix Does Not Exist (P0)

**Current state**: `backends/mock/MockRunner.ts` spawns a hardcoded echo worker
that returns a fixed `review_result_v1` JSON. It does not read diffs, analyze
code, or produce real findings. `backends/reasonix/ReasonixRunner.ts` exists
as a process-spawn bridge but targets a binary that does not yet exist.

**What is needed**:
- A real Reasonix CLI or HTTP service that accepts a review task as JSON
  on stdin, reads the referenced diff file, performs actual code review,
  and returns structured findings on stdout.

---

## Gap 2: Review Result Contract Still Carries Envelope Fields (P0)

**Current state**: The `review_result_v1` contract and mock worker output include
`schema_version`, `task_id`, `request_id`, `status` — fields that belong to
Coagent wrapper metadata.

**What is needed**: Remove these envelope fields from the Reasonix result
contract. Track `task_id`/`request_id` internally in the adapter. Reasonix
output should be pure review data: `verdict`, `summary`, `findings[]`,
`tests_to_run[]`, `risks[]`, `assumptions[]`, `confidence`.

Tracked in: `docs/implementation/review-diff-agent-collaboration-plan.md` Task 2.

---

## Gap 3: No Context Projection (P1)

**Current state**: MCP tool arguments pass directly to Reasonix as task input.
There is no redaction, no compression, no secret filtering, no projection
hashing.

**What is needed**: Context Projector that transforms Codex global context
into minimal, security-filtered Reasonix input. See
`docs/coasonix/01-architecture/03-context-architecture.md` and
`docs/coasonix/03-reasonix/04-context-projection-threat-model.md`.

---

## Gap 4: No Real CI / Test Integration (P1)

**Current state**: The system operates on static diff files pushed to
`.agent/diffs/`. There is no integration with CI pipelines, no automatic
diff capture, no test log ingestion.

---

## Gap 5: No Patch Generation or Application (P1)

**Current state**: Only `reasonix.review_diff` is exposed. There is no
patch proposal, safety checking, dry-run, apply, or rollback. Design specs
exist in `docs/coasonix/04-patch-and-verification/`. Code does not exist.

---

## Gap 6: No Cache Reuse (P2)

**Current state**: The `cache_entries` SQLite table exists with schema for
cache metadata, but `reuse_enabled` is always 0 and no cache-hit path is
implemented. Design spec exists in
`docs/coasonix/03-reasonix/03-cache-engineering-model.md`.

---

## Gap 7: No Observability Beyond SQLite Audit (P2)

**Current state**: The only observability mechanism is the append-only
`audit_events` SQLite table. No metrics counters, tracing spans, debug
hooks, or SLO thresholds exist. Design spec in
`docs/coasonix/02-runtime/05-observability-contract.md`.

---

## Gap 8: No Human Approval Gate (P2)

**Current state**: No approval flow. All operations are either auto-allowed
by the runtime gate or auto-denied. Design spec in
`docs/coasonix/04-patch-and-verification/04-human-approval-gate.md`.

---

## Gap 9: No Verification Gate (P1)

**Current state**: No structured verification of claims. Codex may run its
own tests after receiving a review, but there is no Coagent-side verification
gate that blocks completion based on unresolved verification gaps.
Design spec in `docs/coasonix/04-patch-and-verification/03-verification-gate.md`.

---

## Gap 10: No Multi-Project / Session Routing (P2)

**Current state**: Each `tools/call` is independent. There is no Project
Controller, no session lane routing, no session reuse. Design spec in
`docs/coasonix/01-architecture/04-project-session-tool-mapping.md`.

---

## Gap 11: Bun Test Failures (P2)

**Current state**: `bun test` has 13 failures, all due to missing local
worker binaries or Codex registration paths (not logic errors). These
should be fixed or documented as environment-dependent.

---

## Summary

```text
P0 (blocks real use):           Gap 1 (real Reasonix), Gap 2 (pure result contract)
P1 (blocks production):         Gap 3 (context projection), Gap 4 (CI integration),
                                 Gap 5 (patch), Gap 9 (verification gate)
P2 (quality/scale):             Gap 6 (cache), Gap 7 (observability), Gap 8 (approval),
                                 Gap 10 (routing), Gap 11 (test env)
```
