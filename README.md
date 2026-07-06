# Coagent

Coagent connects two agent systems without merging their responsibilities:

```text
Codex   = assigns work and makes the final decision
Coagent = performs safe protocol translation, runtime gating, and audit
Reasonix = completes the delegated expert task
Codex   = evaluates the result and decides the next step
```

Start here:

1. [Collaboration Model](docs/coasonix/00-collaboration-model.md)
2. [Executive Summary / Status](docs/coasonix/00-executive-summary.md)
3. [Documentation Index](docs/coasonix/README.md)
4. [Active Forward Plan](docs/implementation/review-diff-agent-collaboration-plan.md)

## Current Focus

The project is intentionally narrowed to one tool:

```text
reasonix.review_diff
```

For this tool, Codex delegates a diff review task to Reasonix. Reasonix should
return review information only: verdict, summary, findings, suggested tests,
risks, assumptions, and confidence. Coagent keeps runtime decisions, backend
status, audit ids, task routing, and protocol metadata internal.

## Architecture

```text
Codex MCP Host
  -> TypeScript reasonix-expert MCP Adapter (packages/reasonix-expert-mcp)
      -> managed Rust Runtime Worker (crates/coagent-runtime-worker)
          -> Rust Runtime Core (crates/coagent-runtime-core)
      -> Reasonix CLI / mock worker
```

The TypeScript adapter handles MCP protocol (initialize, tools/list, tools/call).
Before delegating to Reasonix, the adapter calls the Rust Runtime Worker over
JSON-RPC 2.0 stdio. The Runtime Core evaluates state and policy gates.
SQLite stores append-only audit records under `.agent/coagent.sqlite`.

## Implementation Status

```text
MCP setup / registration:                  implemented (codex/setup.ts, codex/health.ts)
MCP server stdio startup:                  implemented (mcp/server.ts)
inline tools/list inputSchema:             implemented (mcp/tools/review-diff.ts)
Pluggable tool handler architecture:       implemented (strategy pattern)
Multi-operation PolicyEngine registry:     implemented
Rust pre-Reasonix runtime gate:            implemented
  - State engine (Created->Running->Completed/Failed)
  - Policy engine (operation, permission, path, argv, network)
  - SQLite append-only audit (10 tables, WAL, FK, no-UPDATE/no-DELETE triggers)
  - JSON Schema validation + duplicate-key detection (schema/mod.rs)
  - Artifact policy (path allowlist/denylist with glob matching)
Rust JSON-RPC stdio Runtime Worker:        implemented (4 methods)
TypeScript Runtime Worker client:          implemented (RuntimeWorkerClient.ts)
mock review_diff vertical slice:           implemented (621-byte echo worker)
healthcheck / conformance / error taxonomy: implemented
pure Reasonix review-only result contract: active transition
patch / approval / autonomous write path:  out of scope
```

The existing code path is operational but transitional: the current mock
backend contract still uses envelope fields such as `schema_version`, `task_id`,
and `request_id` in the review result. The active plan is to move those fields
into Coagent internals and make Reasonix return only the review result.

## Documentation Layers

| Layer | Path | Purpose |
|---|---|---|
| Product model | `docs/coasonix/00-collaboration-model.md` | Codex / Coagent / Reasonix decision chain |
| Current status | `docs/coasonix/00-executive-summary.md` | Implemented vs planned vs out-of-scope |
| Active plan | `docs/implementation/review-diff-agent-collaboration-plan.md` | Current review_diff refactoring plan |
| Architecture | `docs/coasonix/01-architecture/` | Roles, MCP communication, context architecture |
| Runtime (implemented) | `docs/coasonix/02-runtime/` | Coagent internal safety gates |
| Reasonix contract | `docs/coasonix/03-reasonix/` | Reasonix task input/output boundaries |
| Design specs (post-v1) | `docs/coasonix/04-patch-and-verification/`, `05-versioning/` | Future gate designs |
| Historical roadmap | `docs/coasonix/06-roadmap/` | Design evolution; not the current status |
| Implementation history | `docs/implementation/v1-mvp-execution-plan.md` | Historical milestone reference |
| Gap analysis | `docs/implementation/gaps-to-production.md` | From MVP to production |

## Roadmap

The gap analysis from current MVP to a real agent-to-agent delegation system:

[docs/implementation/gaps-to-production.md](docs/implementation/gaps-to-production.md)

## Active Plan

[docs/implementation/review-diff-agent-collaboration-plan.md](docs/implementation/review-diff-agent-collaboration-plan.md)

## Useful Commands

Install the Coagent MCP server into Codex with the mock backend profile:

```powershell
bun run setup:codex-mcp --target-repo D:\path\to\target-repo
```

Run the Codex-side healthcheck:

```powershell
bun run health:codex-mcp --target-repo D:\path\to\target-repo
```

Run the local MCP stdio server directly:

```powershell
$env:COAGENT_REPO_ROOT = "D:\path\to\repo"
$env:COAGENT_RUNTIME_WORKER = "D:\Coagent\target\debug\coagent-runtime-worker.exe"
$env:COAGENT_AGENT_COMMAND_JSON = '["reasonix","review-diff"]'
bun run --silent --cwd=packages/reasonix-expert-mcp start:mcp
```

Run local verification:

```powershell
cargo test --workspace
bun test
python -m json.tool schemas/coagent-v1.schema.json > $null
cargo fmt --all -- --check
git diff --check
```
