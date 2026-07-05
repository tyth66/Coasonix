# v1 MVP Implementation Summary

**Status:** v1 MVP is complete for the read-only, Rust-gated
`reasonix.review_diff` vertical slice, including a runnable MCP stdio server
shell and official MCP SDK client compatibility.

**Boundary:** v1 proves the Coasonix runtime invariant for one controlled
Reasonix review tool. It does not implement autonomous patching, approval UI,
network exceptions, remote transports, daemon mode, or real Reasonix
credentials.

**Invariant:** every Reasonix-related side effect crosses Rust schema, state,
policy, path, shell, audit, lock, and SQLite gates before execution. Every
Reasonix result crosses Rust schema validation before it can become MCP
`structuredContent`.

## Source Boundaries

Project specifications:

```text
docs/coasonix/
schemas/coasonix-v1.schema.json
```

Runtime implementation:

```text
crates/coasonix-runtime-core/
crates/coasonix-runtime-worker/
packages/reasonix-expert-mcp/
```

This file is the compressed implementation handoff. Detailed architecture and
roadmap source documents remain under `docs/coasonix/`.

Next implementation focus:

```text
docs/implementation/codex-side-gateway-roadmap.md
```

The next slice is Codex-side gateway productization: reproducible Codex MCP
registration, healthcheck, and a backend-neutral worker conformance contract.
Reasonix, MimoCode, and other agents should enter later as backend bridges.

## Completed Scope

| Milestone | Completed result | Main files |
|---|---|---|
| M0 | Rust and Bun workspaces plus package layout | `Cargo.toml`, `package.json`, package scaffolds |
| M1 | Schema registry, duplicate-key rejection, canonical JSON/hash | `crates/coasonix-runtime-core/src/schema/`, `src/canonical/` |
| M2 | Task state, artifact path policy, shell/argv policy, baseline policy profile | `src/state/`, `src/artifact/`, `src/policy/` |
| M3 | Repo-local SQLite, migrations, append-only audit, locks, cache metadata | `src/storage/` |
| M4 | `RuntimeKernel` decision merge and evidence persistence | `src/kernel/` |
| M5 | Rust JSON-RPC stdio runtime worker | `crates/coasonix-runtime-worker/src/main.rs` |
| M6 | TypeScript runtime worker client | `packages/reasonix-expert-mcp/src/worker/` |
| M7 | Testable MCP tool adapter for `tools/list` and `tools/call` | `packages/reasonix-expert-mcp/src/mcp/tools.ts` |
| M8 | Mock Reasonix `review_diff` vertical slice | `packages/reasonix-expert-mcp/src/reasonix/` |
| M9 | Runnable Bun stdio MCP server shell and initialization lifecycle | `packages/reasonix-expert-mcp/src/mcp/server.ts` |
| M10 | Official MCP SDK client compatibility | `packages/reasonix-expert-mcp/src/mcp/server.test.ts` |
| M11 | Stable startup script and operator-facing environment contract | `packages/reasonix-expert-mcp/package.json`, `README.md` |
| M12 | Codex MCP setup installer with mock backend profile, protocol-clean startup args, and post-add registration verification | `package.json`, `packages/reasonix-expert-mcp/src/codex/`, `packages/reasonix-expert-mcp/src/reasonix/mock-worker.ts`, `bin/coasonix-mock-worker*` |
| M13 | Codex MCP healthcheck with registration, server startup, runtime, tools/list, mock review, and shutdown diagnostics | `packages/reasonix-expert-mcp/src/codex/health.ts`, `packages/reasonix-expert-mcp/src/codex/health.test.ts`, `package.json` |
| M14 | Backend-neutral Agent Worker Contract conformance for `review-diff` worker stdout/stdin/exit semantics | `packages/reasonix-expert-mcp/src/agent/worker-contract.ts`, `packages/reasonix-expert-mcp/src/agent/worker-contract.test.ts`, `package.json` |
| M15 | Internal tool naming migration with reserved backend-neutral alias while preserving external v1 `reasonix.review_diff` | `packages/reasonix-expert-mcp/src/agent/naming.ts`, `packages/reasonix-expert-mcp/src/mcp/tools.ts`, `packages/reasonix-expert-mcp/src/mcp/tools.test.ts` |
| M16+ | Codex-facing error taxonomy and backend profiles | `docs/implementation/codex-side-gateway-roadmap.md` |

Working v1 call path:

```text
MCP tools/call reasonix.review_diff
-> TypeScript adapter normalizes input
-> runtime.evaluate_operation over JSON-RPC stdio
-> Rust validates schema, state, policy, path, and argv
-> Rust persists runtime_decision and audit_event in SQLite
-> TypeScript invokes configured Reasonix command only after decision == allow
-> TypeScript extracts exactly one JSON object from Reasonix stdout
-> runtime.validate_schema validates review_result_v1
-> Rust persists schema validation evidence
-> TypeScript returns MCP structuredContent only for valid output
```

## Runnable MCP Server Shell

The local server entrypoint is:

```text
packages/reasonix-expert-mcp/src/index.ts
```

Stable package command:

```powershell
bun run --silent --cwd=packages/reasonix-expert-mcp start:mcp
```

Codex registration command:

```powershell
bun run setup:codex-mcp --target-repo D:\path\to\target-repo
```

The setup command:

```text
builds target/debug/coasonix-runtime-worker(.exe) when missing
registers a coasonix MCP server with codex mcp add
uses bun run --silent --cwd=<repo>/packages/reasonix-expert-mcp start:mcp
sets COASONIX_REPO_ROOT to the explicit target repository
sets COASONIX_SCHEMA_PATH and COASONIX_RUNTIME_WORKER to stable Coasonix repo paths
sets COASONIX_REASONIX_COMMAND_JSON to the repo-local mock worker profile
verifies codex mcp get coasonix and codex mcp list after registration
```

The current setup profile is intentionally mock-only. Real Reasonix Desktop,
MimoCode, or other agent bridges must wait for the M14 backend-neutral worker
conformance contract.

Codex healthcheck command:

```powershell
bun run health:codex-mcp --target-repo D:\path\to\target-repo
```

The healthcheck command:

```text
checks codex mcp get coasonix and codex mcp list
starts the MCP server with the same bun run --silent launch shape
confirms initialize returns reasonix-expert-mcp serverInfo
confirms runtime.initialize completed before the initialize response
confirms tools/list returns exactly reasonix.review_diff
parses stdout as JSON-RPC response frames only
runs one mock review_diff call through Rust runtime gates
classifies Codex, server startup, runtime worker, backend worker, and shutdown failures separately
writes a concise operator report and exits nonzero when any check fails
```

Agent Worker Contract conformance command:

```powershell
bun run conformance:agent-worker
bun run conformance:agent-worker --command-json '["worker-executable","review-diff"]'
```

The worker contract is backend-neutral:

```text
argv is [worker_executable, "review-diff"]
stdin is one review_diff_input_v1 JSON object
stdout is exactly one review_result_v1 JSON object
stderr is diagnostics only
exit 0 means the worker response is available
nonzero exit means worker failure
task_id and request_id must match the input
markdown-fenced JSON, multiple JSON objects, empty stdout, malformed JSON, schema mismatch, invalid confidence, timeout, and nonzero exit fail conformance
```

Tool naming compatibility:

```text
external v1 tool name remains reasonix.review_diff
backend-neutral alias is reserved internally as agent.review_diff
runtime operation mapping remains reasonix.review_diff for v1 Rust policy compatibility
tools/list does not expose agent.review_diff yet
```

Required environment:

```text
COASONIX_REPO_ROOT
COASONIX_SCHEMA_PATH
COASONIX_RUNTIME_WORKER
one of:
  COASONIX_REASONIX_COMMAND_JSON
  COASONIX_REASONIX_COMMAND
```

Optional environment:

```text
COASONIX_RUNTIME_REQUEST_TIMEOUT_MS = 2000
COASONIX_REASONIX_TIMEOUT_MS = 10000
```

Recommended Windows development example:

```powershell
$env:COASONIX_REPO_ROOT = "D:\path\to\target-repo"
$env:COASONIX_SCHEMA_PATH = "D:\Coasonix\schemas\coasonix-v1.schema.json"
$env:COASONIX_RUNTIME_WORKER = "D:\Coasonix\target\debug\coasonix-runtime-worker.exe"
$env:COASONIX_REASONIX_COMMAND_JSON = '["reasonix","review-diff"]'
bun run --silent --cwd=packages/reasonix-expert-mcp start:mcp
```

Configuration rules:

```text
fail startup if required config is missing
resolve repo root, schema path, and runtime worker to absolute paths
prefer COASONIX_REASONIX_COMMAND_JSON for argv-safe command configuration
if COASONIX_REASONIX_COMMAND is used, split only simple whitespace argv and reject quoted ambiguity
never execute through a shell
do not infer fallback repo roots silently
start via `bun run --silent` or an equivalent direct executable invocation so stdout stays protocol-only
point the Reasonix argv at a stdio worker that reads review input from stdin and writes one review_result_v1 JSON object to stdout
do not point the argv at a GUI-only desktop executable unless it explicitly implements that stdio contract
```

## Initialization Lifecycle

Server startup:

```text
1. load and validate environment config
2. construct RuntimeWorkerClient({ command: [COASONIX_RUNTIME_WORKER], requestTimeoutMs })
3. call runtime.initialize with repo_root, schema_path, and reasonix_executable
4. construct ReasonixProcessRunner with configured argv and timeout
5. construct createReasonixToolsAdapter({ initialized: true, runtime, reasonixCommand, reasonix })
6. start line-delimited JSON-RPC stdio handling for MCP-compatible requests
7. serve initialize, notifications/initialized, tools/list, and tools/call
```

The important lifecycle guard is:

```text
initialized: true is set only after runtime.initialize succeeds.
```

If `runtime.initialize` fails:

```text
do not serve tools/list or tools/call
write diagnostics to stderr only
attempt RuntimeWorkerClient.shutdown()
exit nonzero
```

During operation:

```text
initialize -> returns MCP server capabilities with tools support
notifications/initialized -> acknowledged as a notification
tools/list -> delegates to the testable adapter listTools()
tools/call -> delegates to adapter.callTool()
unknown method -> Method not found
```

The server shell never:

```text
calls Reasonix directly
interprets allow/deny itself
writes .agent state directly
exposes resources, prompts, sampling, logging, patch, or approval surfaces
adds post-v1 tools
```

Shutdown:

```text
stdin close -> RuntimeWorkerClient.shutdown() -> process exit
SIGINT/SIGTERM -> idempotent shutdown -> exit
uncaught fatal error -> diagnostic on stderr -> shutdown attempt -> nonzero exit
stdout remains reserved for JSON-RPC protocol frames only
```

## Verified Behavior

Current test coverage proves:

```text
schema registry loads and validates v1 payloads
duplicate JSON keys fail before schema validation
canonical hashes are stable across object key ordering
illegal or terminal task state transitions are denied
path traversal, outside-repo paths, symlink escapes, and denylisted paths fail
shell strings and argv bypasses fail
network access is denied by default
runtime decisions and audit events commit atomically
audit rows are append-only
JSON-RPC worker exposes only v1 runtime methods
worker stdout contains JSON-RPC frames only
TypeScript worker client handles timeout, crash, restart, and unavailable cases
tools/list exposes only reasonix.review_diff
tools/call asks Rust before Reasonix invocation
deny/unavailable paths do not invoke Reasonix
valid review_result_v1 becomes structuredContent
malformed, mismatched, timed-out, or nonzero Reasonix output is rejected
real Bun server process serves tools/list and tools/call over stdio
official MCP SDK Client can connect through StdioClientTransport
transport close shuts the runtime worker down cleanly
package exposes start:mcp as the stable local server command
start:mcp invocation is documented with Bun silent mode for protocol-clean stdout
README documents the minimum runtime environment contract
setup:codex-mcp builds/registers/verifies a Codex MCP entry with stable paths
mock profile worker emits one review_result_v1 JSON object over stdout
health:codex-mcp reports codex_mcp_not_registered separately from server_startup_failed
health:codex-mcp reports runtime_unavailable separately from worker_nonzero_exit
health:codex-mcp passes against the mock profile through the real MCP server process
conformance:agent-worker passes against the repo-local mock worker
Agent Worker Contract validation rejects timeout, empty stdout, malformed JSON, multiple JSON objects, markdown-fenced JSON, wrong task_id, wrong request_id, schema mismatch, nonzero exit, and invalid confidence
M15 naming constants preserve reasonix.review_diff externally while reserving agent.review_diff internally
tools/list still exposes only reasonix.review_diff
```

Repository verification command set:

```text
cargo test --workspace
bun test
python -m json.tool schemas/coasonix-v1.schema.json > $null
cargo fmt --all -- --check
git diff --check
```

## Explicit Non-Goals

These remain post-v1 and must not be added without matching schemas, runtime
gates, denial tests, malformed-output tests, audit events, and documentation:

```text
real Reasonix credentials
reasonix.propose_patch
patch apply
patch transaction commit
human approval UI
network allow exceptions
remote HTTP transport
local daemon
multi-repo worker sharing
project-level shared session lane reuse
advanced Project Controller cache reuse
security_audit/debug/performance/architecture/test_plan tools
Reasonix write access to Codex worktree
```

Safe autonomous patch operation is still blocked until patch safety, approval,
and verification gates are implemented and tested.

## Review Status

The current v1 MVP review found no remaining Critical or Important findings for
the read-only `reasonix.review_diff` server slice.

Review checks performed:

```text
MCP server startup fails closed on missing config
runtime.initialize occurs before any adapter is marked initialized
Reasonix argv comes from explicit structured configuration
runtime deny path prevents Reasonix invocation
SDK dependency is dev-only and used by compatibility tests, not server runtime
stdout is protocol-only and diagnostics go to stderr
transport close and process signals call runtime shutdown idempotently
documentation keeps patch/autonomous write features out of v1 scope
```
