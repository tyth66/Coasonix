import type { ReasonixRunResult, ReviewDiffInput } from "../mcp/tools";

export interface ReasonixProcessRunnerOptions {
  command: string[];
  timeoutMs: number;
}

export class ReasonixProcessRunner {
  private readonly command: string[];
  private readonly timeoutMs: number;

  constructor(options: ReasonixProcessRunnerOptions) {
    this.command = options.command;
    this.timeoutMs = options.timeoutMs;
  }

  async runReviewDiff(input: ReviewDiffInput): Promise<ReasonixRunResult> {
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
