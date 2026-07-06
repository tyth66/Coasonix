// ACP (Agent Client Protocol) type definitions. Based on
// https://agentclientprotocol.com and Reasonix internal/acp/protocol.go.

// ---- Initialize ----

export interface InitializeParams {
  protocolVersion: number;
  clientInfo?: { name: string; title?: string; version?: string };
}

export interface InitializeResult {
  protocolVersion: number;
  agentCapabilities: {
    loadSession: boolean;
    promptCapabilities: { image: boolean; audio: boolean; embeddedContext: boolean };
    mcpCapabilities: { http: boolean; sse: boolean };
  };
  agentInfo: { name: string; title?: string; version?: string };
}

// ---- Session lifecycle ----

export interface SessionNewParams {
  cwd: string;
}

export interface SessionNewResult {
  sessionId: string;
}

// ---- Content blocks ----

export interface ContentBlock {
  type: "text" | "resource";
  text?: string;
  resource?: { uri: string; mimeType?: string; text?: string };
}

// ---- session/prompt ----

export interface SessionPromptParams {
  sessionId: string;
  prompt: ContentBlock[];
}

export type StopReason = "end_turn" | "cancelled" | "error";

export interface SessionPromptResult {
  stopReason: StopReason;
}

// ---- session/cancel (notification) ----

export interface SessionCancelParams {
  sessionId: string;
}

// ---- session/update notifications (agent -> client) ----

export interface SessionUpdateParams {
  sessionId: string;
  update: SessionUpdateVariant;
}

export type SessionUpdateVariant = MessageChunk | ToolCall | ToolCallUpdate;

export interface MessageChunk {
  sessionUpdate: "agent_message_chunk" | "agent_thought_chunk";
  content: ContentBlock;
}

export interface ToolCall {
  sessionUpdate: "tool_call";
  toolCallId: string;
  title: string;
  kind: string;
  status: "pending" | "completed";
  rawInput: unknown;
}

export interface ToolCallUpdate {
  sessionUpdate: "tool_call_update";
  toolCallId: string;
  status: "completed" | "failed";
  content: Array<{ type: "content"; content: ContentBlock }>;
}

// ---- session/request_permission ----

export interface PermissionRequestParams {
  sessionId: string;
  toolCall: {
    toolCallId: string;
    title: string;
    kind: string;
    status: string;
  };
  options: Array<{
    optionId: string;
    kind: "allow_once" | "allow_always" | "reject_once" | "reject_always";
  }>;
}

export interface PermissionRequestResult {
  outcome: { outcome: "selected" | "cancelled"; optionId?: string };
}

// ---- JSON-RPC wire ----

export type JsonRpcId = string | number;
