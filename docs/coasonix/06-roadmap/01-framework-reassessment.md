# Framework Reassessment (Historical)

This document records the original framework reassessment from the design phase.
**It does not represent the current implementation status.** Many items marked
"implementation missing" here were subsequently implemented. See
[00-executive-summary.md](../00-executive-summary.md) and
[../../implementation/review-diff-agent-collaboration-plan.md](../../implementation/review-diff-agent-collaboration-plan.md)
for current status.

## Original Reassessment Conclusion

The framework advanced from "multi-agent architecture vision" to engineering
specification. The core conclusions remain valid:

```text
1. Codex is the sole final authority.
2. Codex and Reasonix do not communicate directly; all traffic passes through
   the Coasonix wrapper.
3. MCP carries control information; Git/files/logs/artifacts carry factual material.
4. Reasonix raw output must not enter Codex decision surface directly.
5. Security boundaries (permission, patch checker, no sampling/elicitation/resources) are defined.
6. Context boundaries: Codex holds global context; Reasonix receives projected context.
7. Verification boundary: Reasonix recommendations must pass Codex verification.
8. Audit boundary: key events, decisions, and verification results need
   repo-local SQLite append-only records.
```

## Five Nested Boundaries (Design Model)

```text
Boundary 1: Authority
  Codex owns task, state, decisions, execution, and final result.

Boundary 2: Protocol
  MCP Wrapper is the only Codex <-> Reasonix bridge.

Boundary 3: Context
  Context Projector exports minimal explicit context; no hidden memory.

Boundary 4: Execution
  Permission, sandbox, path policy, patch checker, and budget limiter gate actions.

Boundary 5: Evidence
  Verification, audit, and human approval determine what can be called complete.
```

## Implementation Status vs Original Assessment

The original assessment marked many areas "implementation missing." Current status:

| Original Area | Original Assessment | Current Status |
|---|---|---|
| Deterministic runtime spec | high readiness | Implemented: state, policy, audit in Rust core |
| Runtime enforcement | high readiness | Implemented: RuntimeKernel + PolicyEngine + SQLite audit |
| MCP MVP | high readiness | Implemented: TS MCP adapter + Rust JSON-RPC worker |
| Tool contracts (v1) | high for v1 | Implemented: review_diff exposed |
| Context projection | medium-high | Post-v1: design complete, not implemented |
| Patch safety | medium-high | Post-v1: design complete, not implemented |
| Audit (v1) | high for v1 | Implemented: SQLite append-only with triggers |
| Verification | medium-high | Post-v1: design complete |
| Human approval | medium-high | Post-v1: design complete |
| Performance review | medium-high | Post-v1: tool definition exists |
| Production HTTP | medium-low | Post-v1: not planned |

## Priority Next Work (Original)

The original priority list. Items already completed are **[DONE]**; remaining are post-v1:

```text
1. [DONE] Implement Draft 2020-12 schema validator.
2. [DONE] Implement Global Task State Machine runner.
3. [DONE] Implement executable canonicalization, path matcher, shell argv parser.
4. [POST-V1] Implement Patch Safety Checker and dry-run/apply transaction contract.
5. [DONE] Implement Policy Execution Engine for path, permission, shell, network gates.
6. [DONE] Compose Runtime Kernel decision flow.
7. [POST-V1] Implement Context Projector redaction, hashing, and adversarial tests.
8. [DONE] Implement audit_event_v1 writer.
9. [POST-V1] Implement verification runner and benchmark/profiling artifact capture.
10. [DONE] Implement STDIO Wrapper MVP.
11. [POST-V1] Add adversarial tests for prompt injection, path traversal, etc.
```

## Final Definition

Coasonix is best described as:

```text
Codex-centered expert delegation runtime with strict tool contracts,
explicit context projection, policy-bound execution, evidence-gated decisions,
and append-only auditability.
```
