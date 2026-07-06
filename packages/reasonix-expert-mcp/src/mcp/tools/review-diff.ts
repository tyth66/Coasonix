

import {
  EXTERNAL_REVIEW_DIFF_TOOL_NAME,
  RUNTIME_REVIEW_DIFF_OPERATION,
} from "../../agent/naming";

import type { AgentRunResult, AgentRunner } from "../../backends/core/interfaces";
import type { ToolHandler } from "../types";


// ── Input schema ──

const INPUT_SCHEMA = {
  type: "object",
  additionalProperties: false,
  required: ["schema_version", "goal", "repo", "artifacts", "permission_level", "output_schema"],
  properties: {
    schema_version: { const: "review_diff_input_v1" },
    task_id: { type: "string" },
    request_id: { type: "string" },
    mode: { const: "review_diff" },
    goal: { type: "string", minLength: 1 },
    repo: {
      type: "object",
      additionalProperties: false,
      required: ["root"],
      properties: {
        root: { type: "string", minLength: 1 },
        base_branch: { type: "string" },
        working_branch: { type: "string" },
      },
    },
    artifacts: {
      type: "object",
      additionalProperties: false,
      required: ["diff_path"],
      properties: {
        context_path: { type: "string" },
        diff_path: { type: "string", minLength: 1 },
        test_log_path: { type: "string" },
        build_log_path: { type: "string" },
      },
    },
    focus: { type: "array", items: { type: "string" } },
    constraints: { type: "array", items: { type: "string" } },
    budget: {
      type: "object",
      additionalProperties: false,
      properties: {
        max_minutes: { type: "integer", minimum: 1 },
        max_output_chars: { type: "integer", minimum: 1 },
        max_steps: { type: "integer", minimum: 1 },
      },
    },
    permission_level: { const: "L1_DIFF_REVIEW" },
    output_schema: { const: "review_result_v1" },
  },
} as const;

// ── Input types ──

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
  budget?: {
    max_minutes?: number;
    max_output_chars?: number;
    max_steps?: number;
  };
  permission_level: string;
  output_schema: "review_result_v1";
}

// ── Tool handler ──

export const reviewDiffHandler: ToolHandler = {
  name: EXTERNAL_REVIEW_DIFF_TOOL_NAME,
  description: "Review a prepared diff through the Coagent runtime gate.",
  inputSchema: INPUT_SCHEMA,

  normalizeInput(value: unknown, nextTaskId: () => string, nextRequestId: () => string) {
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
        mode: "review_diff" as const,
        focus: input.focus ?? [],
        constraints: input.constraints ?? [],
      } as ReviewDiffInput,
    };
  },

  buildRuntimeRequest(input: ReviewDiffInput) {
    return {
      task_id: input.task_id,
      request_id: input.request_id,
      operation: RUNTIME_REVIEW_DIFF_OPERATION,
      permission_level: input.permission_level,
      resources: {
        read_paths: [
          input.artifacts.context_path,
          input.artifacts.diff_path,
          input.artifacts.test_log_path,
          input.artifacts.build_log_path,
        ].filter((p): p is string => Boolean(p)),
        write_paths: [`.agent/results/${input.request_id}.json`],
        network: false,
      },
    };
  },

  async invokeAgent(runner: AgentRunner, input: ReviewDiffInput): Promise<AgentRunResult> {
    return runner.runReviewDiff(input);
  },

  validateOutput(value: Record<string, unknown>): { path: string; message: string } | null {
    if (value.schema_version !== "review_result_v1") {
      return { path: "/schema_version", message: "schema_version must be review_result_v1" };
    }
    if (typeof value.status !== "string" || !["ok", "partial", "error", "timeout"].includes(value.status)) {
      return { path: "/status", message: "status must be a valid review_result_v1 status" };
    }
    if (typeof value.verdict !== "string" || !["pass", "needs_fix", "risky", "unknown", "not_applicable"].includes(value.verdict)) {
      return { path: "/verdict", message: "verdict must be a valid review_result_v1 verdict" };
    }
    if (typeof value.summary !== "string") {
      return { path: "/summary", message: "summary must be a string" };
    }
    if (typeof value.confidence !== "number" || value.confidence < 0 || value.confidence > 1) {
      return { path: "/confidence", message: "confidence must be a number between 0 and 1" };
    }
    return null;
  },
};

