import type { AgentRunner, AgentRunResult } from "../core/interfaces";

// MockRunner validates the full review_result_v1 contract round-trip:
// it echoes back the task_id and request_id so identity checks pass.

export class MockRunner implements AgentRunner {
  async runReviewDiff(input: {
    task_id?: string;
    request_id?: string;
    [key: string]: unknown;
  }): Promise<AgentRunResult> {
    const result = {
      schema_version: "review_result_v1",
      task_id: input.task_id ?? "TASK-mock",
      request_id: input.request_id ?? "REQ-mock",
      status: "ok",
      verdict: "pass" as const,
      summary: "Mock runner completed review.",
      findings: [] as Array<Record<string, unknown>>,
      tests_to_run: [] as string[],
      risks: [] as string[],
      assumptions: [] as string[],
      confidence: 0.9,
    };
    return {
      stdout: JSON.stringify(result),
      stderr: "",
      exitCode: 0,
    };
  }

  async shutdown(): Promise<void> {}
}
