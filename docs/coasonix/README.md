# Coasonix Documentation Index

Coasonix enables two agent systems to collaborate directly, without merging
their responsibilities:

```text
Codex   = assigns tasks, owns execution context, makes final decisions
Coasonix = safely gates calls, translates protocol, records audit
Reasonix = performs the delegated expert task
Codex   = reads the result and decides what to do next
```

The authoritative role model is in [00-collaboration-model.md](00-collaboration-model.md).

## Current Tool

```text
reasonix.review_diff
```

Only this one tool. Reasonix should return review information only, not
runtime, worker, schema, backend, or MCP protocol details.

## Reading Order

1. [00-collaboration-model.md](00-collaboration-model.md) - Canonical role boundaries
2. [00-executive-summary.md](00-executive-summary.md) - Current status and boundaries
3. [../implementation/review-diff-agent-collaboration-plan.md](../implementation/review-diff-agent-collaboration-plan.md) - Active forward plan
4. [01-architecture/01-overview-and-roles.md](01-architecture/01-overview-and-roles.md) - Architecture and role details
5. [01-architecture/02-communication-and-mcp.md](01-architecture/02-communication-and-mcp.md) - MCP communication and wrapper boundary
6. [03-reasonix/01-tool-contracts-and-wrapper.md](03-reasonix/01-tool-contracts-and-wrapper.md) - Authoritative review_diff tool contract
7. [02-runtime/02-runtime-enforcement-layer.md](02-runtime/02-runtime-enforcement-layer.md) - Runtime gate (implemented)
8. [02-runtime/03-policy-engine.md](02-runtime/03-policy-engine.md) - Policy engine (implemented)
9. [02-runtime/06-executable-runtime-details.md](02-runtime/06-executable-runtime-details.md) - Detailed executable spec (partially implemented)
10. [../../schemas/coasonix-v1.schema.json](../../schemas/coasonix-v1.schema.json) - Current test contract fixture

## Document Layers

| Layer | Document | Purpose |
|---|---|---|
| Product model | [00-collaboration-model.md](00-collaboration-model.md) | Codex / Coasonix / Reasonix decision chain |
| Current status | [00-executive-summary.md](00-executive-summary.md) | Implemented vs planned vs out-of-scope |
| Active plan | [../implementation/review-diff-agent-collaboration-plan.md](../implementation/review-diff-agent-collaboration-plan.md) | Current review_diff refactoring plan |
| Architecture | [01-architecture/](01-architecture) | Architecture and communication boundaries |
| Runtime (implemented) | [02-runtime/](02-runtime) | Coasonix internal safety gates |
| Reasonix contract | [03-reasonix/](03-reasonix) | Reasonix task input/output boundaries |
| Design specs (post-v1) | [04-patch-and-verification/](04-patch-and-verification), [05-versioning/](05-versioning) | Future gate designs |
| Historical roadmap | [06-roadmap/](06-roadmap) | Design evolution; not the current status |
| Implementation history | [../implementation/v1-mvp-execution-plan.md](../implementation/v1-mvp-execution-plan.md) | Historical milestone reference |

## Current Status

```text
MCP setup / healthcheck / conformance:       implemented
Rust pre-Reasonix runtime gate:              implemented
mock review_diff vertical slice:             implemented
Reasonix pure review-only output:            active transition (see forward plan)
additional tools / patch / approval / HTTP:  out of scope
```

Old documents describing multi-tool, patch, approval, remote transport, or
real backend profiles are post-v1 design or historical background. Do not
expand the tool list until the review_diff pure-result boundary is clean.
