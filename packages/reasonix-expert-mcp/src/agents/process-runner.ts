import type { AgentRunResult } from "./types";

interface ProcessInput {
  goal?: string;
  repo?: { root?: string };
  artifacts?: { diff_path?: string };
  [key: string]: unknown;
}

export interface AgentProcessRunnerOptions {
  command: string[];
  timeoutMs: number;
}

export class AgentProcessRunner {
  private readonly command: string[];
  private readonly timeoutMs: number;

  constructor(options: AgentProcessRunnerOptions) {
    this.command = options.command;
    this.timeoutMs = options.timeoutMs;
  }

  async runReviewDiff(input: ProcessInput): Promise<AgentRunResult> {
    const process = Bun.spawn(this.command, {
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = new Response(process.stdout).text();
    const stderr = new Response(process.stderr).text();

    process.stdin.write(JSON.stringify(input));
    process.stdin.end();

    let timedOut = false;
    let timeoutId: ReturnType<typeof setTimeout>;
    const timeout = new Promise<null>((resolve) => {
      timeoutId = setTimeout(() => {
        timedOut = true;
        try {
          process.kill();
        } catch {
          // Process may already have exited.
        }
        resolve(null);
      }, this.timeoutMs);
    });

    const exitCode = await Promise.race([process.exited, timeout]);
    clearTimeout(timeoutId!);
    const [stdoutText, stderrText] = await Promise.all([stdout, stderr]);

    return {
      stdout: stdoutText,
      stderr: stderrText,
      exitCode: exitCode ?? -1,
      timedOut,
    };
  }
}

