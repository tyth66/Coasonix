# Reasonix Tool Contract: `reasonix.review_diff`

This document is the **canonical product target** for the first Reasonix tool.
All other documents that describe the review_diff result shape should reference
this one.

```text
Tool: reasonix.review_diff
Purpose: ask Reasonix to review a prepared diff
Rule: Reasonix returns review information only
```

## Responsibility Split

```text
Codex   -> decides to request a diff review
Coagent -> validates/gates/translates the request
Reasonix -> performs the review task
Coagent -> wraps the review result for MCP
Codex   -> makes the final decision
```

## Coagent-Owned Input Envelope

The MCP tool input may include Coagent-owned fields such as:

```text
goal
repo.root
repo.base_branch
repo.working_branch
artifacts.diff_path
artifacts.context_path
artifacts.test_log_path
focus
constraints
budget
permission_level (always L1_DIFF_REVIEW)
```

These fields are how Codex asks Coagent to delegate a safe task. They are not
fields Reasonix must echo back.

## Reasonix Task Input

Before calling Reasonix, Coagent translates the MCP arguments into a task
that is natural for a reviewing agent:

```text
Review this diff for correctness, regression risk, missing tests, and protocol
or safety issues. Use the provided context and return only review information.
```

The task may reference artifact paths or inline diff content depending on the
backend bridge, but the semantic task is the same.

## Reasonix Task Output (Target)

The target Reasonix output contains **only review information**:

```json
{
  "verdict": "pass | needs_fix | risky | unknown | not_applicable",
  "summary": "Short review conclusion.",
  "findings": [
    {
      "severity": "blocker | major | minor | note",
      "category": "correctness | test | security | protocol | maintainability | other",
      "file": "relative/path.ext",
      "line": 1,
      "issue": "What is wrong or risky.",
      "evidence": "Why the issue follows from the diff.",
      "recommendation": "What Codex should consider changing.",
      "confidence": 0.0
    }
  ],
  "tests_to_run": [],
  "risks": [],
  "assumptions": [],
  "confidence": 0.0
}
```

Allowed verdicts: `pass`, `needs_fix`, `risky`, `unknown`, `not_applicable`.

## Fields Reasonix Must NOT Return

These are Coagent internals. They must not appear in the Reasonix review payload:

```text
schema_version
task_id
request_id
status
runtime_decision
worker_status
backend_profile
audit_event_id
stderr
```

## Coagent Wrapping

Coagent may wrap Reasonix pure review data into MCP structures:

```text
content[]              -> MCP text content
structuredContent      -> the validated review result object
_meta                  -> diagnostics, error codes, layer info
isError                -> true for errors, false for success
```

Coagent may use internal ids and diagnostics to protect the call path. Those
fields belong to wrapper metadata and audit records, not to Reasonix review.

## Transitional Implementation State (as of 2026-07-06)

The current code path is operational but transitional:

**What is implemented:**
- `mcp/tools/review-diff.ts`: ToolHandler with full input/output validation
- `mcp/adapter.ts`: Complete call flow — normalize -> runtime gate -> agent -> validate -> wrap
- `schemas/coagent-v1.schema.json`: review_diff_input_v1 + review_result_v1 contracts
- `agent/worker-contract.ts`: Agent worker contract with reviewResultSchemaError() validation

**What is transitional:**
- The mock backend (`MockRunner`) emits a hardcoded `review_result_v1` JSON
  with envelope fields: `schema_version`, `task_id`, `request_id`, `status`
- The adapter checks `task_id`/`request_id` identity match between request
  and Reasonix output; this identity tracking currently relies on these
  envelope fields being present in the result
- The schema fixture still requires `schema_version`, `task_id`, `request_id`,
  `status` in `review_result_v1`

**Target state (Task 2 of active plan):**
- Move `task_id`/`request_id` tracking to Coagent-internal wrapper state
- Remove `schema_version`, `task_id`, `request_id`, `status` from Reasonix
  result contract
- Reasonix mock emits only review data (verdict, summary, findings, etc.)

The active migration plan is:

```text
../../implementation/review-diff-agent-collaboration-plan.md
```

## Acceptance Criteria for the Next Pass

```text
1. Reasonix mock emits only review data (no envelope fields).
2. Adapter wraps review data into MCP result without requiring Reasonix to echo ids.
3. Runtime gate still happens before Reasonix invocation.
4. Worker diagnostics never become review content.
5. Tests prove malformed review data is rejected.
6. Docs and schema fixture match the pure review result.
```
