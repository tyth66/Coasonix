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
Coasonix -> validates/gates/translates the request
Reasonix -> performs the review task
Coasonix -> wraps the review result for MCP
Codex   -> makes the final decision
```

## Coasonix-Owned Input Envelope

The MCP tool input may include Coasonix-owned fields such as:

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
permission_level
```

These fields are how Codex asks Coasonix to delegate a safe task. They are not
fields Reasonix must echo back.

## Reasonix Task Input

Before calling Reasonix, Coasonix should translate the MCP arguments into a task
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

These are Coasonix internals. They must not appear in the Reasonix review payload:

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

## Coasonix Wrapping

Coasonix may wrap Reasonix pure review data into MCP structures:

```text
content[]
structuredContent
_meta
isError
```

Coasonix may use internal ids and diagnostics to protect the call path. Those
fields belong to wrapper metadata and audit records, not to Reasonix review.

## Transitional Implementation State

The current code still uses a `review_result_v1` payload that includes
system-envelope fields (`schema_version`, `task_id`, `request_id`, `status`)
in the mock and test fixtures. This is a known transitional state.

The active migration plan is:

```text
../../implementation/review-diff-agent-collaboration-plan.md
```

## Acceptance Criteria for the Next Pass

```text
1. Reasonix mock emits only review data.
2. Adapter wraps review data into MCP result without requiring Reasonix to echo ids.
3. Runtime gate still happens before Reasonix invocation.
4. Worker diagnostics never become review content.
5. Tests prove malformed review data is rejected.
6. Docs and schema fixture match the pure review result.
```
