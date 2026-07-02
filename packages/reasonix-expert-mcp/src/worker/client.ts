import { encodeRequestFrame, parseResponseFrame, type JsonRpcId } from "./protocol";
import { RuntimeWorkerError } from "./errors";

export { RuntimeWorkerError } from "./errors";

export interface RuntimeWorkerClientOptions {
  command: string[];
  requestTimeoutMs: number;
}

interface PendingRequest {
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
  timer: ReturnType<typeof setTimeout>;
}

type WorkerProcess = ReturnType<typeof Bun.spawn>;

export class RuntimeWorkerClient {
  private readonly command: string[];
  private readonly requestTimeoutMs: number;
  private process: WorkerProcess | null = null;
  private nextRequestNumber = 1;
  private readonly pending = new Map<JsonRpcId, PendingRequest>();
  private stopping = false;

  constructor(options: RuntimeWorkerClientOptions) {
    this.command = options.command;
    this.requestTimeoutMs = options.requestTimeoutMs;
  }

  isRunning(): boolean {
    return this.process !== null;
  }

  async call(method: string, params: unknown = {}): Promise<unknown> {
    const process = await this.ensureStarted();
    const id = `REQ-${this.nextRequestNumber++}`;
    const frame = encodeRequestFrame(id, method, params);

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(RuntimeWorkerError.unavailable(`worker request ${id} timed out`));
        void this.stopProcess();
      }, this.requestTimeoutMs);

      this.pending.set(id, { resolve, reject, timer });

      try {
        process.stdin.write(frame);
        process.stdin.flush();
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(RuntimeWorkerError.unavailable("failed to write worker request", error));
        void this.stopProcess();
      }
    });
  }

  async shutdown(): Promise<unknown> {
    if (!this.process) {
      return null;
    }

    this.stopping = true;
    const process = this.process;
    try {
      const result = await this.call("runtime.shutdown", {});
      await process.exited.catch(() => undefined);
      return result;
    } finally {
      this.process = null;
      this.stopping = false;
    }
  }

  async restart(): Promise<void> {
    await this.stopProcess();
    await this.start();
  }

  async start(): Promise<void> {
    if (this.process) {
      return;
    }

    let process: WorkerProcess;
    try {
      process = Bun.spawn(this.command, {
        stdin: "pipe",
        stdout: "pipe",
        stderr: "pipe",
      });
    } catch (error) {
      throw RuntimeWorkerError.unavailable("failed to start runtime worker", error);
    }

    this.process = process;
    void this.readStdout(process);
    void this.drainStderr(process);
    void process.exited.then((exitCode) => this.handleExit(process, exitCode));
  }

  private async ensureStarted(): Promise<WorkerProcess> {
    await this.start();
    const process = this.process;
    if (!process) {
      throw RuntimeWorkerError.unavailable("runtime worker is unavailable");
    }
    return process;
  }

  private async readStdout(process: WorkerProcess): Promise<void> {
    const reader = process.stdout.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    try {
      while (true) {
        const { value, done } = await reader.read();
        if (done) {
          break;
        }
        buffer += decoder.decode(value, { stream: true });
        let newlineIndex = buffer.indexOf("\n");
        while (newlineIndex >= 0) {
          const line = buffer.slice(0, newlineIndex);
          buffer = buffer.slice(newlineIndex + 1);
          this.handleResponseLine(line);
          newlineIndex = buffer.indexOf("\n");
        }
      }
    } catch (error) {
      this.rejectAll(RuntimeWorkerError.unavailable("failed to read worker stdout", error));
    }
  }

  private handleResponseLine(line: string): void {
    let response;
    try {
      response = parseResponseFrame(line);
    } catch (error) {
      this.rejectAll(error);
      void this.stopProcess();
      return;
    }

    const pending = this.pending.get(response.id);
    if (!pending) {
      return;
    }
    this.pending.delete(response.id);
    clearTimeout(pending.timer);

    if ("error" in response) {
      pending.reject(RuntimeWorkerError.fromJsonRpcError(response.error));
      return;
    }
    pending.resolve(response.result);
  }

  private async drainStderr(process: WorkerProcess): Promise<void> {
    const reader = process.stderr.getReader();
    try {
      while (!(await reader.read()).done) {
        // Drain diagnostics so a verbose worker cannot block on a full stderr pipe.
      }
    } catch {
      // Diagnostics are non-authoritative for the client contract.
    }
  }

  private handleExit(process: WorkerProcess, exitCode: number): void {
    if (this.process !== process) {
      return;
    }
    this.process = null;
    if (this.stopping && exitCode === 0) {
      return;
    }
    this.rejectAll(
      RuntimeWorkerError.unavailable(`runtime worker exited with code ${exitCode}`),
    );
  }

  private async stopProcess(): Promise<void> {
    const process = this.process;
    if (!process) {
      return;
    }
    this.process = null;
    try {
      process.stdin.end();
    } catch {
      // The worker may already have exited.
    }
    try {
      process.kill();
    } catch {
      // The worker may already have exited.
    }
    await process.exited.catch(() => undefined);
    this.rejectAll(RuntimeWorkerError.unavailable("runtime worker stopped"));
  }

  private rejectAll(error: unknown): void {
    for (const [id, pending] of this.pending) {
      this.pending.delete(id);
      clearTimeout(pending.timer);
      pending.reject(error);
    }
  }
}
