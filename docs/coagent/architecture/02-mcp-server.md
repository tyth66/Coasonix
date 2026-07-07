# MCP Server (rmcp)

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
        // 1. Validate input → 2. Runtime gate → 3. Invoke backend →
        // 4. Validate output → 5. Complete/Fail lifecycle → 6. Wrap response
    }
}

#[tool_handler]
impl ServerHandler for CoagentServer {
    fn get_info(&self) -> ServerInfo { ... }
}

#[tokio::main]
async fn main() {
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
}
```

## Request Flow

```
Codex MCP tools/call
  -> rmcp dispatches to #[tool] handler
  -> handler validates input (ReviewDiffInput::validate)
  -> handler calls RuntimeKernel::evaluate_operation (same process)
  -> on allow: invokes Backend (Mock or Reasonix ACP)
  -> validates output (PureReviewResult::validate)
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

Selected via `COAGENT_BACKEND` env var (default: mock).

## Reasonix ACP Contract Tests

The live Reasonix integration test is ignored by default because it requires the
Reasonix CLI and external model credentials. The backend boundary is covered by
local contract tests that create a fake Reasonix executable and exercise the
same `Command::spawn` + stdin/stdout JSON-RPC path used in production.

Covered protocol cases:

- initialize/session handshake error frames preserve backend error messages
- `session/update` agent text chunks are collected into a review result
- invalid model output is rejected as `parse review`
- prompt-time process EOF is surfaced as a protocol error instead of waiting for
  the wall-clock timeout

## Input/Output Schema

`ReviewDiffInput` derives `schemars::JsonSchema` — rmcp auto-generates
MCP `inputSchema` from the struct fields. `PureReviewResult` is the
canonical review output, wrapped with `CoagentReviewWrapper` metadata.

## Deployment

```powershell
# Build
cargo build --release -p coagent-mcp-server

# Register with Codex (mock backend)
codex mcp add coagent `
  --env COAGENT_REPO_ROOT=D:\your-repo `
  -- D:\Coagent\target\release\coagent-mcp-server.exe
```
