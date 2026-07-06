# Coagent Documentation

## Architecture

- [Collaboration Model](architecture/00-collaboration-model.md) — Roles, boundaries, current scope
- [Runtime](architecture/01-runtime.md) — State machine, policy engine, SQLite audit
- [MCP Server](architecture/02-mcp-server.md) — rmcp integration, tool definition, backend pluggability

## Development

### Build

```powershell
# Rust MCP server (primary)
cargo build -p coagent-mcp-server

# TypeScript adapter (legacy, deprecated)
bun run --cwd=packages/reasonix-expert-mcp start:mcp
```

### Test

```powershell
cargo test --workspace    # Rust: 81 pass (3 ignored) (runtime-core, runtime-worker, mcp-server)
bun test                  # TypeScript: 82 pass, 1 skip, 0 fail
```

### Verification

```powershell
cargo build -p coagent-mcp-server
cargo test --workspace
bun test
cargo fmt --all -- --check
```

## Project Structure

```
crates/
  coagent-runtime-core/     Runtime state + policy + audit (library)
  coagent-runtime-worker/   [DEPRECATED] JSON-RPC stdio worker
  coagent-mcp-server/       Rust MCP server binary (primary)

packages/
  reasonix-expert-mcp/      [DEPRECATED] TypeScript MCP adapter

schemas/
  coagent-v1.schema.json    review_diff contract fixture
```

