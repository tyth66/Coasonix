# MCP Server (rmcp) — v2

The MCP server is built with `rmcp` (official Rust MCP SDK, 14.7M downloads).

## Tool Definition

```rust
#[tool_router]
impl CoagentServer {
    #[tool(
        name = "reasonix.review_diff",
        description = "Review a prepared diff through the Coagent runtime gate."
    )]
    async fn review_diff(
        &self,
        Parameters(input): Parameters<ReviewDiffInput>,
    ) -> Result<CallToolResult, ErrorData> {
        // 1. Validate input (SchemaRegistry — single authority, JSON Schema 2020-12)
        // 2. Runtime gate (same-process evaluate_operation)
        //     → Allow: continue to backend
        //     → Deny: return error
        //     → RequireApproval: return {"status":"approval_required",...}
        // 3. Invoke backend (Mock | Reasonix ACP)
        // 4. Validate output (SchemaRegistry)
        // 5. Complete/Fail lifecycle close
        // 6. Wrap { review, metadata } response
    }
}
```

## Request Flow

```
Codex MCP tools/call
  -> rmcp dispatches to #[tool] handler
  -> handler validates input via SchemaRegistry (JSON Schema 2020-12)
  -> handler calls RuntimeKernel::evaluate_operation (same process)
  -> on Allow: invokes Backend (Mock or Reasonix ACP)
  -> on RequireApproval: returns paused status, caller must approve
  -> validates output via SchemaRegistry
  -> calls RuntimeKernel::complete_operation or fail_operation
  -> wraps { review, metadata } result
  -> rmcp serializes to MCP CallToolResult JSON
```

## Backend Pluggability

```rust
enum Backend {
    Mock,                        // Returns PureReviewResult::mock_pass()
    Reasonix(ReasonixRunner),   // ACP protocol to DeepSeek models
}
```

By default, the server constructs the backend from the registered tool's backend
binding. `COAGENT_BACKEND=mock` or `COAGENT_BACKEND=reasonix` can override that
binding for local development and integration tests. Unknown backend override
values fail closed during configuration parsing.

## Reasonix ACP Contract Tests

5 protocol scenarios covered with fake stdio ACP backend (no live API key):

- initialize/session handshake error frames preserve backend error messages
- `session/update` agent text chunks are collected into a review result
- invalid model output is rejected as `parse review`
- prompt-time process EOF is surfaced as protocol error
- session/new error is surfaced correctly

## Input/Output Schema

`ReviewDiffInput` derives `schemars::JsonSchema` — rmcp auto-generates
MCP `inputSchema` from the struct fields. `PureReviewResult` is the
canonical review output, wrapped with `CoagentReviewWrapper` metadata.

## Schema Unification (v2)

The handwritten `ReviewDiffInput::validate()` has been replaced with a passthrough.
All validation is routed through `SchemaRegistry` using the embedded
`schemas/coagent-v1.schema.json` (JSON Schema 2020-12). The schema registry
is the single authority for:

- request validation (`review_diff_input_v1`)
- response validation (`pure_review_result_v1`)
- wrapper validation (`coagent_review_wrapper_v1`)
- duplicate-key rejection (`parse_json_no_duplicate_keys`)

## Deployment

```powershell
# Build
cargo build --release -p coagent-mcp-server

# Register with Codex (mock backend)
codex mcp add coagent `
  --env COAGENT_REPO_ROOT=D:\your-repo `
  -- D:\Coagent\target\release\coagent-mcp-server.exe
```
