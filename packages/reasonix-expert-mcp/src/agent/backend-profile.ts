import { resolve } from "node:path";

import { ERROR_CODES } from "./error-taxonomy";

export type BackendProfile = "mock" | "conformance" | "reasonix-cli" | "mimocode-cli";

export interface BackendProfileEnvironment {
  COASONIX_REASONIX_CLI_COMMAND_JSON?: string;
  COASONIX_MIMOCODE_CLI_COMMAND_JSON?: string;
  COASONIX_REASONIX_TIMEOUT_MS?: string;
  COASONIX_MIMOCODE_TIMEOUT_MS?: string;
}

export interface ResolvedBackendProfile {
  profile: BackendProfile;
  command: string[];
  timeoutMs: number;
}

export interface ResolveBackendProfileOptions {
  profile: BackendProfile;
  repoRoot: string;
  env?: BackendProfileEnvironment;
}

export class BackendProfileError extends Error {
  readonly code = ERROR_CODES.BACKEND_NOT_CONFIGURED;

  constructor(message: string) {
    super(`${ERROR_CODES.BACKEND_NOT_CONFIGURED}: ${message}`);
    this.name = "BackendProfileError";
  }
}

export function resolveBackendProfile(options: ResolveBackendProfileOptions): ResolvedBackendProfile {
  const env = options.env ?? process.env;
  switch (options.profile) {
    case "mock":
    case "conformance":
      return {
        profile: options.profile,
        command: mockWorkerCommand(options.repoRoot),
        timeoutMs: timeoutMs(env.COASONIX_REASONIX_TIMEOUT_MS),
      };
    case "reasonix-cli":
      return {
        profile: options.profile,
        command: commandJson(env.COASONIX_REASONIX_CLI_COMMAND_JSON, "COASONIX_REASONIX_CLI_COMMAND_JSON"),
        timeoutMs: timeoutMs(env.COASONIX_REASONIX_TIMEOUT_MS),
      };
    case "mimocode-cli":
      return {
        profile: options.profile,
        command: commandJson(env.COASONIX_MIMOCODE_CLI_COMMAND_JSON, "COASONIX_MIMOCODE_CLI_COMMAND_JSON"),
        timeoutMs: timeoutMs(env.COASONIX_MIMOCODE_TIMEOUT_MS ?? env.COASONIX_REASONIX_TIMEOUT_MS),
      };
  }
}

function mockWorkerCommand(repoRoot: string): string[] {
  return [
    resolve(
      repoRoot,
      process.platform === "win32" ? "bin/coasonix-mock-worker.cmd" : "bin/coasonix-mock-worker",
    ),
    "review-diff",
  ];
}

function commandJson(value: string | undefined, name: string): string[] {
  if (!value) {
    throw new BackendProfileError(`${name} is required for this backend profile`);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch (error) {
    throw new BackendProfileError(`${name} must be valid JSON: ${String(error)}`);
  }
  if (
    !Array.isArray(parsed) ||
    parsed.length === 0 ||
    parsed.some((item) => typeof item !== "string" || item.length === 0)
  ) {
    throw new BackendProfileError(`${name} must be a non-empty string array`);
  }
  return parsed;
}

function timeoutMs(value: string | undefined): number {
  if (!value) {
    return 10_000;
  }
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    throw new BackendProfileError("COASONIX_REASONIX_TIMEOUT_MS must be a positive integer");
  }
  return parsed;
}
