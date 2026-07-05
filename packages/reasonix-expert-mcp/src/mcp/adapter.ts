import { RuntimeWorkerError } from "../worker/client";
import { extractSingleJsonObject } from "../reasonix/output-normalizer";
import { ERROR_CODES, errorLayerForCode } from "../agent/error-taxonomy";
import type { ReasonixRunResult, ReasonixRunner } from "../reasonix/types";
import { reviewDiffHandler } from "./tools/review-diff";

// ── Shared interfaces ──

export interface RuntimeClient {
  call(method: string, params?: unknown): Promise<unknown>;
}

export interface ToolCallRequest {
  name: string;
  arguments?: unknown;
}

export interface ToolResult {
  isError: boolean;
  content: Array<{ type: "text"; text: string }>;
  structuredContent?: Record<string, unknown>;
  _meta?: Record<string, unknown>;
}

export interface ToolHandler {
  readonly name: string;
  readonly description: string;
  readonly inputSchema: object;
  normalizeInput(value: unknown, nextTaskId: () => string, nextRequestId: () => string): { ok: true; value: unknown } | { ok: false; error: string };
  buildRuntimeRequest(input: unknown, reasonixCommand?: string[]): Record<string, unknown>;
  invokeReasonix(runner: ReasonixRunner, input: unknown): Promise<ReasonixRunResult>;
  validateOutput(value: Record<string, unknown>): { path: string; message: string } | null;
}

export interface ReasonixToolsAdapterOptions {
  runtime: RuntimeClient;
  reasonix: ReasonixRunner;
  reasonixCommand?: string[];
  initialized?: boolean;
}

// ── Runtime decision payload ──

interface RuntimeDecisionPayload {
  schema_version: "runtime_decision_v1";
  task_id: string;
  request_id?: string;
  operation: string;
  decision: "allow" | "deny" | "require_approval" | "retryable_error" | "fatal_error";
  engine_results: Record<string, string>;
  reasons: string[];
}

// ── Tool registry ──

const toolRegistry = new Map<string, ToolHandler>();
toolRegistry.set(reviewDiffHandler.name, reviewDiffHandler);

// ── Public API ──

export function listTools() {
  const tools = [];
  for (const handler of toolRegistry.values()) {
    tools.push({
      name: handler.name,
      description: handler.description,
      inputSchema: handler.inputSchema,
    });
  }
  return { tools };
}

export function createReasonixToolsAdapter(options: ReasonixToolsAdapterOptions) {
  let nextTaskNumber = 1;
  let nextRequestNumber = 1;
  const initialized = options.initialized ?? false;

  return {
    listTools,

    async callTool(request: ToolCallRequest): Promise<ToolResult> {
      if (!initialized) {
        return errorToolResult(ERROR_CODES.RUNTIME_UNAVAILABLE, "MCP server is not initialized");
      }

      const handler = toolRegistry.get(request.name);
      if (!handler) {
        return errorToolResult(ERROR_CODES.RUNTIME_SCHEMA_INVALID, `Unknown tool ${request.name}`);
      }

      const input = handler.normalizeInput(
        request.arguments,
        () => `TASK-${request.name.replace(/\./g, "-")}-${nextTaskNumber++}`,
        () => `REQ-${request.name.replace(/\./g, "-")}-${nextRequestNumber++}`,
      );
      if (!input.ok) {
        return errorToolResult(ERROR_CODES.RUNTIME_SCHEMA_INVALID, input.error, {
          side_effect: "side_effect_not_executed",
        });
      }

      let decision: RuntimeDecisionPayload;
      try {
        decision = asRuntimeDecision(
          await options.runtime.call(
            "runtime.evaluate_operation",
            handler.buildRuntimeRequest(input.value, options.reasonixCommand),
          ),
        );
      } catch (error) {
        return runtimeUnavailableResult(error);
      }

      if (decision.decision !== "allow") {
        const runtimeCode =
          decision.decision === "fatal_error"
            ? ERROR_CODES.RUNTIME_UNAVAILABLE
            : ERROR_CODES.RUNTIME_POLICY_DENIED;
        return errorToolResult(
          runtimeCode,
          `Runtime decision ${decision.decision}: ${decision.reasons.join("; ")}`,
          { side_effect: "side_effect_not_executed" },
        );
      }

      let run: ReasonixRunResult;
      try {
        run = await handler.invokeReasonix(options.reasonix, input.value);
      } catch (error) {
        return errorToolResult(
          ERROR_CODES.WORKER_UNAVAILABLE,
          error instanceof Error ? error.message : "Reasonix failed",
          { side_effect: "side_effect_not_executed" },
        );
      }
      if (run.timedOut) {
        return errorToolResult(ERROR_CODES.WORKER_TIMEOUT, "Reasonix invocation timed out", {
          diagnostics: { stderr: run.stderr },
        });
      }
      if (run.exitCode !== 0) {
        return errorToolResult(
          ERROR_CODES.WORKER_NONZERO_EXIT,
          `Reasonix exited with ${run.exitCode}`,
          { diagnostics: { stderr: run.stderr } },
        );
      }

      const parsed = extractSingleJsonObject(run.stdout);
      if (!parsed.ok) {
        const parseCode = parsed.error.includes("empty")
          ? ERROR_CODES.WORKER_EMPTY_STDOUT
          : ERROR_CODES.WORKER_MALFORMED_JSON;
        return errorToolResult(parseCode, parsed.error, {
          diagnostics: { stderr: run.stderr },
        });
      }

      const parsedValue = parsed.value as Record<string, unknown>;
      const inputValue = input.value as Record<string, unknown>;
      if (parsedValue.task_id !== inputValue.task_id || parsedValue.request_id !== inputValue.request_id) {
        return errorToolResult(
          ERROR_CODES.WORKER_IDENTITY_MISMATCH,
          "Reasonix output task_id/request_id did not match request",
          { diagnostics: { stderr: run.stderr } },
        );
      }

      const validationError = handler.validateOutput(parsed.value);
      if (validationError) {
        return errorToolResult(
          ERROR_CODES.WORKER_SCHEMA_INVALID,
          "Reasonix output failed contract validation",
          { diagnostics: { stderr: run.stderr, schema_errors: [validationError] } },
        );
      }

      return {
        isError: false,
        content: [{ type: "text", text: String(parsedValue.summary ?? "Reasonix review completed.") }],
        structuredContent: parsed.value,
        _meta: diagnosticsMeta(run.stderr),
      };
    },
  };
}

function asRuntimeDecision(value: unknown): RuntimeDecisionPayload {
  if (!value || typeof value !== "object") {
    throw RuntimeWorkerError.unavailable("runtime.evaluate_operation returned invalid payload");
  }
  return value as RuntimeDecisionPayload;
}

function runtimeUnavailableResult(error: unknown): ToolResult {
  const resolvedCode =
    error instanceof RuntimeWorkerError ? error.code : ERROR_CODES.RUNTIME_UNAVAILABLE;
  const message = error instanceof Error ? error.message : "Runtime worker is unavailable";
  return errorToolResult(resolvedCode, message, { side_effect: "side_effect_not_executed" });
}

function errorToolResult(code: string, summary: string, meta: Record<string, unknown> = {}): ToolResult {
  return {
    isError: true,
    content: [{ type: "text", text: `${code}: ${summary}` }],
    _meta: { ...meta, code, layer: errorLayerForCode(code) },
  };
}

function diagnosticsMeta(stderr: string): Record<string, unknown> | undefined {
  return stderr ? { diagnostics: { stderr } } : undefined;
}
