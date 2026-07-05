# Versioning and Compatibility Strategy

> **设计规格（Design Specification）**：此文档描述的是 post-v1 兼容性策略。
> 当前 v1 无多版本共存场景，兼容性规则尚未被代码执行。

Coasonix has four versioned surfaces: Codex contract, Wrapper protocol, Reasonix runtime contract, and Schema/tool contract. This file defines compatibility rules so the system does not gradually fragment.

## 1. Versioned Surfaces

```text
coasonix_framework_version
wrapper_protocol_version
reasonix_runtime_version
tool_contract_version
schema_family_version
policy_version
context_projection_version
reasonix_project_routing_version
reasonix_session_lane_version
```

## 2. Compatibility Matrix

| Producer | Consumer | Compatibility Rule |
|---|---|---|
| Codex | Wrapper | MCP protocol + tool schema negotiation |
| Wrapper | Reasonix | Wrapper-owned prompt and output schema |
| Reasonix | Wrapper | Strict schema validation |
| Wrapper | Codex | structuredContent schema_version |
| Policy | Wrapper | policy_version hash included in cache key |
| Context Projector | Reasonix | context_projection_v1 compatibility |
| Session Router | Reasonix | project_key and session_key compatibility |
| Session Router | Cache | lane and static_prefix_hash compatibility |

## 3. Capability Negotiation

Initialize response must include:

```json
{
  "coasonix_framework_version": "0.2",
  "wrapper_protocol_version": "1.0.0",
  "reasonix_project_routing_version": "1.0.0",
  "reasonix_session_lane_version": "1.0.0",
  "supported_schema_families": ["coasonix.v1"],
  "supported_session_lanes": ["review", "security", "debug", "performance", "architecture", "patch"],
  "supported_tools": {
    "reasonix.review_diff": ["review_result_v1"],
    "reasonix.performance_review": ["performance_review_v1"]
  },
  "deprecated_tools": [],
  "compatibility_mode": "strict"
}
```

## 4. Tool Deprecation

Deprecation phases:

```text
active
deprecated
disabled_by_default
removed
```

Rules:

```text
1. deprecated tools remain schema-valid.
2. disabled_by_default tools require explicit config.
3. removed tools must not appear in tools/list.
4. replacement tool must be documented before removal.
5. deprecation emits audit warning when called.
```

## 5. Shim Rules

Wrapper may apply compatibility shim only when:

```text
1. source schema is known.
2. target schema is known.
3. transformation is deterministic.
4. no security, permission, path, or decision field is dropped.
5. audit event schema_shim_applied is emitted.
```

Wrapper must reject when:

```text
required security field missing
permission_level missing
task_id or request_id mismatch
unknown schema_version
unknown session lane
project_key mismatch
session_key mismatch
lossy transformation required
```

## 6. Upgrade Rules

```text
1. Schema minor additions must be optional.
2. Required field changes require major version.
3. Enum removal requires major version.
4. Tool behavior semantic changes require tool contract version bump.
5. Policy tightening may happen in minor version.
6. Policy loosening requires explicit human approval in production profiles.
7. Session lane routing semantic changes require reasonix_session_lane_version bump.
8. Project key component changes require reasonix_project_routing_version bump.
```

## 7. Rollback Rules

```text
1. Wrapper rollback must preserve audit readability.
2. Codex may call older wrapper only if schema family is supported.
3. Cached results from newer schema must not be served to older Codex.
4. Rollback must invalidate result cache if prompt, schema, or policy changed.
5. Rollback must invalidate session lane reuse if routing version changed.
```

## 8. Version Drift Detection

Required checks:

```text
tools/list schema matches registry
enabled_tools exists in registry
schema file version matches docs
policy_version hash matches audit events
Wrapper prompt template hash matches cache key
Reasonix runtime version recorded in every result metadata
project_key hash recorded for every Reasonix call
session_key hash and lane recorded for every Reasonix call
patch-capable tools do not route to read-only lanes
```
