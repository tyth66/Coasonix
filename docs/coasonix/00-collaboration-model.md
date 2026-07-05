# Coasonix Collaboration Model

This is the canonical product model for Coasonix.

```text
Codex   = assigns work, owns execution context, and makes the final decision
Coasonix = provides safe protocol translation, runtime gating, and audit
Reasonix = performs the delegated expert task and returns only the task result
Codex   = evaluates the result and decides what to do next
```

Coasonix is not trying to turn Reasonix into a generic CLI utility. Codex and
Reasonix are both agent systems. Coasonix exists so Codex can delegate a bounded
expert task to Reasonix without giving Reasonix control of Codex's workspace,
policy, terminal, or final decision.

## Role Boundaries

### Codex

Codex owns:

```text
user intent
planning
workspace changes
verification
final decision
final user response
```

Codex may call Reasonix when an expert review or reasoning task would improve
quality, but Codex must not treat Reasonix output as an instruction stream.
Reasonix output is advice/evidence for Codex to evaluate.

### Coasonix

Coasonix owns:

```text
MCP tool surface
request normalization
runtime allow/deny gate
path / argv / network policy
audit records
protocol conversion between Codex and Reasonix
error classification
```

Coasonix must keep internal protocol, runtime, audit, and backend diagnostics out
of Reasonix's task result. Those details may appear in logs or MCP error metadata,
but they are not Reasonix's answer to Codex.

### Reasonix

Reasonix owns:

```text
the delegated expert task
review reasoning
findings
recommendations
confidence in its review
```

Reasonix must return only the task result. For `reasonix.review_diff`, that means
review information only: verdict, summary, findings, suggested tests, risks,
assumptions, and confidence. It must not return runtime decisions, worker status,
backend diagnostics, schema validation payloads, task routing metadata, or MCP
transport details.

## `reasonix.review_diff` Target Chain

```text
Codex calls reasonix.review_diff
-> Coasonix validates MCP arguments and prepares the Reasonix task
-> Coasonix asks Rust Runtime whether the call is allowed
-> Rust Runtime checks task state, paths, argv, network policy, and auditability
-> Coasonix delegates the review task to Reasonix only when Rust returns allow
-> Reasonix returns review information only
-> Coasonix wraps that review into an MCP tool result
-> Codex decides whether and how to use the review
```

## Result Boundary

Reasonix target result:

```json
{
  "verdict": "needs_fix",
  "summary": "The diff introduces one correctness risk.",
  "findings": [
    {
      "severity": "major",
      "category": "correctness",
      "file": "src/example.ts",
      "line": 42,
      "issue": "The new branch can skip validation.",
      "evidence": "The early return happens before the input guard.",
      "recommendation": "Move the guard before the early return.",
      "confidence": 0.86
    }
  ],
  "tests_to_run": ["bun test src/example.test.ts"],
  "risks": [],
  "assumptions": [],
  "confidence": 0.86
}
```

Coasonix may internally attach `task_id`, `request_id`, backend exit status,
error codes, audit ids, and protocol metadata, but those are wrapper metadata,
not Reasonix's review result.

## Non-Goals for the Current Slice

Do not expand beyond `reasonix.review_diff` until this result boundary is clean.
Post-v1 surfaces such as patch application, human approval, remote transports,
network exceptions, and additional Reasonix tools remain out of scope.
