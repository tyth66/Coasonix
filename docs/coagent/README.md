# Coagent Documentation

## Architecture

- [Collaboration Model](architecture/00-collaboration-model.md) — Roles, boundaries, current scope
- [Runtime](architecture/01-runtime.md) — State machine, policy engine, SQLite audit
- [MCP Server](architecture/02-mcp-server.md) — rmcp integration, tool definition, backend pluggability
- [General Agent Runtime Gaps](architecture/03-general-agent-runtime-gaps.md) — Deficits to close before Coagent becomes a mature general agent runtime

## Development

### Build

```powershell
# Rust MCP server (primary)
cargo build -p coagent-mcp-server
```

### Test

```powershell
cargo test --workspace    # Rust: 94 pass, 1 ignored live Reasonix integration
```

### Verification

```powershell
cargo build -p coagent-mcp-server
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
```

## Project Structure

```
crates/
  coagent-runtime-core/     Runtime state + policy + audit (library)
  coagent-runtime-worker/   [DEPRECATED] JSON-RPC stdio worker
  coagent-mcp-server/       Rust MCP server binary (primary)

schemas/
  coagent-v1.schema.json    review_diff contract fixture
```

