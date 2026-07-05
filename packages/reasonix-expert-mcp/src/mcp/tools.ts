import { RuntimeWorkerError } from "../worker/client";
import { extractSingleJsonObject } from "../reasonix/output-normalizer";
import {
  EXTERNAL_REVIEW_DIFF_TOOL_NAME,
  RUNTIME_REVIEW_DIFF_OPERATION,
} from "../agent/naming";

const REVIEW_DIFF_INPUT_REF =
  "https://coasonix.local/schemas/coasonix-v1.schema.json#/$defs/review_diff_input_v1";

export interface RuntimeClient {
  call(method: string, params?: unknown): Promise<unknown>;
}

export interface ReasonixRunner {
  runReviewDiff(input: ReviewDiffInput): Promise<ReasonixRunResult>;
}

export interface ReasonixToolsAdapterOptions {
  runtime: RuntimeClient;
  reasonix: ReasonixRunner;
  reasonixCommand?: string[];
  initialized?: boolean;
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

export interface ReviewDiffInput {
  schema_version: "review_diff_input_v1";
  task_id?: string;
  request_id?: string;
  mode?: "review_diff";
  goal: string;
  repo: {
    root: string;
    base_branch?: string;
    working_branch?: string;
  };
  artifacts: {
    context_path?: string;
    diff_path: string;
    test_log_path?: string;
    build_log_path?: string;
  };
  focus?: string[];
  constraints?: string[];
  permission_level: string;
  output_schema: "review_result_v1";
}

export interface ReasonixRunResult {
  stdout: string;
  stderr: string;
  exitCode: number;
  timedOut?: boolean;
}

export function listTools() {
  return {
    tools: [
      {
        name: EXTERNAL_REVIEW_DIFF_TOOL_NAME,
        description: "Review a prepared diff through the Coasonix runtime gate.",
        inputSchema: {
          type: "object",
          $ref: REVIEW_DIFF_INPUT_REF,
        },
      },
    ],
  };
}

export function createReasonixToolsAdapter(options: ReasonixToolsAdapterOptions) {
  let nextTaskNumber = 1;
  let nextRequestNumber = 1;
  const initialized = options.initialized ?? false;

  return {
    listTools,

    async callTool(request: ToolCallRequest): Promise<ToolResult> {
      if (!initialized) {
        return errorToolResult("runtime_unavailable", "MCP server is not initialized");
      }
      if (request.name !== EXTERNAL_REVIEW_DIFF_TOOL_NAME) {
        return errorToolResult("invalid_input", `Unknown tool ${request.name}`);
      }

      const input = normalizeReviewDiffInput(
        request.arguments,
        () => `TASK-review-diff-${nextTaskNumber++}`,
        () => `REQ-review-diff-${nextRequestNumber++}`,
      );
      if (!input.ok) {
        return errorToolResult("invalid_input", input.error);
      }

      let decision: RuntimeDecisionPayload;
      try {
        decision = asRuntimeDecision(
          await options.runtime.call(
            "runtime.evaluate_operation",
            runtimeOperationRequest(input.value, options.reasonixCommand),
          ),
        );
      } catch (error) {
        return runtimeUnavailableResult(error);
      }

      if (decision.decision !== "allow") {
        return errorToolResult(
          "permission_denied",
          `Runtime decision ${decision.decision}: ${decision.reasons.join("; ")}`,
          { side_effect: "side_effect_not_executed" },
        );
      }

      let run: ReasonixRunResult;
      try {
        run = await options.reasonix.runReviewDiff(input.value);
      } catch (error) {
        return errorToolResult("reasonix_failed", error instanceof Error ? error.message : "Reasonix failed");
      }
      if (run.timedOut) {
        return errorToolResult("timeout", "Reasonix invocation timed out", {
          diagnostics: { stderr: run.stderr },
        });
      }
      if (run.exitCode !== 0) {
        return errorToolResult("reasonix_failed", `Reasonix exited with ${run.exitCode}`, {
          diagnostics: { stderr: run.stderr },
        });
      }

      const parsed = extractSingleJsonObject(run.stdout);
      if (!parsed.ok) {
        return errorToolResult("reasonix_failed", parsed.error, {
          diagnostics: { stderr: run.stderr },
        });
      }

      if (
        parsed.value.task_id !== input.value.task_id ||
        parsed.value.request_id !== input.value.request_id
      ) {
        return errorToolResult(
          "schema_validation_failed",
          "Reasonix output task_id/request_id did not match request",
          { diagnostics: { stderr: run.stderr } },
        );
      }

      const validation = asSchemaValidation(
        await options.runtime.call("runtime.validate_schema", {
          task_id: input.value.task_id,
          request_id: input.value.request_id,
          expected_schema: "review_result_v1",
          payload: parsed.value,
        }),
      );
      if (!validation.valid) {
        return errorToolResult("schema_validation_failed", "Reasonix output failed schema validation", {
          diagnostics: { stderr: run.stderr, schema_errors: validation.errors },
        });
      }

      return {
        isError: false,
        content: [
          {
            type: "text",
            text: String(parsed.value.summary ?? "Reasonix review completed."),
          },
        ],
        structuredContent: parsed.value,
        _meta: diagnosticsMeta(run.stderr),
      };
    },
  };
}

type NormalizedInput =
  | { ok: true; value: ReviewDiffInput }
  | { ok: false; error: string };

interface RuntimeDecisionPayload {
  schema_version: "runtime_decision_v1";
  task_id: string;
  request_id?: string;
  operation: string;
  decision: "allow" | "deny" | "require_approval" | "retryable_error" | "fatal_error";
  engine_results: Record<string, string>;
  reasons: string[];
}

interface SchemaValidationPayload {
  schema_version: "schema_validation_result_v1";
  task_id: string;
  request_id?: string;
  expected_schema: string;
  valid: boolean;
  errors: unknown[];
}

function normalizeReviewDiffInput(
  value: unknown,
  nextTaskId: () => string,
  nextRequestId: () => string,
): NormalizedInput {
  if (!value || typeof value !== "object") {
    return { ok: false, error: "Tool arguments must be an object" };
  }
  const input = value as Partial<ReviewDiffInput>;
  if (input.schema_version !== "review_diff_input_v1") {
    return { ok: false, error: "schema_version must be review_diff_input_v1" };
  }
  if (!input.goal || !input.repo?.root || !input.artifacts?.diff_path) {
    return { ok: false, error: "goal, repo.root, and artifacts.diff_path are required" };
  }
  if (input.permission_level !== "L1_DIFF_REVIEW") {
    return { ok: false, error: "permission_level must be L1_DIFF_REVIEW" };
  }
  if (input.output_schema !== "review_result_v1") {
    return { ok: false, error: "output_schema must be review_result_v1" };
  }

  return {
    ok: true,
    value: {
      ...input,
      task_id: input.task_id ?? nextTaskId(),
      request_id: input.request_id ?? nextRequestId(),
      mode: "review_diff",
      focus: input.focus ?? [],
      constraints: input.constraints ?? [],
    } as ReviewDiffInput,
  };
}

function runtimeOperationRequest(input: ReviewDiffInput, reasonixCommand = ["reasonix", "review-diff"]) {
  return {
    task_id: input.task_id,
    request_id: input.request_id,
    operation: RUNTIME_REVIEW_DIFF_OPERATION,
    permission_level: input.permission_level,
    resources: {
      read_paths: artifactReadPaths(input),
      write_paths: [`.agent/results/${input.request_id}.json`],
      network: false,
      command: reasonixCommand,
    },
  };
}

function artifactReadPaths(input: ReviewDiffInput): string[] {
  return [
    input.artifacts.context_path,
    input.artifacts.diff_path,
    input.artifacts.test_log_path,
    input.artifacts.build_log_path,
  ].filter((path): path is string => Boolean(path));
}

function asRuntimeDecision(value: unknown): RuntimeDecisionPayload {
  if (!value || typeof value !== "object") {
    throw RuntimeWorkerError.unavailable("runtime.evaluate_operation returned invalid payload");
  }
  return value as RuntimeDecisionPayload;
}

function asSchemaValidation(value: unknown): SchemaValidationPayload {
  if (!value || typeof value !== "object") {
    throw RuntimeWorkerError.unavailable("runtime.validate_schema returned invalid payload");
  }
  return value as SchemaValidationPayload;
}

function runtimeUnavailableResult(error: unknown): ToolResult {
  const code = error instanceof RuntimeWorkerError ? error.code : "runtime_unavailable";
  const message = error instanceof Error ? error.message : "Runtime worker is unavailable";
  return errorToolResult(code, message, { side_effect: "side_effect_not_executed" });
}

function errorToolResult(
  status: string,
  summary: string,
  meta: Record<string, unknown> = {},
): ToolResult {
  return {
    isError: true,
    content: [
      {
        type: "text",
        text: `${status}: ${summary}`,
      },
    ],
    _meta: meta,
  };
}

function diagnosticsMeta(stderr: string): Record<string, unknown> | undefined {
  return stderr ? { diagnostics: { stderr } } : undefined;
}
