# Machine-Executable Schema Contract and Schema Versioning

> **设计规格（Design Specification）**：此文档描述的是 post-v1 schema 版本演进策略。
> 当前 v1 只有一个 schema 版本（`review_diff_input_v1` / `review_result_v1`），
> 无版本协商、向后兼容、deprecation 机制。schema fixture 位于
> `schemas/coasonix-v1.schema.json`。

本文件定义 Coasonix 的测试契约 Schema 层。当前 v1 运行时不依赖该文件启动；该文件用于锁定当前 `reasonix.review_diff` 输入/输出形状。

## 1. Canonical Schema Location

```text
../../../schemas/coasonix-v1.schema.json
```

该文件是 v1 `reasonix.review_diff` 测试契约锚点。

## 2. Required Schemas

```text
review_diff_input_v1
review_result_v1
```

## 3. Strict Validation Rules

```text
1. additionalProperties defaults to false for top-level objects.
2. task_id and request_id are required on Coasonix call-scoped metadata, not on the pure Reasonix review payload.
3. schema_version is a Coasonix compatibility/test concern, not a required field in the pure Reasonix review payload.
4. status and verdict use enum constraints.
5. confidence is number from 0 to 1.
6. file paths must be relative artifact paths unless explicitly documented.
7. unknown fields fail validation unless under metadata.
8. metadata is allowed but must not contain secrets.
```

## 4. Wrapper Behavior

```text
1. inputSchema validation fails -> invalid_input.
2. output contract validation fails -> worker_schema_invalid.
3. Coasonix compatibility metadata must remain internally consistent when such metadata is used.
4. Wrapper must not repair semantically invalid Reasonix output.
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
