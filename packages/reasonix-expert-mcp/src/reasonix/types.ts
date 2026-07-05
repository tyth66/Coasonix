// Shared types for the Reasonix process management layer.
// MCP tools layer and reasonix runner both import from here,
// eliminating the tools -> runner import cycle.

export interface ReasonixRunResult {
  stdout: string;
  stderr: string;
  exitCode: number;
  timedOut?: boolean;
}

export interface ReasonixRunner {
  runReviewDiff(input: { goal: string; repo: { root: string }; artifacts: { diff_path: string }; [key: string]: unknown }): Promise<ReasonixRunResult>;
}
