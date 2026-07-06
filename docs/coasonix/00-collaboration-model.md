# Coagent Collaboration Model

This is the canonical product model for Coagent.

```text
Codex   = assigns work, owns execution context, and makes the final decision
Coagent = provides safe protocol translation, runtime gating, and audit
Reasonix = performs the delegated expert task and returns only the task result
Codex   = evaluates the result and decides what to do next
```

Coagent is not trying to turn Reasonix into a generic CLI utility. Codex and
Reasonix are both agent systems. Coagent exists so Codex can delegate a bounded
expert task to Reasonix without giving Reasonix control of Codex''s workspace,
policy, terminal, or final decision.

## Architecture (Implemented)

```text
Codex MCP Host
  -> TypeScript reasonix-expert MCP Adapter (packages/reasonix-expert-mcp)
      -> managed Rust Runtime Worker (crates/coagent-runtime-worker)
          -> Rust Runtime Core (crates/coagent-runtime-core)
      -> Reasonix CLI / mock worker
```

The TypeScript adapter handles MCP protocol. Before delegating to Reasonix,
the adapter calls the Rust Runtime Worker over JSON-RPC 2.0 stdio. The
Runtime Core evaluates state and policy gates. Only on `allow` does the
adapter invoke Reasonix. SQLite stores append-only audit records.

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

### Coagent

Coagent owns:

```text
MCP tool surface (reasonix.review_diff)
request normalization (adapter.ts normalizeInput)
runtime allow/deny gate (Rust RuntimeKernel.evaluate_operation)
  - state machine check (Created->Running->Completed/Failed)
  - policy engine check (operation, permission, path, argv, network)
path / argv / network policy (ArtifactPolicy + PolicyEngine)
JSON-RPC 2.0 stdio worker communication (RuntimeWorkerClient)
audit records (SQLite append-only, 10 tables, WAL, FK)
protocol conversion between Codex and Reasonix
error classification (14 codes across 6 layers)
```

Coagent must keep internal protocol, runtime, audit, and backend diagnostics out
of Reasonix''s task result. Those details may appear in logs or MCP error metadata,
but they are not Reasonix''s answer to Codex.

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
-> Coagent validates MCP arguments and prepares the Reasonix task
-> Coagent asks Rust Runtime whether the call is allowed (evaluate_operation)
-> Rust Runtime checks task state, paths, argv, network policy, and auditability
-> Coagent delegates the review task to Reasonix only when Rust returns allow
-> Reasonix returns review information only
-> Coagent validates the output (JSON parse, identity check, schema validation)
-> Coagent wraps that review into an MCP tool result (structuredContent)
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

Coagent may internally attach `task_id`, `request_id`, backend exit status,
error codes, audit ids, and protocol metadata, but those are wrapper metadata,
not Reasonix''s review result.

## Transitional Implementation State

The current code path is operational but transitional:

- The mock backend (`MockRunner`) emits a hardcoded `review_result_v1` JSON
  that includes system envelope fields (`schema_version`, `task_id`, `request_id`,
  `status`).
- The adapter (`adapter.ts`) checks `task_id`/`request_id` identity match
  between request and Reasonix output; these checks will move to Coagent-internal
  tracking after the envelope fields are removed from the Reasonix contract.
- The `schemas/coagent-v1.schema.json` fixture defines both `review_diff_input_v1`
  and `review_result_v1` with envelope fields still present.
- The active plan (`docs/implementation/review-diff-agent-collaboration-plan.md`)
  targets Task 2 to remove these fields from the Reasonix result.

## Non-Goals for the Current Slice

Do not expand beyond `reasonix.review_diff` until this result boundary is clean.
Post-v1 surfaces such as patch application, human approval, remote transports,
network exceptions, and additional Reasonix tools remain out of scope.

## Verification

```powershell
cargo test --workspace        # Rust Runtime Core + Worker (all pass)
bun test                      # TypeScript adapter (70 pass, 13 fail: env/missing binaries)
python -m json.tool schemas/coagent-v1.schema.json > $null
cargo fmt --all -- --check
```
