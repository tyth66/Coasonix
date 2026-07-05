# Coasonix

Coasonix is a Codex-Orchestrated Reasonix Runtime: Codex remains the primary controller, executor, verifier, and final decision maker while Reasonix is invoked as a controlled expert system through `reasonix-expert`.

Start from the documentation index:

1. [docs/coasonix/README.md](docs/coasonix/README.md)
2. [docs/coasonix/00-executive-summary.md](docs/coasonix/00-executive-summary.md)
3. [Architecture](docs/coasonix/01-architecture/01-overview-and-roles.md)
4. [Runtime](docs/coasonix/02-runtime/01-global-task-state-machine.md)
5. [Reasonix Integration](docs/coasonix/03-reasonix/01-tool-contracts-and-wrapper.md)
6. [Patch and Verification](docs/coasonix/04-patch-and-verification/01-patch-transaction-model.md)
7. [Versioning](docs/coasonix/05-versioning/01-schema-contract-and-versioning.md)
8. [Roadmap](docs/coasonix/06-roadmap/01-framework-reassessment.md)
9. [v1 Implementation Blueprint](docs/coasonix/06-roadmap/07-v1-implementation-blueprint.md)
10. [Codex-Side Gateway Roadmap](docs/implementation/codex-side-gateway-roadmap.md)

Current status:

```text
Deterministic Multi-Agent Runtime Spec: complete
Runtime Enforcement Layer design: complete
Global Runtime / Project Controller isolation / Session Pool / session lane mapping: complete
MVP engineering defaults: complete
v1 technology baseline: Rust 2024 core, Bun ESM adapter, JSON-RPC stdio worker, SQLite persistence
v1 implementation blueprint: complete through M13
v1 MVP implementation: complete for Rust-gated reasonix.review_diff through a runnable MCP stdio server
Codex-side gateway productization: M12 setup installer and M13 healthcheck implemented with mock profile validation
Safe autonomous patch operation: still blocked until patch safety, approval, and verification gates are implemented
```

Canonical schema registry:

[schemas/coasonix-v1.schema.json](schemas/coasonix-v1.schema.json)

Current implementation entry points:

```text
crates/coasonix-runtime-core/      Rust runtime kernel, schema, policy, state, audit, and storage
crates/coasonix-runtime-worker/    JSON-RPC stdio worker exposing runtime methods
packages/reasonix-expert-mcp/      Bun/TypeScript MCP stdio server, adapter, worker client, and mock Reasonix runner
docs/implementation/               Implementation execution notes and verification evidence
```

Next implementation focus:

[docs/implementation/codex-side-gateway-roadmap.md](docs/implementation/codex-side-gateway-roadmap.md)

The next slice should formalize the backend-neutral Agent Worker Contract before
adding Reasonix, MimoCode, or other backend bridges.

Install the Coasonix MCP server into Codex with the mock backend profile:

```powershell
bun run setup:codex-mcp --target-repo D:\path\to\target-repo
```

The installer builds the Rust runtime worker when needed, registers `coasonix`
with `codex mcp add`, uses `bun run --silent` for protocol-clean MCP startup,
and verifies the durable Codex registration with `codex mcp get coasonix` and
`codex mcp list`. The default backend profile points at the repo-local mock
worker, not at Reasonix Desktop.

Run the Codex-side gateway healthcheck:

```powershell
bun run health:codex-mcp --target-repo D:\path\to\target-repo
```

The healthcheck validates Codex registration, starts the same protocol-clean MCP
server launch shape, confirms `initialize` and `tools/list`, runs one mock
`reasonix.review_diff` call through the Rust runtime gate, checks runtime
shutdown, and writes a concise operator report. Failures are labeled by layer,
including `codex_mcp_not_registered`, `server_startup_failed`,
`runtime_unavailable`, and worker failure codes.

Run the local MCP stdio server:

```powershell
$env:COASONIX_REPO_ROOT = "D:\path\to\repo"
$env:COASONIX_SCHEMA_PATH = "D:\Coasonix\schemas\coasonix-v1.schema.json"
$env:COASONIX_RUNTIME_WORKER = "D:\Coasonix\target\debug\coasonix-runtime-worker.exe"
$env:COASONIX_REASONIX_COMMAND_JSON = '["reasonix","review-diff"]'
bun run --silent --cwd=packages/reasonix-expert-mcp start:mcp
```

The server is intentionally narrow: it initializes the Rust runtime worker
before serving tool calls, exposes only `reasonix.review_diff`, and returns MCP
`structuredContent` only after Rust validates the Reasonix result schema.
Use `COASONIX_REASONIX_COMMAND_JSON` to point at the installed Reasonix command
or at a local mock command when running development smoke tests.
The `--silent` flag is required for MCP stdio because stdout must contain only
JSON-RPC protocol frames.
The configured Reasonix command must be a stdio worker that reads the review
request from stdin and writes one `review_result_v1` JSON object to stdout;
launching a GUI-only desktop executable is not sufficient.

Verification:

```text
cargo test --workspace
bun test
python -m json.tool schemas/coasonix-v1.schema.json > $null
cargo fmt --all -- --check
git diff --check
```
