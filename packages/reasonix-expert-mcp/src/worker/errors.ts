const JSON_RPC_RUNTIME_CODES = new Map<number, string>([
  [-32001, "runtime_policy_denied"],
  [-32002, "runtime_state_denied"],
  [-32003, "runtime_schema_invalid"],
  [-32004, "runtime_approval_required"],
  [-32005, "runtime_budget_exceeded"],
  [-32006, "runtime_snapshot_mismatch"],
  [-32007, "runtime_storage_error"],
  [-32008, "runtime_unavailable"],
  [-32009, "runtime_unknown_operation"],
  [-32010, "runtime_internal_error"],
]);

export class RuntimeWorkerError extends Error {
  readonly code: string;
  readonly cause?: unknown;

  constructor(code: string, message: string, cause?: unknown) {
    super(message);
    this.name = "RuntimeWorkerError";
    this.code = code;
    this.cause = cause;
  }

  static unavailable(message: string, cause?: unknown): RuntimeWorkerError {
    return new RuntimeWorkerError("runtime_unavailable", message, cause);
  }

  static fromJsonRpcError(error: {
    code: number;
    message: string;
    data?: unknown;
  }): RuntimeWorkerError {
    return new RuntimeWorkerError(
      JSON_RPC_RUNTIME_CODES.get(error.code) ?? `json_rpc_${error.code}`,
      error.message,
      error.data,
    );
  }
}
