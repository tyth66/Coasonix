# Communication and MCP Boundary

MCP is the Codex-facing control protocol. Reasonix is not exposed directly to
Codex. Coasonix owns the boundary.

## Layers

```text
MCP control plane:  Codex -> Coasonix tool call
Runtime gate:       Coasonix -> Rust Runtime allow/deny
Expert task plane:  Coasonix -> Reasonix delegated task
MCP result plane:   Coasonix -> Codex structured result
```

## MCP Tool

Current v1 exposes only:

```text
reasonix.review_diff
```

`tools/list` must expose an inline MCP `inputSchema`. That schema is for Codex to
know how to call the tool. It is not a Reasonix result contract.

## Request Flow

```text
Codex tools/call reasonix.review_diff
-> Coasonix normalizes arguments
-> Coasonix allocates internal request identity as needed
-> Coasonix asks Rust Runtime to evaluate the operation
-> Rust Runtime returns allow/deny
-> Coasonix invokes Reasonix only on allow
-> Reasonix returns review information only
-> Coasonix wraps review data into MCP tool result
-> Codex decides whether to use the review
```

## What Codex Sees

Codex should see the review result and, when something fails, a Coasonix error
with a clear layer/code. Codex should not need to interpret backend process logs
or runtime audit internals as if they were Reasonix answer.

The canonical review_diff result contract is defined in
[../03-reasonix/01-tool-contracts-and-wrapper.md](../03-reasonix/01-tool-contracts-and-wrapper.md).

Coasonix may include internal diagnostics in MCP `_meta` for failures, for example:

```json
{
  "code": "runtime_policy_denied",
  "layer": "runtime"
}
```

Those fields are Coasonix metadata, not Reasonix review content.

## What Reasonix Should Return

Reasonix should return only the expert task result. For `review_diff`, it should
not return:

```text
schema_version
task_id
request_id
runtime decision
worker status
backend profile
stderr diagnostics
MCP protocol fields
audit ids
```

If Coasonix needs those fields for routing or verification, Coasonix owns them
internally.

## Failure Flow

```text
invalid MCP arguments   -> Coasonix rejects before Runtime or Reasonix
runtime deny            -> Coasonix does not invoke Reasonix
Reasonix timeout/nonzero/malformed output -> Coasonix returns an MCP error
Reasonix review contract invalid          -> Coasonix returns an MCP error
```

In all failure cases, Codex receives an error classification. It does not receive
worker stdout/stderr as trusted review content.

## Current Implementation Note

The live implementation still uses a transitional backend payload that includes
system-envelope fields in the review result. The active plan to move these to
Coasonix wrapper metadata is in
[../../implementation/review-diff-agent-collaboration-plan.md](../../implementation/review-diff-agent-collaboration-plan.md).
