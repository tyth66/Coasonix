// Core interfaces for Coagent backend agents.

export interface AgentRunResult {
  stdout: string;
  stderr: string;
  exitCode: number;
  timedOut?: boolean;
}

export interface AgentRunner {
  runReviewDiff(input: {
    goal: string;
    repo: { root: string };
    artifacts: { diff_path: string };
    [key: string]: unknown;
  }): Promise<AgentRunResult>;
  shutdown(): Promise<void>;
}
