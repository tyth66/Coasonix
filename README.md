# Coagent

Codex-Orchestrated Reasonix Runtime — single-binary Rust MCP server.

```
Codex   = assigns work and makes the final decision
Coagent = performs safe protocol translation, runtime gating, and audit
Reasonix = completes the delegated expert task
Codex   = evaluates the result and decides the next step
```

## Quick Start

```powershell
# Build
cargo build -p coagent-mcp-server

# Register with Codex
codex mcp add coagent `
  --env COAGENT_REPO_ROOT=D:\your-repo `
  -- D:\Coagent\target\debug\coagent-mcp-server.exe
```

## Architecture

```
Codex MCP Host
  -> coagent-mcp-server.exe (~5 MB, single binary, zero dependencies)
      ├── rmcp          MCP protocol (initialize, tools/list, tools/call)
      ├── RuntimeKernel  state machine + policy engine + SQLite audit
      └── Backend        Mock | Reasonix (ACP -> DeepSeek models)
```

## Project Structure

```
crates/
  coagent-runtime-core/     Runtime state + policy + audit (library)
  coagent-runtime-worker/   [DEPRECATED] JSON-RPC stdio worker
  coagent-mcp-server/       MCP server binary (primary)

docs/coagent/              Canonical documentation
schemas/                   Contract fixtures
```

## Implementation Status

```text
MCP protocol (rmcp):                        implemented (official Rust SDK, 14.7M downloads)
Rust MCP server binary:                     implemented (single exe, same-process RuntimeKernel)
Runtime state machine:                      implemented (Created->Running->Completed/Failed/Cancelled)
Policy engine:                              implemented (operation, permission, path, network)
Artifact policy:                            implemented (allowlist/denylist, glob, traversal, symlink)
SQLite audit + runtime events:             implemented (12 tables, WAL, append-only audit/events)
Pure review result boundary:                implemented (Reasonix returns semantic-only; Coagent wraps)
Runtime lifecycle closure:                  implemented (same-process complete/fail in Rust)
Mock Reasonix backend:                      implemented (instant mock review)
Real Reasonix ACP backend:                  implemented (DeepSeek models over ACP protocol)
Reasonix ACP contract tests:                implemented (fake stdio ACP backend, no live API key)
patch / approval / autonomous write path:   out of scope
```

## Verification

```powershell
cargo test --workspace    # 94 pass, 1 ignored (live Reasonix integration)
cargo build -p coagent-mcp-server
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

## Documentation

- [Collaboration Model](docs/coagent/architecture/00-collaboration-model.md)
- [Runtime: State, Policy, Audit](docs/coagent/architecture/01-runtime.md)
- [MCP Server (rmcp)](docs/coagent/architecture/02-mcp-server.md)
- [General Agent Runtime Gaps](docs/coagent/architecture/03-general-agent-runtime-gaps.md)
- [Documentation Index](docs/coagent/README.md)

