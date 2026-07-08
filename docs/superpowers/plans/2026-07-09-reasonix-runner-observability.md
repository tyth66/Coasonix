# ReasonixRunner Observability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add lightweight single-runner observability and a read-only runtime status tool without introducing multi-session management.

**Architecture:** `ReasonixRunner` owns an `Arc<Mutex<ReasonixRunnerStats>>` alongside its single session mutex. `AcpBackend` exposes a read-only stats snapshot, and `CoagentServer` exposes `coagent.runtime_status` as a passive MCP tool that reports selected backend, repo root, and Reasonix stats when available.

**Tech Stack:** Rust 2024, Tokio async tests, rmcp tool macros, existing fake Reasonix ACP test scripts, Markdown docs.

## Global Constraints

- Keep the current single `ReasonixRunner` and single persistent ACP session.
- Do not add session pools, `SessionKey`, multi-lane execution, or new business tools.
- Do not add dependencies.
- Use TDD: write failing tests before production changes.
- `coagent.runtime_status` must be read-only and must not call Reasonix or invoke any backend.

---

### Task 1: Add ReasonixRunnerStats

**Files:**
- Modify: `crates/coagent-mcp-server/src/backends/reasonix.rs`

**Interfaces:**
- Produces: `ReasonixRunnerStats`
- Produces: `ReasonixRunner::stats() -> ReasonixRunnerStats`

- [x] Write failing assertions in existing fake Reasonix tests for reuse, reconnect, timeout, and spawn errors.
- [x] Run targeted `cargo test -p coagent-mcp-server reasonix_runner_ --quiet` and confirm stats APIs are missing.
- [x] Add stats fields and update them during session creation, prompt attempts, reconnects, timeouts, and errors.
- [x] Run targeted Reasonix tests and confirm green.

### Task 2: Expose Backend Runtime Status

**Files:**
- Modify: `crates/coagent-mcp-server/src/backends/acp_backend.rs`
- Modify: `crates/coagent-mcp-server/src/main.rs`

**Interfaces:**
- Produces: `AcpBackend::runtime_status() -> ReasonixRuntimeStatus`
- Produces: `RuntimeStatusResponse`

- [x] Write failing unit tests for initial Reasonix backend status and mock backend status.
- [x] Add serializable runtime status structs.
- [x] Store selected backend ID, repo root, and optional `AcpBackend` handle in `CoagentServer`.
- [x] Add `coagent.runtime_status` MCP handler that returns JSON text only.

### Task 3: Register And Document Runtime Status

**Files:**
- Modify: `crates/coagent-mcp-server/src/tools/tool_spec.rs`
- Modify: `docs/coagent/architecture/02-mcp-server.md`
- Create: `docs/coagent/architecture/07-session-management.md`

**Interfaces:**
- Produces: `ToolSpec::runtime_status()`

- [x] Write failing registry test that default registry lists both built-in tools.
- [x] Register `coagent.runtime_status` as read-only.
- [x] Document current single-runner status output and future `SessionKey` direction.
- [x] Run full verification.
