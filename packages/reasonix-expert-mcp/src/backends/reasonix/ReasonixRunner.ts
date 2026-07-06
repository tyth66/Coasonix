import { ACPSessionPool } from "../acp/ACPSessionPool";
import type { AgentRunner, AgentRunResult } from "../core/interfaces";
import type { ReviewDiffInput } from "../../mcp/tools/review-diff";

// ---- Options ----

export interface ReasonixRunnerOptions {
  model: string;
  cwd: string;
  requestTimeoutMs: number;
}

// ---- Runner ----

export class ReasonixRunner implements AgentRunner {
  private pool: ACPSessionPool;

  constructor(options: ReasonixRunnerOptions) {
    this.pool = new ACPSessionPool({
      command: ["reasonix", "acp", "--model", options.model],
      cwd: options.cwd,
      requestTimeoutMs: options.requestTimeoutMs,
    });
  }

  async runReviewDiff(input: ReviewDiffInput): Promise<AgentRunResult> {
    await this.pool.ensureSession();
    const prompt = buildReviewPromptText(input);
    return this.pool.sendPrompt(prompt);
  }

  async shutdown(): Promise<void> {
    await this.pool.shutdown();
  }
}

// ---- Prompt builder ----

function buildReviewPromptText(input: ReviewDiffInput): string {
  const lines = [
    "You are reviewing a code diff.",
    "",
    `Review goal: ${input.goal}`,
    "",
    `Repository root: ${input.repo.root}`,
    "",
    "Artifacts:",
    `- diff_path: ${input.artifacts.diff_path}`,
  ];
  if (input.artifacts.context_path) {
    lines.push(`- context_path: ${input.artifacts.context_path}`);
  }
  if (input.artifacts.test_log_path) {
    lines.push(`- test_log_path: ${input.artifacts.test_log_path}`);
  }
  if (input.artifacts.build_log_path) {
    lines.push(`- build_log_path: ${input.artifacts.build_log_path}`);
  }
  if (input.focus?.length) {
    lines.push("", `Focus areas: ${input.focus.join(", ")}`);
  }
  if (input.constraints?.length) {
    lines.push("", `Constraints: ${input.constraints.join(", ")}`);
  }
  lines.push(
    "",
    "Read the diff file, analyze it, then return your review as a single JSON object with this exact schema. Return ONLY the JSON, no other text:",
    "{",
    '  "schema_version": "review_result_v1",',
    `  "task_id": "${input.task_id ?? "TASK-unknown"}",`,
    `  "request_id": "${input.request_id ?? "REQ-unknown"}",`,
    '  "status": "ok",',
    '  "verdict": "pass" | "needs_fix" | "risky" | "unknown",',
    '  "summary": "one-sentence summary",',
    '  "findings": [{ "id": "...", "severity": "blocker"|"major"|"minor"|"note", "category": "...", "file": "...", "line": N, "issue": "...", "evidence": "...", "recommendation": "...", "confidence": 0.0-1.0 }],',
    '  "tests_to_run": ["..."],',
    '  "risks": ["..."],',
    '  "assumptions": ["..."],',
    '  "confidence": 0.0-1.0',
    "}",
  );
  return lines.join("\n");
}
