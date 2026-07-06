import type { Subprocess } from "bun";
import type { JsonRpcId } from "./ACPTypes";

// ---- ACP frame types ----

interface AcpRequestFrame {
  jsonrpc: "2.0";
  id: JsonRpcId;
  method: string;
  params: unknown;
}

interface AcpNotificationFrame {
  jsonrpc: "2.0";
  method: string;
  params: unknown;
}

interface AcpInboundResponse {
  kind: "response";
  id: JsonRpcId;
  result: unknown;
  error?: { code: number; message: string };
}

interface AcpInboundNotification {
  kind: "notification";
  method: string;
  params: unknown;
}

type AcpInboundFrame = AcpInboundResponse | AcpInboundNotification;

// ---- Client options ----

export interface ACPClientOptions {
  command: string[];
  cwd: string;
  requestTimeoutMs: number;
}

// ---- Pending request ----

interface PendingRequest {
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
  timer: ReturnType<typeof setTimeout>;
}

// ---- Notification callback ----

export type ACPNotificationCallback = (method: string, params: unknown) => void;

// ---- Client ----

export class ACPClient {
  private readonly command: string[];
  private readonly cwd: string;
  private readonly requestTimeoutMs: number;
  private process: Subprocess<"pipe", "pipe", "pipe"> | null = null;
  private nextId = 1;
  private readonly pending = new Map<JsonRpcId, PendingRequest>();
  private notificationCallback: ACPNotificationCallback | null = null;
  private stopping = false;

  constructor(options: ACPClientOptions) {
    this.command = options.command;
    this.cwd = options.cwd;
    this.requestTimeoutMs = options.requestTimeoutMs;
  }

  onNotification(callback: ACPNotificationCallback): void {
    this.notificationCallback = callback;
  }

  // ---- Start the underlying process. Callers must 1) start(), 2) call() initialize, 3) call() session/new. ----

  async start(): Promise<void> {
    if (this.process) return;

    let proc: Subprocess<"pipe", "pipe", "pipe">;
    try {
      proc = Bun.spawn(this.command, {
        stdin: "pipe",
        stdout: "pipe",
        stderr: "pipe",
        cwd: this.cwd,
        env: { ...Bun.env },
      });
    } catch (error) {
      throw new ACPError("failed to start ACP agent process", error);
    }

    this.process = proc;
    void this.readStdout(proc);
    void this.drainStderr(proc);
    void proc.exited.then((exitCode) => this.handleExit(proc, exitCode));
  }

  // ---- Request (blocking) ----

  async call(method: string, params: unknown = {}): Promise<unknown> {
    const proc = this.process;
    if (!proc) throw new ACPError("ACP client not started");
    const id = this.nextId++;

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new ACPError(`ACP request ${method} timed out after ${this.requestTimeoutMs}ms`));
        void this.stopProcess();
      }, this.requestTimeoutMs);

      this.pending.set(id, { resolve, reject, timer });

      const frame: AcpRequestFrame = { jsonrpc: "2.0", id, method, params };
      try {
        proc.stdin.write(JSON.stringify(frame) + "\n");
        proc.stdin.flush();
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(new ACPError("failed to write ACP request", error));
        void this.stopProcess();
      }
    });
  }

  // ---- Notification (fire-and-forget) ----

  async notify(method: string, params: unknown = {}): Promise<void> {
    const proc = this.process;
    if (!proc) throw new ACPError("ACP client not started");
    const frame: AcpNotificationFrame = { jsonrpc: "2.0", method, params };
    try {
      proc.stdin.write(JSON.stringify(frame) + "\n");
      proc.stdin.flush();
    } catch (error) {
      throw new ACPError("failed to write ACP notification", error);
    }
  }

  // ---- Lifecycle ----

  async shutdown(): Promise<void> {
    if (!this.process) return;
    this.stopping = true;
    const proc = this.process;
    try { await this.call("session/close", {}); } catch { /* best-effort */ }
    try { proc.stdin.end(); } catch { /* ignore */ }
    try { proc.kill(); } catch { /* ignore */ }
    await proc.exited.catch(() => undefined);
    this.process = null;
    this.stopping = false;
    this.rejectAll(new ACPError("ACP client shut down"));
  }

  // ---- Internal ----

  private async readStdout(proc: Subprocess<"pipe", "pipe", "pipe">): Promise<void> {
    const reader = proc.stdout.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    try {
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        let nl = buffer.indexOf("\n");
        while (nl >= 0) {
          const line = buffer.slice(0, nl).trim();
          buffer = buffer.slice(nl + 1);
          if (line) this.handleLine(line);
          nl = buffer.indexOf("\n");
        }
      }
    } catch (error) {
      this.rejectAll(new ACPError("ACP stdout read failed", error));
    }
  }

  private handleLine(line: string): void {
    let parsed: unknown;
    try { parsed = JSON.parse(line); } catch { return; }

    const frame = classifyFrame(parsed);
    if (!frame) return;

    if (frame.kind === "response") {
      const pending = this.pending.get(frame.id);
      if (!pending) return;
      this.pending.delete(frame.id);
      clearTimeout(pending.timer);
      if (frame.error) {
        pending.reject(new ACPError(frame.error.message));
      } else {
        pending.resolve(frame.result);
      }
    } else if (frame.kind === "notification") {
      this.notificationCallback?.(frame.method, frame.params);
    }
  }

  private async drainStderr(proc: Subprocess<"pipe", "pipe", "pipe">): Promise<void> {
    const reader = proc.stderr.getReader();
    try { while (true) { const { done } = await reader.read(); if (done) break; } } catch { /* non-authoritative */ }
  }

  private handleExit(proc: Subprocess<"pipe", "pipe", "pipe">, exitCode: number): void {
    if (this.process !== proc) return;
    this.process = null;
    if (this.stopping && exitCode === 0) return;
    this.rejectAll(new ACPError(`ACP agent process exited with code ${exitCode}`));
  }

  private async stopProcess(): Promise<void> {
    const proc = this.process;
    if (!proc) return;
    this.process = null;
    try { proc.stdin.end(); } catch { /* ignore */ }
    try { proc.kill(); } catch { /* ignore */ }
    await proc.exited.catch(() => undefined);
    this.rejectAll(new ACPError("ACP agent process stopped"));
  }

  private rejectAll(error: unknown): void {
    for (const [id, pending] of this.pending) {
      this.pending.delete(id);
      clearTimeout(pending.timer);
      pending.reject(error);
    }
  }
}

// ---- Error ----

export class ACPError extends Error {
  constructor(message: string, cause?: unknown) {
    super(message);
    this.name = "ACPError";
    if (cause !== undefined) (this as Record<string, unknown>).cause = cause;
  }
}

// ---- Frame classifier ----

function classifyFrame(value: unknown): AcpInboundFrame | null {
  if (!value || typeof value !== "object") return null;
  const obj = value as Record<string, unknown>;
  if (obj.jsonrpc !== "2.0") return null;

  const hasId = obj.id !== undefined && obj.id !== null;
  const hasMethod = typeof obj.method === "string";

  if (hasMethod && !hasId) {
    return { kind: "notification", method: obj.method, params: obj.params };
  }
  if (hasId && !hasMethod) {
    const error = obj.error as { code: number; message: string } | undefined;
    return { kind: "response", id: obj.id as JsonRpcId, result: obj.result, error };
  }
  return null;
}

