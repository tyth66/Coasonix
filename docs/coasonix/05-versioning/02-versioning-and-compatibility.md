# Versioning and Compatibility Strategy

> **Design Specification**: This document describes post-v1 compatibility
> strategy. v1 has no multi-version coexistence, no version negotiation,
> no deprecation mechanism. Compatibility rules are not enforced by code.

Coagent has four versioned surfaces: Codex contract, Wrapper protocol,
Reasonix runtime contract, and Schema/tool contract. This file defines
compatibility rules so the system does not gradually fragment.

## 1. Versioned Surfaces (Design Model)

```text
coagent_framework_version
wrapper_protocol_version
reasonix_runtime_version
tool_contract_version
schema_family_version
policy_version
context_projection_version
reasonix_project_routing_version
reasonix_session_lane_version
```

## 2. Compatibility Matrix (Design Model)

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

## 3. Capability Negotiation (Design Model)

Initialize response target shape:

```json
{
  "coagent_framework_version": "0.2",
  "wrapper_protocol_version": "1.0.0",
  "supported_schema_families": ["coagent.v1"],
  "supported_tools": {
    "reasonix.review_diff": ["review_result_v1"]
  },
  "deprecated_tools": [],
  "compatibility_mode": "strict"
}
```

**Current v1 reality**: The MCP server returns a minimal initialize response
with `protocolVersion` and `capabilities: { tools: {} }`. No version
negotiation, tool deprecation, or schema family advertisement.

## 4. Tool Deprecation (Design Model)

Deprecation phases:
```text
active -> deprecated -> disabled_by_default -> removed
```

## 5. Shim Rules (Design Model)

Wrapper may apply compatibility shim only when deterministic and safe.
Wrapper must reject when security, permission, path, or identity fields
are missing.

## 6. Upgrade / Rollback Rules (Design Model)

Post-v1. No runtime version switching exists in v1.

## 7. Current v1 Reality

```text
1. One tool: reasonix.review_diff
2. One schema family: review_diff_input_v1 / review_result_v1
3. No version negotiation
4. No deprecation
5. No shims
6. Schema fixture is a test contract, not runtime-loaded
7. Compatibility mode: strict by default (no alternatives exist)
```
