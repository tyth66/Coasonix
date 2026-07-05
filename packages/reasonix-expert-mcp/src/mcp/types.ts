import type { AgentRunResult, AgentRunner } from "../agents/types";

// ── MCP-facing types ──

export interface RuntimeClient {
  call(method: string, params?: unknown): Promise<unknown>;
}

export interface ToolCallRequest {
  name: string;
  arguments?: unknown;
}

export interface ToolResult {
  isError: boolean;
  content: Array<{ type: "text"; text: string }>;
  structuredContent?: Record<string, unknown>;
  _meta?: Record<string, unknown>;
}

// ── Tool handler interface ──

export interface ToolHandler {
  readonly name: string;
  readonly description: string;
  readonly inputSchema: object;
  normalizeInput(value: unknown, nextTaskId: () => string, nextRequestId: () => string): { ok: true; value: unknown } | { ok: false; error: string };
  buildRuntimeRequest(input: unknown, agentCommand?: string[]): Record<string, unknown>;
  invokeAgent(runner: AgentRunner, input: unknown): Promise<AgentRunResult>;
  validateOutput(value: Record<string, unknown>): { path: string; message: string } | null;
}

// ── Adapter options ──

export interface ReasonixToolsAdapterOptions {
  runtime: RuntimeClient;
  agent: AgentRunner;
  agentCommand?: string[];
  initialized?: boolean;
}
