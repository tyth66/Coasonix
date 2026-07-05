# Schema Contract Testing

Current v1 does not run a Runtime Schema Enforcement Layer on the architecture
path. The schema file is retained as a test contract fixture for the active
Reasonix review tool:

```text
../../../schemas/coasonix-v1.schema.json
```

Version evolution rules live in `../05-versioning/01-schema-contract-and-versioning.md`.

## 1. Responsibilities

```text
test-time input contract validation
test-time output contract validation
schema_version matching
strict additionalProperties handling
duplicate-key rejection tests
canonical JSON/hash support
```

## 2. Validation Flow

```text
schema contract fixture
-> validate review_diff_input_v1 in tests
-> validate review_result_v1 in tests
-> reject duplicate JSON keys in tests
```

## 3. Runtime Shape

Runtime no longer exposes `runtime.validate_schema` in v1. The MCP adapter
performs a narrow local `review_result_v1` contract check before returning
`structuredContent`.

## 4. Hard Requirements

```text
1. Invalid tool input blocks tools/call.
2. Invalid Reasonix output blocks Codex decision.
3. Runtime startup must not require a schema path.
4. Coasonix wrapper metadata must remain internally consistent when output_schema/schema_version metadata is used.
5. Unknown review payload shape fails unless an explicit compatibility path exists.
6. Wrapper must not repair semantically invalid Reasonix output.
```

## 5. Fail-Closed Cases

```text
missing Coasonix internal request identity
confidence outside 0..1
unknown review payload shape
unexpected top-level field
review output missing required fields
```

## 6. Coverage Status

The current schema fixture defines only the active v1 Reasonix review input and
output contracts:

```text
review_diff_input_v1
review_result_v1
```

Architecture impact:

```text
The architecture path intentionally stays schema-free at Runtime startup; the
fixture is for regression tests and contract documentation.
```
