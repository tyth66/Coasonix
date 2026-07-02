import { RuntimeWorkerError } from "./errors";

export type JsonRpcId = string | number;

export interface JsonRpcSuccess {
  jsonrpc: "2.0";
  id: JsonRpcId;
  result: unknown;
}

export interface JsonRpcFailure {
  jsonrpc: "2.0";
  id: JsonRpcId | null;
  error: {
    code: number;
    message: string;
    data?: unknown;
  };
}

export type JsonRpcResponse = JsonRpcSuccess | JsonRpcFailure;

export function encodeRequestFrame(
  id: JsonRpcId,
  method: string,
  params: unknown,
): string {
  return `${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`;
}

export function parseResponseFrame(line: string): JsonRpcResponse {
  let parsed: unknown;
  try {
    parsed = JSON.parse(line);
  } catch (error) {
    throw RuntimeWorkerError.unavailable("worker stdout was not valid JSON", error);
  }

  if (!isJsonRpcResponse(parsed)) {
    throw RuntimeWorkerError.unavailable("worker stdout was not a JSON-RPC response");
  }

  return parsed;
}

function isJsonRpcResponse(value: unknown): value is JsonRpcResponse {
  if (!value || typeof value !== "object") {
    return false;
  }
  const response = value as Record<string, unknown>;
  if (response.jsonrpc !== "2.0") {
    return false;
  }
  if (
    typeof response.id !== "string" &&
    typeof response.id !== "number" &&
    response.id !== null
  ) {
    return false;
  }
  return "result" in response || isJsonRpcErrorObject(response.error);
}

function isJsonRpcErrorObject(value: unknown): boolean {
  if (!value || typeof value !== "object") {
    return false;
  }
  const error = value as Record<string, unknown>;
  return typeof error.code === "number" && typeof error.message === "string";
}
