import { describe, expect, test } from "bun:test";

import { errorLayerForCode, ERROR_CODES } from "./error-taxonomy";
import { healthCodexMcp } from "../codex/health";
import { validateAgentWorkerReviewResult } from "./worker-contract";

describe("Codex-facing error taxonomy", () => {
  test("maps roadmap error codes to operator layers", () => {
    expect(errorLayerForCode(ERROR_CODES.CODEX_MCP_NOT_REGISTERED)).toBe("codex");
    expect(errorLayerForCode(ERROR_CODES.SERVER_STARTUP_FAILED)).toBe("server");
    expect(errorLayerForCode(ERROR_CODES.RUNTIME_UNAVAILABLE)).toBe("runtime");
    expect(errorLayerForCode(ERROR_CODES.RUNTIME_POLICY_DENIED)).toBe("runtime");
    expect(errorLayerForCode(ERROR_CODES.RUNTIME_SCHEMA_INVALID)).toBe("runtime");
    expect(errorLayerForCode(ERROR_CODES.WORKER_UNAVAILABLE)).toBe("worker");
    expect(errorLayerForCode(ERROR_CODES.WORKER_TIMEOUT)).toBe("worker");
    expect(errorLayerForCode(ERROR_CODES.WORKER_EMPTY_STDOUT)).toBe("worker");
    expect(errorLayerForCode(ERROR_CODES.WORKER_NONZERO_EXIT)).toBe("worker");
    expect(errorLayerForCode(ERROR_CODES.WORKER_SCHEMA_INVALID)).toBe("worker");
    expect(errorLayerForCode(ERROR_CODES.BACKEND_NOT_CONFIGURED)).toBe("backend");
    expect(errorLayerForCode(ERROR_CODES.CONFIG_MISSING)).toBe("config");
  });

  test("prefix-matches runtime_* and worker_* codes from the JSON-RPC bridge", () => {
    // RuntimeWorkerError codes coming through the JSON-RPC bridge
    expect(errorLayerForCode("runtime_state_denied")).toBe("runtime");
    expect(errorLayerForCode("runtime_storage_error")).toBe("runtime");
    expect(errorLayerForCode("runtime_approval_required")).toBe("runtime");
    expect(errorLayerForCode("runtime_budget_exceeded")).toBe("runtime");
    expect(errorLayerForCode("runtime_snapshot_mismatch")).toBe("runtime");
    expect(errorLayerForCode("runtime_unknown_operation")).toBe("runtime");
    expect(errorLayerForCode("runtime_internal_error")).toBe("runtime");
    // Worker prefix matching
    expect(errorLayerForCode("worker_identity_mismatch")).toBe("worker");
    // Unknown codes fall back to worker
    expect(errorLayerForCode("unknown_code")).toBe("worker");
  });

  test("healthcheck reports Codex registration failures with a codex layer", async () => {
    const report = await healthCodexMcp({
      repoRoot: process.cwd(),
      targetRepo: process.cwd(),
      skipGatewaySmoke: true,
      run: async () => ({ exitCode: 1, stdout: "", stderr: "No MCP server named coasonix" }),
    });

    expect(report.checks[0]).toMatchObject({
      status: "fail",
      code: "codex_mcp_not_registered",
      layer: "codex",
    });
  });

  test("worker contract failures report a worker layer", () => {
    const result = validateAgentWorkerReviewResult(
      {
        schema_version: "review_diff_input_v1",
        task_id: "TASK-layer",
        request_id: "REQ-layer",
        mode: "review_diff",
        goal: "Validate error layer.",
        repo: { root: process.cwd() },
        artifacts: { diff_path: ".agent/diffs/current.diff" },
        permission_level: "L1_DIFF_REVIEW",
        output_schema: "review_result_v1",
      },
      { stdout: "", stderr: "", exitCode: 0 },
    );

    expect(result).toMatchObject({
      ok: false,
      code: "worker_empty_stdout",
      layer: "worker",
    });
  });
});
