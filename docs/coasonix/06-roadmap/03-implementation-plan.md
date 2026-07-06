# Implementation Plan and Critical Node Definitions (Design Reference)

This document defines the 16 critical nodes that must be stable, auditable, and
verifiable in any Coagent implementation. These nodes are design boundaries,
not implementation status claims.

**For current implementation status, see:**
- [../00-executive-summary.md](../00-executive-summary.md)
- [../../implementation/review-diff-agent-collaboration-plan.md](../../implementation/review-diff-agent-collaboration-plan.md)

**Nodes 1-8, 12 are implemented in the v1 Rust Runtime Core + TS MCP Adapter.**
**Nodes 9-11, 13-16 are post-v1 design specifications.**

---

## 1. Node Implementation Status Summary

| ID | Node | Status | Implementation |
|---|---|---|---|
| N01 | Task Intake and Task State | Implemented | `state/mod.rs` (TaskState, TaskStateValue), `adapter.ts` (task_id generation) |
| N02 | Codex Primary Control | Implemented | Architectural invariant: Codex owns final decision; adapter gates Reasonix |
| N03 | Context Projector | Post-v1 design | `01-architecture/03-context-architecture.md`; no code |
| N04 | MCP Session and Transport | Implemented | `mcp/server.ts` (JSON-RPC stdio), `codex/setup.ts` (registration) |
| N05 | Tool and Schema Registry | Implemented | `mcp/tools/review-diff.ts` (ToolHandler), `schemas/coagent-v1.schema.json` |
| N06 | Wrapper Input Gate | Implemented | `adapter.ts` (normalizeInput, buildRuntimeRequest, runtime.evaluate_operation) |
| N07 | Reasonix Execution Gate | Implemented | `adapter.ts` (only invokes agent on allow), `MockRunner`/`ReasonixRunner` |
| N08 | Output Normalization Gate | Implemented | `adapter.ts` (JSON parse, identity check, schema validation, structuredContent) |
| N09 | Codex Decision Gate | Post-v1 design | Not yet implemented as structured gate |
| N10 | Patch Safety Checker | Post-v1 design | `04-patch-and-verification/` design docs; no code |
| N11 | Verification Gate | Post-v1 design | `04-patch-and-verification/03-verification-gate.md`; no code |
| N12 | Audit Event Model | Implemented | `storage/mod.rs` (10 tables, append-only, WAL, FK, triggers) |
| N13 | Loop and Budget Limiter | Post-v1 design | Design defined in policy-engine; no runtime enforcement |
| N14 | Human Approval Gate | Post-v1 design | `04-patch-and-verification/04-human-approval-gate.md`; no code |
| N15 | Performance Review Gate | Post-v1 design | Tool definition only; no implementation |
| N16 | Reasonix Project Controller and Session Router | Post-v1 design | `01-architecture/04-project-session-tool-mapping.md`; no code |

## 2. Global Invariants

The following invariants cross all nodes:

```text
1. Codex owns final decision.
2. Reasonix output is advisory, never authoritative.
3. Wrapper is the only protocol and security boundary between Codex and Reasonix.
4. MCP is control plane only; repository facts move through explicit artifacts.
5. Every Reasonix result must be schema-valid before Codex sees it as structuredContent.
6. Every patch proposal must remain a proposal until Codex validates and applies it.
7. Every accepted recommendation must have verification evidence or an explicit verification gap.
8. Every high-risk branch must stop at Human Approval Gate.
9. Every important transition must be audit-logged with task_id and request_id.
10. Hidden memory is forbidden; continuity is explicit task state.
11. Same project boundary routes to one Reasonix Project Controller, not one isolated project per Codex session.
12. Session lanes are cache boundaries, not memory boundaries.
13. Different project boundaries never share Reasonix sessions, task state, artifacts, result cache, patch proposals, context projections, audit namespace, or permission profile.
14. Codex calls Reasonix capabilities, not Reasonix internal agents.
15. Reasonix memory may generate hypotheses, but never verification evidence.
```

## 3. N01 Task Intake and Task State [Implemented]

### 3.1 Implementation

Task identity is generated in `adapter.ts` by the `reviewDiffHandler.normalizeInput()`:

```typescript
nextTaskId: () => `TASK-${request.name.replace(/\./g, "-")}-${nextTaskNumber++}`
nextRequestId: () => `REQ-${request.name.replace(/\./g, "-")}-${nextRequestNumber++}`
```

Task state is managed in `state/mod.rs`:

```rust
TaskStateValue: Created -> Running -> Completed/Failed
```

`RuntimeKernel.evaluate_state()` checks terminal state before allowing operations.
`RuntimeKernel.persist_running_state()` advances Created -> Running.

### 3.2 Hard Requirements (Implemented)

```text
1. task_id is unique within the adapter instance lifetime. [YES]
2. task_id appears in audit events and runtime decisions. [YES]
3. Task state is explicit; no hidden session memory. [YES - SQLite backed]
4. Completion requires verification evidence. [POST-V1]
```

## 4. N02 Codex Primary Control [Implemented as Architectural Invariant]

Codex owns: user intent, planning, workspace changes, verification, final decision.

Coagent owns: MCP tool surface, runtime gate, audit, protocol conversion.

Reasonix owns: the delegated expert task output only.

This is enforced by:
- `adapter.ts`: Codex calls `tools/call`; adapter gates execution
- `kernel/mod.rs`: Rust RuntimeKernel decides allow/deny before Reasonix invocation
- The adapter validates Reasonix output before returning to Codex

## 5. N03 Context Projector [Post-v1 Design]

See `01-architecture/03-context-architecture.md` and `03-reasonix/04-context-projection-threat-model.md`.
No implementation in v1. MCP tool arguments pass directly to Reasonix.

## 6. N04 MCP Session and Transport [Implemented]

Implementation:
- `mcp/server.ts`: JSON-RPC 2.0 over stdio (initialize, tools/list, tools/call)
- `codex/setup.ts`: Codex MCP registration
- `codex/health.ts`: Healthcheck system
- Transport: stdin/stdout newline-delimited JSON-RPC frames

Lifecycle: `initialize -> notifications/initialized -> tools/list -> tools/call -> shutdown`

## 7. N05 Tool and Schema Registry [Implemented]

Active tool: `reasonix.review_diff`

Implementation:
- `mcp/tools/review-diff.ts`: ToolHandler with inputSchema, description, normalizeInput, buildRuntimeRequest, invokeAgent, validateOutput
- `mcp/adapter.ts`: ToolRegistry as Map<string, ToolHandler>
- `schemas/coagent-v1.schema.json`: review_diff_input_v1 and review_result_v1 contracts

Post-v1 documented tool contracts (no implementation): security_audit, debug_hypothesis, architecture_options, performance_review, propose_patch, test_plan.

## 8. N06 Wrapper Input Gate [Implemented]

Implementation in `adapter.ts` `callTool()`:

```text
1. Check initialized state
2. Lookup handler in toolRegistry
3. handler.normalizeInput(arguments) -> generate task_id/request_id, validate schema
4. handler.buildRuntimeRequest(input) -> construct evaluate_operation params
5. runtime.call("runtime.evaluate_operation", params) -> Rust RuntimeKernel
6. Check decision.decision === "allow"
7. Only proceed to Reasonix on allow
```

## 9. N07 Reasonix Execution Gate [Implemented]

Implementation: `adapter.ts` only calls `handler.invokeAgent(agent, input)` when
`decision.decision === "allow"`.

Backend abstraction: `AgentRunner` interface with `MockRunner` and `ReasonixRunner` implementations.

## 10. N08 Output Normalization Gate [Implemented]

Implementation in `adapter.ts` after agent invocation:

```text
1. Check run.timedOut -> WORKER_TIMEOUT error
2. Check run.exitCode !== 0 -> WORKER_NONZERO_EXIT error
3. extractSingleJsonObject(run.stdout) -> parse JSON
4. Check task_id/request_id match -> WORKER_IDENTITY_MISMATCH
5. handler.validateOutput(parsed) -> contract check
6. Return structuredContent with parsed.value + _meta diagnostics
```

## 11. N09-N11, N13-N16 [Post-v1 Design]

These nodes are documented in:
- `04-patch-and-verification/` (N10 Patch Safety, N11 Verification, N14 Human Approval)
- `02-runtime/02-runtime-enforcement-layer.md` §13 (N13 Loop/Budget Limiter)
- `03-reasonix/` (N15 Performance Review Gate)
- `01-architecture/04-project-session-tool-mapping.md` (N16 Project/Session Router)

No implementation exists for any of these nodes in v1.

## 12. N12 Audit Event Model [Implemented]

Implementation: `storage/mod.rs`

```text
10 SQLite tables via sequential migrations
audit_events: append-only (BEFORE UPDATE/DELETE triggers raise ABORT)
runtime_decisions: FK to audit_events.id
WAL journal mode, FULL synchronous, FK enforcement
Per-task monotonic task_sequence
Global monotonic audit id (SQLite rowid)
```

Every `RuntimeKernel.evaluate_operation()` call persists:
1. An audit_events row with event_type = `runtime_decision_{allow|deny|fatal_error|...}`
2. A runtime_decisions row with FK to the audit event
