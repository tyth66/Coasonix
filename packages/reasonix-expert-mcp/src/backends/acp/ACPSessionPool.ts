import { ACPClient, type ACPNotificationCallback } from "./ACPClient";
import type {
  InitializeResult,
  SessionNewResult,
  SessionUpdateParams,
  ToolCallUpdate,
  SessionPromptResult,
} from "./ACPTypes";
import { extractSingleJsonObject } from "../core/output-normalizer";
import type { AgentRunResult } from "../core/interfaces";

// ---- Options ----

export interface ACPSessionConfig {
  command: string[];      // e.g. ["reasonix", "acp", "--model", "deepseek-pro"]
  cwd: string;
  requestTimeoutMs: number;
}

// ---- Session Pool ----

// ACPSessionPool manages a long-lived ACP session to a single agent backend.
// It handles initialize → session/new → session/prompt and collects
// agent_message_chunk notifications to extract the review JSON.
//
// Each agent backend (Reasonix, MiMoCode, ...) extends or composes this pool,
// providing only its launch command and prompt template.

export class ACPSessionPool {
  private readonly config: ACPSessionConfig;
  private client: ACPClient | null = null;
  private sessionId: string | null = null;
  private initError: Error | null = null;

  constructor(config: ACPSessionConfig) {
    this.config = config;
  }

  async ensureSession(): Promise<void> {
    if (this.client && this.sessionId) return;
    if (this.initError) throw this.initError;

    const client = new ACPClient({
      command: this.config.command,
      cwd: this.config.cwd,
      requestTimeoutMs: this.config.requestTimeoutMs,
    });

    this.client = client;

    try {
      await client.start();
      await client.call("initialize", {
        protocolVersion: 1,
        clientInfo: { name: "coagent", version: "0.1.0" },
      });
      const sessionResult = (await client.call("session/new", {
        cwd: this.config.cwd,
      })) as SessionNewResult;
      this.sessionId = sessionResult.sessionId;
    } catch (error) {
      this.initError = error instanceof Error ? error : new Error(String(error));
      throw this.initError;
    }
  }

  async sendPrompt(promptText: string): Promise<AgentRunResult> {
    const client = this.client;
    if (!client || !this.sessionId) {
      return { stdout: "", stderr: "ACP session not initialized", exitCode: 1 };
    }

    let collectedText = "";
    let stderrBuf = "";

    return new Promise<AgentRunResult>((resolve) => {
      const callback: ACPNotificationCallback = (method, params) => {
        if (method !== "session/update") return;
        const update = (params as SessionUpdateParams).update;
        if (!update || typeof update !== "object" || !("sessionUpdate" in update)) return;

        const u = update as { sessionUpdate: string; content?: { text?: string } };

        if (u.sessionUpdate === "agent_message_chunk" && u.content?.text) {
          collectedText += u.content.text;
        }
        if (u.sessionUpdate === "tool_call_update") {
          const tu = update as ToolCallUpdate;
          if (tu.status === "failed" && tu.content?.[0]?.content?.text) {
            stderrBuf += tu.content[0].content.text + "\n";
          }
        }
      };

      client.onNotification(callback);

      client
        .call("session/prompt", {
          sessionId: this.sessionId!,
          prompt: [{ type: "text", text: promptText }],
        })
        .then((raw) => {
          const r = raw as SessionPromptResult;
          const extracted = extractSingleJsonObject(collectedText);
          resolve({
            stdout: extracted.ok ? JSON.stringify(extracted.value) : collectedText,
            stderr: stderrBuf.trim() || (extracted.ok ? "" : extracted.error),
            exitCode: extracted.ok ? 0 : 1,
          });
        })
        .catch((error) => {
          resolve({
            stdout: collectedText,
            stderr: error instanceof Error ? error.message : "ACP prompt failed",
            exitCode: 1,
          });
        });
    });
  }

  async shutdown(): Promise<void> {
    if (this.client) {
      await this.client.shutdown();
      this.client = null;
      this.sessionId = null;
      this.initError = null;
    }
  }
}
