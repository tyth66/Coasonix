# Machine-Executable Schema Contract and Schema Versioning

本文件定义 Coasonix 的机器可执行 Schema 层。所有 schema 必须使用 JSON Schema Draft 2020-12，Wrapper 必须以 strict validation 执行。

## 1. Canonical Schema Location

```text
../schemas/coasonix-v1.schema.json
```

该文件是 v1 schema registry 的机器可执行锚点。

## 2. Required Schemas

```text
task_state_v1
context_projection_v1
review_result_v1
security_audit_v1
debug_hypothesis_v1
architecture_options_v1
performance_review_v1
patch_proposal_v1
test_plan_v1
error_result_v1
codex_decision_v1
patch_safety_report_v1
patch_transaction_v1
verification_result_v1
audit_event_v1
human_approval_request_v1
transition_request_v1
transition_result_v1
schema_validation_result_v1
policy_evaluation_result_v1
runtime_operation_request_v1
runtime_decision_v1
```

## 3. Strict Validation Rules

```text
1. additionalProperties defaults to false for top-level objects.
2. task_id and request_id are required on every call-scoped object.
3. schema_version is required on every structured output.
4. status and verdict use enum constraints.
5. confidence is number from 0 to 1.
6. file paths must be relative artifact paths unless explicitly documented.
7. unknown fields fail validation unless under metadata.
8. metadata is allowed but must not contain secrets.
```

## 4. Wrapper Behavior

```text
1. inputSchema validation fails -> invalid_input.
2. outputSchema validation fails -> schema_validation_failed.
3. requested output_schema must equal returned schema_version.
4. A tool result with isError=true must still validate against error_result_v1.
5. Wrapper must not repair semantically invalid Reasonix output.
```

## 5. Version Evolution Rules

### 5.1 Patch Version

Patch-level schema revisions may:

```text
add optional fields
tighten descriptions
add enum values only if older consumers ignore unknown enum values explicitly
```

Patch-level revisions must not:

```text
remove required fields
rename fields
change field type
change status or verdict semantics
```

### 5.2 Minor Version

Minor version may add optional fields and new result types. Wrapper may accept older minor versions only when a compatibility shim exists.

### 5.3 Major Version

Major version may remove or rename fields. Major versions require explicit Codex and Wrapper capability negotiation.

## 6. Compatibility Negotiation

Every MCP initialize response should expose:

```json
{
  "schema_families": ["coasonix.v1"],
  "supported_output_schemas": [
    "review_result_v1",
    "performance_review_v1"
  ],
  "compatibility_mode": "strict"
}
```

## 7. Fallback Rules

```text
1. Codex may not silently downgrade schema.
2. Wrapper may translate old Reasonix output only through explicit shim.
3. Shim must emit audit event schema_shim_applied.
4. If no shim exists, return schema_validation_failed.
5. Fallback must never bypass security fields, path fields, or permission fields.
```

## 8. Deprecation Rules

```text
1. Deprecated schema remains accepted for one compatibility window only.
2. Deprecation must be announced in tools/list metadata.
3. Deprecated schema must still pass strict validation.
4. Removing a schema requires major version change.
```
