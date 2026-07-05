import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const repoRoot = resolve(import.meta.dir, "../../../..");

describe("MCP server operational contract", () => {
  test("package exposes a stable MCP server start script", () => {
    const packageJson = JSON.parse(
      readFileSync(resolve(repoRoot, "packages/reasonix-expert-mcp/package.json"), "utf8"),
    );

    expect(packageJson.scripts?.["start:mcp"]).toBe("bun src/index.ts");
  });

  test("top-level README documents the minimum MCP server environment", () => {
    const readme = readFileSync(resolve(repoRoot, "README.md"), "utf8");

    expect(readme).toContain("bun run --silent --cwd=packages/reasonix-expert-mcp start:mcp");
    expect(readme).toContain("COASONIX_REPO_ROOT");
    expect(readme).toContain("COASONIX_RUNTIME_WORKER");
    expect(readme).toContain("COASONIX_AGENT_COMMAND_JSON");
  });
});


