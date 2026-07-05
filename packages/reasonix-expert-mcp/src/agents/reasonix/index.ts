// Reasonix agent backend registration.
// When a real Reasonix CLI exists, replace the mock with the actual invocation.
import type { AgentRunner } from "../types";
import { AgentProcessRunner } from "../process-runner";

export function createReasonixAgent(command: string[], timeoutMs: number): AgentRunner {
  return new AgentProcessRunner({ command, timeoutMs }) as unknown as AgentRunner;
}
