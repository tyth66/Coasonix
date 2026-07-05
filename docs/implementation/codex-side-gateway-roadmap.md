# Codex-Side Gateway Roadmap

This roadmap defines the next implementation direction after the completed v1
MVP. The priority is to make Coasonix a stable Codex-side MCP gateway before
adding Reasonix, MimoCode, or other agent backends.

## Positioning

The intended layering is:

```text
Codex Desktop / Codex CLI
-> Coasonix MCP server
-> Rust Runtime Gate
-> Agent Worker Contract
-> Reasonix / MimoCode / other agent bridge
```

Codex should integrate with Coasonix, not with a specific backend agent. Backend
agents become replaceable worker implementations once the Codex gateway,
runtime gate, and conformance harness are stable.

## Current Evidence

The current v1 already proves:

```text
the MCP server can initialize over stdio
tools/list exposes reasonix.review_diff
tools/call reaches Rust runtime.evaluate_operation before any worker side effect
Rust persists decisions and audit evidence
Reasonix/mock worker stdout is schema-validated before MCP structuredContent
official MCP SDK Client can connect through StdioClientTransport
startup must use Bun silent mode because MCP stdout is protocol-only
```

The live desktop check also showed:

```text
Codex CLI can register Coasonix with codex mcp add
Coasonix can answer initialize and tools/list from that launch shape
Reasonix desktop GUI executable is not a valid v1 worker because it does not emit review_result_v1 JSON on stdout
```

Therefore the next slice should not be direct Reasonix desktop automation. It
should productize the Codex side first.

## M12: Codex MCP Installation

Add a reproducible installer command:

```text
bun run setup:codex-mcp
```

Current status:

```text
implemented
root package exposes setup:codex-mcp
default backend profile is mock
installer builds the Rust runtime worker when target/debug output is missing
installer registers coasonix through codex mcp add
installer verifies codex mcp get coasonix and codex mcp list after add
mock worker emits one review_result_v1 JSON object on stdout
```

The installer should:

```text
locate the Coasonix repository root
build or verify target/debug/coasonix-runtime-worker.exe
resolve schemas/coasonix-v1.schema.json
select a backend profile, defaulting to mock or conformance
generate argv-safe COASONIX_* environment values
run codex mcp add coasonix with protocol-clean startup args
verify codex mcp get coasonix can read the registered server
avoid temp repo paths in durable Codex config
```

The Codex launch command must use silent mode:

```text
bun run --silent --cwd=<repo>/packages/reasonix-expert-mcp start:mcp
```

Acceptance:

```text
codex mcp get coasonix succeeds
codex mcp list shows coasonix enabled
registered command uses Bun silent mode
registered env points at stable repo-local paths or explicit user-selected target paths
no global Codex config entry points at a temporary test repo
```

Implemented entrypoints:

```text
package.json -> setup:codex-mcp
packages/reasonix-expert-mcp/src/codex/setup.ts
packages/reasonix-expert-mcp/src/codex/setup.test.ts
packages/reasonix-expert-mcp/src/reasonix/mock-worker.ts
bin/coasonix-mock-worker.cmd
bin/coasonix-mock-worker
```

Usage:

```powershell
bun run setup:codex-mcp --target-repo D:\path\to\target-repo
```

The default command is safe for Codex-side gateway validation because it uses
the mock worker profile. It does not launch Reasonix Desktop, MimoCode, or any
other backend agent.

## M13: Codex MCP Healthcheck

Add a diagnostic command:

```text
bun run health:codex-mcp
```

Current status:

```text
implemented
root package exposes health:codex-mcp
healthcheck checks Codex registration with codex mcp get/list
healthcheck starts the MCP server with the same Bun silent startup shape
healthcheck validates initialize, runtime.initialize, tools/list, stdout JSON-RPC framing, mock review_diff, and shutdown
healthcheck returns nonzero when any check fails
healthcheck distinguishes codex_mcp_not_registered, server_startup_failed, runtime_unavailable, and worker failure codes
```

The healthcheck should be independent of real Reasonix/MimoCode credentials. It
should validate the Coasonix gateway itself:

```text
server process starts
initialize returns reasonix-expert-mcp serverInfo
tools/list returns exactly the expected v1 tool set
runtime.initialize succeeds
runtime worker can shutdown
stdout contains JSON-RPC frames only
stderr contains diagnostics only
mock/conformance worker can return one valid review_result_v1
```

Acceptance:

```text
healthcheck returns nonzero on missing config
healthcheck distinguishes Codex registration failure from server startup failure
healthcheck distinguishes runtime worker failure from backend worker failure
healthcheck writes a concise operator report
```

Implemented entrypoints:

```text
package.json -> health:codex-mcp
packages/reasonix-expert-mcp/src/codex/health.ts
packages/reasonix-expert-mcp/src/codex/health.test.ts
```

Usage:

```powershell
bun run health:codex-mcp --target-repo D:\path\to\target-repo
```

The healthcheck still uses the mock backend profile by default. It does not
validate Reasonix Desktop, MimoCode, credentials, or backend-specific bridge
behavior. Those remain blocked behind M14 worker conformance.

## M14: Agent Worker Contract

Promote the mock Reasonix runner into a formal conformance target and separate
the worker contract from any one backend name.

Current status:

```text
implemented
root package exposes conformance:agent-worker
contract validation is backend-neutral and lives under packages/reasonix-expert-mcp/src/agent/
repo-local mock worker passes the success conformance check
contract rejects timeout, empty stdout, malformed JSON, multiple JSON objects, markdown-fenced JSON, wrong task_id, wrong request_id, schema mismatch, nonzero exit, and invalid confidence
```

The worker contract is:

```text
argv: [worker_executable, "review-diff"]
stdin: review_diff_input_v1 JSON
stdout: exactly one review_result_v1 JSON object
stderr: diagnostics only
exit 0: successful worker response
exit nonzero: worker failure
```

Conformance cases:

```text
success
timeout
empty stdout
malformed JSON
multiple JSON objects
markdown-fenced JSON
wrong task_id
wrong request_id
schema mismatch
nonzero exit
stderr-only failure
invalid confidence
```

Acceptance:

```text
mock/conformance worker passes the full matrix
real backend bridges must pass the same matrix before being recommended
Coasonix core does not know whether the worker is Reasonix, MimoCode, or another agent
```

Implemented entrypoints:

```text
package.json -> conformance:agent-worker
packages/reasonix-expert-mcp/src/agent/worker-contract.ts
packages/reasonix-expert-mcp/src/agent/worker-contract.test.ts
```

Usage:

```powershell
bun run conformance:agent-worker
bun run conformance:agent-worker --command-json '["worker-executable","review-diff"]'
```

The first command validates the repo-local mock worker. The second validates a
candidate backend worker's success path against the same strict stdout/stdin
contract. Backend bridges still need their own malformed-output and failure-mode
tests before being recommended, but they must use this contract as the shared
acceptance surface.

## M15: Tool Naming Migration

The external v1 tool name remains:

```text
reasonix.review_diff
```

Current status:

```text
implemented as internal naming migration
EXTERNAL_REVIEW_DIFF_TOOL_NAME remains reasonix.review_diff
BACKEND_NEUTRAL_REVIEW_DIFF_ALIAS is reserved as agent.review_diff
RUNTIME_REVIEW_DIFF_OPERATION maps to reasonix.review_diff for v1 Rust policy compatibility
tools/list still exposes only reasonix.review_diff
```

For internal architecture, introduce backend-neutral terminology:

```text
agent.review_diff
agent worker
backend profile
worker command
```

Suggested compatibility path:

```text
v1.0: keep external reasonix.review_diff and introduce internal AgentWorkerContract
v1.1: add a backend-neutral alias such as coasonix.review_diff or agent.review_diff
v2: treat reasonix as one backend profile rather than the core tool namespace
```

Do not rename the public tool until clients and docs have a compatibility path.

Implemented entrypoints:

```text
packages/reasonix-expert-mcp/src/agent/naming.ts
packages/reasonix-expert-mcp/src/mcp/tools.ts
packages/reasonix-expert-mcp/src/mcp/tools.test.ts
packages/reasonix-expert-mcp/src/codex/health.ts
```

The backend-neutral alias is intentionally not exposed yet. A future alias
release must add client-facing compatibility tests, schema/policy review, and
documentation before `agent.review_diff` or another alias appears in
`tools/list`.

## M16: Codex-Facing Error Taxonomy

Improve error messages so Codex can explain failures without backend-specific
guesswork.

Recommended categories:

```text
config_missing
codex_mcp_not_registered
server_startup_failed
runtime_unavailable
runtime_policy_denied
runtime_schema_invalid
worker_unavailable
worker_timeout
worker_empty_stdout
worker_nonzero_exit
worker_schema_invalid
backend_not_configured
```

Acceptance:

```text
operator-facing errors say which layer failed
tool results preserve side_effect_not_executed when no backend side effect happened
worker stdout/stderr diagnostics never become trusted structuredContent
```

## M17: Backend Profiles

Add explicit profiles only after M12-M14 are stable:

```text
mock
conformance
reasonix-cli
mimocode-cli
```

The profile should select only the backend worker command and timeout defaults.
It must not change the Rust runtime gate or MCP tool semantics.

Example future commands:

```text
bun run setup:codex-mcp --profile conformance
bun run setup:codex-mcp --profile reasonix-cli
bun run setup:codex-mcp --profile mimocode-cli
```

## Non-Goals For The Codex-Side Slice

Do not add these while doing M12-M14:

```text
direct Reasonix desktop GUI automation
MimoCode bridge
new write tools
patch apply
human approval UI
remote HTTP transport
local daemon
network allow exceptions
```

The stop condition for the Codex-side slice is:

```text
Codex can reproducibly register Coasonix, run a healthcheck, list tools, and
complete a mock/conformance review_diff call through the same runtime gates that
real backends must use later.
```
