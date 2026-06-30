# Schema Enforcement

Schema Enforcement Layer validates all structured inputs and outputs against the canonical registry:

```text
../schemas/coasonix-v1.schema.json
```

This document owns runtime schema enforcement behavior. Version evolution rules live in `../05-versioning/01-schema-contract-and-versioning.md`.

## 1. Responsibilities

```text
input validation
output validation
schema_version matching
strict additionalProperties handling
error_result_v1 shaping
compatibility shim decision
schema error reporting
```

## 2. Validation Flow

```text
operation request
-> validate request schema
-> validate expected payload schema name
-> execute only when Runtime decision allows
-> validate result schema
-> verify returned schema_version equals requested output_schema
-> emit schema_validation_result_v1 or fail closed
```

## 3. API Shape

Request example:

```json
{
  "schema_version": "schema_validation_request_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "expected_schema": "performance_review_v1",
  "payload": {}
}
```

Result example:

```json
{
  "schema_version": "schema_validation_result_v1",
  "task_id": "TASK-001",
  "request_id": "REQ-001",
  "expected_schema": "performance_review_v1",
  "valid": false,
  "errors": [
    {
      "path": "/confidence",
      "message": "must be <= 1"
    }
  ]
}
```

## 4. Hard Requirements

```text
1. Invalid tool input blocks tools/call.
2. Invalid Reasonix output blocks Codex decision.
3. Invalid error result is fatal wrapper error.
4. output_schema must match returned schema_version.
5. Unknown schema_version fails unless explicit shim exists.
6. Shim must emit schema_shim_applied audit event.
7. Wrapper must not repair semantically invalid Reasonix output.
```

## 5. Fail-Closed Cases

```text
missing task_id
request_id mismatch
confidence outside 0..1
unknown schema_version
unexpected top-level field
patch proposal without files_changed
performance_review without benchmark_plan
invalid error_result_v1
```

## 6. Known Coverage Gap

The runtime examples currently reference `schema_validation_request_v1` and `policy_evaluation_request_v1`. The canonical schema registry defines `schema_validation_result_v1` and `policy_evaluation_result_v1`, but not those request objects as standalone top-level schema variants.

This is a documentation/schema coverage issue to resolve in a schema-focused task. It is not changed by this structural reorganization.
