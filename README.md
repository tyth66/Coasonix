# Coasonix

Coasonix connects two agent systems without merging their responsibilities:

```text
Codex   = assigns work and makes the final decision
Coasonix = performs safe protocol translation, runtime gating, and audit
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
risks, assumptions, and confidence. Coasonix keeps runtime decisions, backend
status, audit ids, task routing, and protocol metadata internal.

## Implementation Status

```text
MCP setup / registration:                 implemented
MCP server stdio startup:                 implemented
inline tools/list inputSchema:            implemented
Rust pre-Reasonix runtime gate:           implemented
  - State engine (Created->Running->Completed/Failed)
  - Policy engine (operation, permission, path, argv, network)
  - SQLite append-only audit (10 tables, WAL, FK)
  - JSON Schema validation + duplicate-key detection
Rust JSON-RPC stdio Runtime Worker:       implemented
TypeScript Runtime Worker client:         implemented
mock review_diff vertical slice:          implemented
healthcheck / conformance / error taxonomy: implemented
pure Reasonix review-only result contract: active transition
patch / approval / autonomous write path:  out of scope
```

The existing code path is operational but transitional: the current mock/backend
contract still uses envelope fields such as `schema_version`, `task_id`, and
`request_id`. The active plan is to move those fields into Coasonix internals and
make Reasonix return only the review result.

## Roadmap

The gap analysis from current MVP to a real agent-to-agent delegation system:

[docs/implementation/gaps-to-production.md](docs/implementation/gaps-to-production.md)

## Active Plan

[docs/implementation/review-diff-agent-collaboration-plan.md](docs/implementation/review-diff-agent-collaboration-plan.md)

## Useful Commands

Install the Coasonix MCP server into Codex with the mock backend profile:

```powershell
bun run setup:codex-mcp --target-repo D:\path\to\target-repo
```

Run the Codex-side healthcheck:

```powershell
bun run health:codex-mcp --target-repo D:\path\to\target-repo
```

Run the local MCP stdio server directly:

```powershell
$env:COASONIX_REPO_ROOT = "D:\path\to\repo"
$env:COASONIX_RUNTIME_WORKER = "D:\Coasonix\target\debug\coasonix-runtime-worker.exe"
$env:COASONIX_AGENT_COMMAND_JSON = '["reasonix","review-diff"]'
bun run --silent --cwd=packages/reasonix-expert-mcp start:mcp
```

Run local verification:

```powershell
cargo test --workspace
bun test
python -m json.tool schemas/coasonix-v1.schema.json > $null
cargo fmt --all -- --check
git diff --check
```



