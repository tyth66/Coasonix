# Coagent Documentation Index

Coagent enables two agent systems to collaborate directly, without merging
their responsibilities:

```text
Codex   = assigns tasks, owns execution context, makes final decisions
Coagent = safely gates calls, translates protocol, records audit
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

1. [00-collaboration-model.md](00-collaboration-model.md) — Canonical role boundaries + architecture
2. [00-executive-summary.md](00-executive-summary.md) — Current status and boundaries
3. [01-architecture/01-overview-and-roles.md](01-architecture/01-overview-and-roles.md) — Architecture and role details
4. [01-architecture/02-communication-and-mcp.md](01-architecture/02-communication-and-mcp.md) — MCP communication and wrapper boundary
5. [03-reasonix/01-tool-contracts-and-wrapper.md](03-reasonix/01-tool-contracts-and-wrapper.md) — Authoritative review_diff tool contract
6. [02-runtime/02-runtime-enforcement-layer.md](02-runtime/02-runtime-enforcement-layer.md) — Runtime gate (Rust RuntimeKernel implemented)
7. [02-runtime/03-policy-engine.md](02-runtime/03-policy-engine.md) — Policy engine (PolicyEngine implemented)
8. [../../schemas/coagent-v1.schema.json](../../schemas/coagent-v1.schema.json) — Current test contract fixture
9. [../implementation/review-diff-agent-collaboration-plan.md](../implementation/review-diff-agent-collaboration-plan.md) — Active forward plan
10. [../implementation/gaps-to-production.md](../implementation/gaps-to-production.md) — Current gaps to production

## Document Layers

| Layer | Document | Purpose |
|---|---|---|
| Product model | [00-collaboration-model.md](00-collaboration-model.md) | Codex / Coagent / Reasonix decision chain + architecture |
| Current status | [00-executive-summary.md](00-executive-summary.md) | Implemented vs planned vs out-of-scope |
| Active plan | [../implementation/review-diff-agent-collaboration-plan.md](../implementation/review-diff-agent-collaboration-plan.md) | Current review_diff refactoring plan |
| Architecture | [01-architecture/](01-architecture) | Architecture and communication boundaries |
| Runtime (implemented) | [02-runtime/](02-runtime) | Coagent internal safety gates (Rust core) |
| Reasonix contract | [03-reasonix/](03-reasonix) | Reasonix task input/output boundaries |
| Design specs (post-v1) | [04-patch-and-verification/](04-patch-and-verification), [05-versioning/](05-versioning) | Future gate designs; all marked as design specification |
| Historical roadmap | [06-roadmap/](06-roadmap) | Design evolution; not the current status |
| Implementation history | [../implementation/v1-mvp-execution-plan.md](../implementation/v1-mvp-execution-plan.md) | Historical milestone reference |
| Gap analysis | [../implementation/gaps-to-production.md](../implementation/gaps-to-production.md) | From MVP to production |

## Current Status

```text
MCP setup / healthcheck / conformance:       implemented
Pluggable tool handler architecture:          implemented
Rust pre-Reasonix runtime gate:              implemented
  - State engine (Created->Running->Completed/Failed)
  - Policy engine (operation, permission, path, argv, network)
  - Artifact policy (path allowlist/denylist, glob matching)
  - SQLite append-only audit (10 tables, WAL, FK, triggers)
  - JSON Schema validation + duplicate-key detection
Rust JSON-RPC stdio Runtime Worker:          implemented (4 methods)
TypeScript Runtime Worker client:            implemented
mock review_diff vertical slice:             implemented
Reasonix pure review-only output:            active transition (see forward plan)
additional tools / patch / approval / HTTP:  out of scope
```

Old documents describing multi-tool, patch, approval, remote transport, or
real backend profiles are post-v1 design or historical background. Do not
expand the tool list until the review_diff pure-result boundary is clean.

## Verification

```powershell
cargo test --workspace        # all pass
bun test                      # 70 pass (13 fail: env/missing binaries)
python -m json.tool schemas/coagent-v1.schema.json > $null
cargo fmt --all -- --check
git diff --check
```
