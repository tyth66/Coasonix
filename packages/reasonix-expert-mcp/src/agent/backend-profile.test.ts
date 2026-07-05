import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";

import { ERROR_CODES } from "./error-taxonomy";
import {
  resolveBackendProfile,
  type BackendProfile,
  type BackendProfileEnvironment,
} from "./backend-profile";

const repoRoot = resolve(import.meta.dir, "../../../..");

describe("backend profiles", () => {
  test.each([
    ["mock", "coasonix-mock-worker"],
    ["conformance", "coasonix-mock-worker"],
  ] satisfies Array<[BackendProfile, string]>)("%s selects a repo-local worker command", (profile, executable) => {
    const resolved = resolveBackendProfile({ profile, repoRoot, env: {} });

    expect(resolved.command[0]).toContain(executable);
    expect(resolved.command[1]).toBe("review-diff");
    expect(resolved.timeoutMs).toBe(10_000);
  });

  test.each([
    ["reasonix-cli", "COASONIX_REASONIX_CLI_COMMAND_JSON"],
    ["mimocode-cli", "COASONIX_MIMOCODE_CLI_COMMAND_JSON"],
  ] satisfies Array<[BackendProfile, keyof BackendProfileEnvironment]>)(
    "%s requires explicit command JSON",
    (profile) => {
      expect(() => resolveBackendProfile({ profile, repoRoot, env: {} })).toThrow(ERROR_CODES.BACKEND_NOT_CONFIGURED);
    },
  );

  test("reasonix-cli uses only the explicitly configured command and timeout", () => {
    const resolved = resolveBackendProfile({
      profile: "reasonix-cli",
      repoRoot,
      env: {
        COASONIX_REASONIX_CLI_COMMAND_JSON: '["reasonix-cli","review-diff"]',
        COASONIX_REASONIX_TIMEOUT_MS: "15000",
      },
    });

    expect(resolved.command).toEqual(["reasonix-cli", "review-diff"]);
    expect(resolved.timeoutMs).toBe(15_000);
  });

  test("mimocode-cli can use a backend-specific timeout", () => {
    const resolved = resolveBackendProfile({
      profile: "mimocode-cli",
      repoRoot,
      env: {
        COASONIX_MIMOCODE_CLI_COMMAND_JSON: '["mimocode-cli","review-diff"]',
        COASONIX_REASONIX_TIMEOUT_MS: "15000",
        COASONIX_MIMOCODE_TIMEOUT_MS: "20000",
      },
    });

    expect(resolved.command).toEqual(["mimocode-cli", "review-diff"]);
    expect(resolved.timeoutMs).toBe(20_000);
  });
});
