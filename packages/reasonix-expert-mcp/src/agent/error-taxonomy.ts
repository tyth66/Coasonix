export const ERROR_CODES = {
  CONFIG_MISSING: "config_missing",
  CODEX_MCP_NOT_REGISTERED: "codex_mcp_not_registered",
  SERVER_STARTUP_FAILED: "server_startup_failed",
  RUNTIME_UNAVAILABLE: "runtime_unavailable",
  RUNTIME_POLICY_DENIED: "runtime_policy_denied",
  RUNTIME_SCHEMA_INVALID: "runtime_schema_invalid",
  WORKER_UNAVAILABLE: "worker_unavailable",
  WORKER_TIMEOUT: "worker_timeout",
  WORKER_EMPTY_STDOUT: "worker_empty_stdout",
  WORKER_MALFORMED_JSON: "worker_malformed_json",
  WORKER_NONZERO_EXIT: "worker_nonzero_exit",
  WORKER_SCHEMA_INVALID: "worker_schema_invalid",
  WORKER_IDENTITY_MISMATCH: "worker_identity_mismatch",
  BACKEND_NOT_CONFIGURED: "backend_not_configured",
} as const;

export function errorLayerForCode(code: string): ErrorLayer {
  if (code.startsWith("runtime_")) {
    return "runtime";
  }
  if (code.startsWith("worker_")) {
    return "worker";
  }
  switch (code) {
    case ERROR_CODES.CONFIG_MISSING:
      return "config";
    case ERROR_CODES.CODEX_MCP_NOT_REGISTERED:
      return "codex";
    case ERROR_CODES.SERVER_STARTUP_FAILED:
      return "server";
    case ERROR_CODES.BACKEND_NOT_CONFIGURED:
      return "backend";
    default:
      return "worker";
  }
}

export type ErrorLayer = "config" | "codex" | "server" | "runtime" | "worker" | "backend";

export type ErrorCode = (typeof ERROR_CODES)[keyof typeof ERROR_CODES];
