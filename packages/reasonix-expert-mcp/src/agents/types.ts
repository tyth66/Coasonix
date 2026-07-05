// Shared types for the agent process management layer.
// MCP tools layer and agent runners both import from here,
// eliminating the tools -> runner import cycle.

export interface AgentRunResult {
  stdout: string;
  stderr: string;
  exitCode: number;
  timedOut?: boolean;
}

export interface AgentRunner {
  runReviewDiff(input: { goal: string; repo: { root: string }; artifacts: { diff_path: string }; [key: string]: unknown }): Promise<AgentRunResult>;
}
