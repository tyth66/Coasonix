# General Agent Runtime Gaps

Coagent is currently a constrained v1 gateway: one MCP tool, one primary
operation, a narrow runtime gate, and an audit-backed review workflow. That is a
good boundary for `reasonix.review_diff`, but it is not yet a mature general
agent runtime.

This document records the main gaps that must be closed before Coagent can act
as a durable runtime control plane for arbitrary agent work.

## Current Shape

The implemented system is best described as:

```text
Codex MCP Host
  -> coagent-mcp-server
      -> RuntimeKernel: state + policy + audit
      -> Backend: Mock | Reasonix ACP
```

The current runtime owns safe protocol translation, a policy gate, task state,
and SQLite audit records for `reasonix.review_diff`. Codex still owns user
intent, workspace edits, and final decisions.

## Main Deficits

### Task Model Is Too Flat

The task lifecycle is intentionally small:

```text
Created -> Running -> Completed | Failed | Cancelled
```

Coagent now records per-operation `runtime_steps` and append-only
`runtime_events` for `step_started`, `policy_evaluated`, and
`lifecycle_closed`. That is enough to start building durable execution traces,
but a general runtime still needs a richer model:

- queued, blocked, waiting-approval, retrying, and partially-completed states
- sub-tasks and dependencies
- richer per-step execution records beyond the initial runtime step table
- retry counts, owner, priority, timeout, and cancellation propagation
- a clear distinction between runtime failure and agent judgment failure

Without the remaining pieces, multi-step agent workflows are still only partly
represented by durable runtime facts.

### Tool And Capability Model Is Hard-Coded

Only `reasonix.review_diff` is registered by the default runtime today. Tool
metadata now lives in `ToolRegistry`, but the registry is still small and does
not yet cover backend binding, command execution, approval policy, or dynamic
tool loading.

A general runtime needs a tool registry with:

- tool name, version, input schema, output schema, and error schema
- declared side effects
- required capabilities for read, write, execute, network, secrets, and external
  services
- per-tool audit records
- compatibility checks for schema and tool versions

Until those remaining fields and loading paths exist, Coagent can describe more
than one tool in core policy tests, but it cannot yet safely host a growing set
of production agent capabilities.

### Policy And Approval Are Not Composable Enough

The current permission enum is a useful sketch, but the implemented path uses
only the diff-review level. A mature runtime needs policy that can be composed
from smaller permissions:

- path policy
- command policy
- network/domain policy
- secret-access policy
- artifact write policy
- human approval gates with pause/resume semantics
- approval provenance: who approved what, for which resources, and for how long

The policy should also support dry-run and explanation modes so callers can
understand why a request would be allowed or denied.

### Schema Enforcement Has Two Tracks

The repository contains a JSON Schema registry, while the MCP handler currently
uses lightweight handwritten validation for the active request and response
path. This creates a drift risk: the schema can reject payloads that the runtime
path accepts, or vice versa.

A general runtime should use one schema authority for:

- request validation
- response validation
- error payloads
- schema migration
- version negotiation
- backward compatibility policy
- duplicate-key and malformed-JSON handling

This is especially important when model output, MCP payloads, and persisted
audit payloads all need to agree on the same contract.

### Execution Isolation Is Still Shallow

The runtime authorizes paths, but it is not yet an execution sandbox. General
agent work needs stronger isolation:

- per-task worktrees or scratch directories
- command sandboxing
- environment allowlists
- working-directory controls
- CPU, wall-clock, output, and token budgets
- network restrictions
- secret redaction
- quarantine for unapproved artifacts

Path authorization is necessary, but it is not sufficient once agents can run
commands, write patches, or call external services.

### Audit Is Not Yet Full Event Sourcing

SQLite audit plus `runtime_steps` / `runtime_events` is a stronger foundation,
but it still records facts more than it drives recovery.

A mature runtime still needs an event model where every durable fact is
replayable:

- task creation
- richer step start, progress, and completion details
- tool call requests and results
- policy decisions
- approvals
- state transitions
- artifact creation and promotion
- retries and cancellations

The runtime should support idempotency keys, crash recovery, and replay/debug
views from the event log.

### Scheduling And Concurrency Are Early

Locks and cache metadata exist, but there is not yet a scheduler. General agent
runtime work needs:

- multi-task queues
- resource locks for files, directories, branches, and external systems
- stale-lock recovery
- deadlock avoidance
- priority and fairness
- shared-state conflict handling between concurrent agents

This becomes critical as soon as multiple agents or multiple tasks can operate
against the same workspace.

### Agent Identity And Provenance Are Incomplete

The documentation separates Codex, Coagent, and Reasonix conceptually. A general
runtime also needs identity as machine-enforced data:

- caller identity
- agent identity
- tool identity
- permission provenance
- artifact provenance
- approval provenance

An audit log should answer not only what happened, but which actor caused it and
under which authority.

### Observability Is Minimal

SQLite records are useful, but operational debugging needs more:

- structured logs
- tracing spans
- task timelines
- policy decision explanations
- metrics for latency, denial rate, retry rate, and tool failure rate
- inspect commands or APIs
- artifact browsing

Agent failures usually cross model output, runtime policy, tool behavior, and
external state. A mature runtime needs first-class inspection surfaces.

### Real Backend Reliability Still Needs Recovery Coverage

The mock backend is deterministic, the real Reasonix ACP integration exists, and
the backend boundary now has fake-ACP contract tests that do not require a live
model. Those tests cover handshake errors, chunk collection, invalid review
JSON, and prompt-time process EOF.

The live integration test still depends on external CLI and API credentials and
is ignored by default. The remaining backend reliability work is recovery and
compatibility depth:

- timeout and cancellation tests
- invalid-frame tests
- process crash tests
- long-lived session recovery tests
- compatibility tests for external agent protocol changes

The live integration test remains useful as an external smoke test, but it is no
longer the only evidence for the ACP boundary.

## Highest-Leverage Next Steps

The next maturity step should not be adding more tools. The runtime core should
first stabilize the abstractions that every future tool will depend on:

1. Route active MCP input and output validation through the schema registry.
2. Extend the tool registry to include backend binding, approval requirements,
   and dynamic tool loading.
3. Extend the initial step/event model so it supports recovery, replay, retries,
   and approval gates.

Once those foundations exist, adding patch generation, command execution, or
additional specialist agents becomes much less risky.
