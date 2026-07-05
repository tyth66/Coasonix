# Coasonix Architecture: Roles and Boundaries

Coasonix coordinates two agent systems:

```text
Codex   = primary controller and final decision maker
Reasonix = delegated expert agent system
Coasonix = safe collaboration boundary between them
```

The architecture is not Codex and Reasonix chatting as peers. Codex assigns a
bounded task. Reasonix completes that task. Coasonix controls the protocol,
runtime gate, and audit boundary.

## Core Chain

```text
User asks Codex
-> Codex decides whether Reasonix review is useful
-> Codex calls reasonix.review_diff through MCP
-> Coasonix validates and gates the request
-> Reasonix performs the diff review
-> Coasonix wraps the review result for MCP
-> Codex evaluates the review and decides the next step
```

## Responsibilities

### Codex

Codex owns:

```text
user intent
planning
workspace edits
command execution
test execution
final decision
final user response
```

Codex may use Reasonix as an expert reviewer. Codex must not let Reasonix own
workspace mutation, policy decisions, or final completion claims.

### Coasonix

Coasonix owns:

```text
MCP tool definition
request normalization
runtime allow/deny decision path
path / argv / network policy
audit
backend process isolation
error taxonomy
MCP result wrapping
```

Coasonix may maintain internal ids, audit records, backend diagnostics, and
runtime decisions. Those details are not Reasonix answer.

### Reasonix

Reasonix owns the delegated expert task only. For `reasonix.review_diff`, that
means:

```text
review the diff
identify findings
summarize risk
recommend tests or fixes
state confidence
```

Reasonix must not return Coasonix runtime status, backend logs, schema validation
payloads, MCP metadata, or routing ids as part of the review result.

## Current v1 Scope

Only this tool is in scope:

```text
reasonix.review_diff
```

Out of scope until this tool is clean:

```text
reasonix.propose_patch
patch apply
approval UI
remote transport
network exceptions
additional Reasonix tools
```

## Target review_diff Result

The canonical review_diff contract is in
[03-reasonix/01-tool-contracts-and-wrapper.md](../03-reasonix/01-tool-contracts-and-wrapper.md).

Reasonix target output is review data only. Coasonix wraps this in MCP
`structuredContent` and attaches internal metadata in MCP `_meta` or audit
records. The review payload itself stays pure.

## Implementation Status

The Rust runtime gate (state + policy engines, SQLite audit) and TypeScript
MCP adapter are implemented with a mock review_diff vertical slice. The
current review_result_v1 contract still carries transitional system-envelope
fields. The active plan to move those to Coasonix wrapper metadata is in
[../../implementation/review-diff-agent-collaboration-plan.md](../../implementation/review-diff-agent-collaboration-plan.md).
