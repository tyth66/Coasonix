import { describe, test, expect } from "bun:test";
import { ACPClient } from "./ACPClient";

// ------ Mock ACP server spawned as a Bun subprocess ------

function acpEchoServer(): string[] {
  // A minimal Node.js script that speaks ACP NDJSON well enough for our tests.
  // It accepts initialize + session/new + session/prompt, and streams back
  // agent_message_chunk notifications before responding to session/prompt.
  return [
    "node",
    "-e",
    `
    const rl = require("readline").createInterface({ input: process.stdin });
    let seq = 0;
    let sessionId = null;
    rl.on("line", (line) => {
      if (!line.trim()) return;
      const req = JSON.parse(line);
      seq++;

      if (req.method === "initialize") {
        // Respond to request
        process.stdout.write(JSON.stringify({
          jsonrpc: "2.0", id: req.id,
          result: {
            protocolVersion: 1,
            agentCapabilities: {
              loadSession: false,
              promptCapabilities: { image: false, audio: false, embeddedContext: true },
              mcpCapabilities: { http: false, sse: false },
            },
            agentInfo: { name: "test-agent", version: "0.0.0" },
          }
        }) + "\\n");
      } else if (req.method === "session/new") {
        sessionId = "sess-" + seq;
        process.stdout.write(JSON.stringify({
          jsonrpc: "2.0", id: req.id,
          result: { sessionId }
        }) + "\\n");
      } else if (req.method === "session/prompt") {
        // Send notifications before responding
        process.stdout.write(JSON.stringify({
          jsonrpc: "2.0",
          method: "session/update",
          params: {
            sessionId,
            update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Hello " } }
          }
        }) + "\\n");
        process.stdout.write(JSON.stringify({
          jsonrpc: "2.0",
          method: "session/update",
          params: {
            sessionId,
            update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "World" } }
          }
        }) + "\\n");
        // Respond
        process.stdout.write(JSON.stringify({
          jsonrpc: "2.0", id: req.id,
          result: { stopReason: "end_turn" }
        }) + "\\n");
      } else {
        process.stdout.write(JSON.stringify({
          jsonrpc: "2.0", id: req.id ?? null,
          error: { code: -32601, message: "method not found" }
        }) + "\\n");
      }
    });
    `,
  ];
}

// ------ Tests ------

describe("ACPClient", () => {
  test("initialize and session/new handshake", async () => {
    const client = new ACPClient({
      command: acpEchoServer(),
      cwd: process.cwd(), 
      requestTimeoutMs: 3000,
    });

    try {
      await client.start();
      const initResult = (await client.call("initialize", {
        protocolVersion: 1,
        clientInfo: { name: "test", version: "1.0.0" },
      })) as { protocolVersion: number };

      expect(initResult.protocolVersion).toBe(1);

      const sessionResult = (await client.call("session/new", {
        cwd: "/tmp",
      })) as { sessionId: string };

      expect(sessionResult.sessionId).toMatch(/^sess-/);
    } finally {
      await client.shutdown();
    }
  });

  test("session/prompt collects agent_message_chunk notifications", async () => {
    const client = new ACPClient({
      command: acpEchoServer(),
      cwd: process.cwd(),
      requestTimeoutMs: 3000,
    });

    const chunks: string[] = [];
    client.onNotification((method, params) => {
      if (method === "session/update") {
        const p = params as {
          update: { sessionUpdate: string; content: { text: string } };
        };
        if (
          p.update?.sessionUpdate === "agent_message_chunk" &&
          p.update.content?.text
        ) {
          chunks.push(p.update.content.text);
        }
      }
    });

    try {
      await client.start();
      await client.call("initialize", {
        protocolVersion: 1,
        clientInfo: { name: "test", version: "1.0.0" },
      });
      const sessionResult = (await client.call("session/new", {
        cwd: "/tmp",
      })) as { sessionId: string };

      const promptResult = await client.call("session/prompt", {
        sessionId: sessionResult.sessionId,
        prompt: [{ type: "text", text: "review this" }],
      });

      expect(chunks).toEqual(["Hello ", "World"]);
      expect((promptResult as { stopReason: string }).stopReason).toBe(
        "end_turn",
      );
    } finally {
      await client.shutdown();
    }
  });

  test("request timeout rejects and stops process", async () => {
    // Server that never responds to session/prompt
    const slowCommand = [
      "node",
      "-e",
      `
      const rl = require("readline").createInterface({ input: process.stdin });
      rl.on("line", (line) => {
        const req = JSON.parse(line);
        if (req.method === "initialize") {
          process.stdout.write(JSON.stringify({
            jsonrpc: "2.0", id: req.id,
            result: { protocolVersion: 1,
              agentCapabilities: {
                loadSession: false,
                promptCapabilities: { image: false, audio: false, embeddedContext: true },
                mcpCapabilities: { http: false, sse: false },
              },
              agentInfo: { name: "test", version: "0.0.0" },
            }
          }) + "\\n");
        } else if (req.method === "session/new") {
          process.stdout.write(JSON.stringify({
            jsonrpc: "2.0", id: req.id,
            result: { sessionId: "sess-timeout" }
          }) + "\\n");
        }
        // session/prompt: intentionally no response
      });
      // Keep process alive long enough for the timeout to fire
      setTimeout(() => {}, 10000);
      `,
    ];

    const client = new ACPClient({
      command: slowCommand,
      cwd: process.cwd(),
      requestTimeoutMs: 200,
    });

    try {
      await client.start();
      await client.call("initialize", {
        protocolVersion: 1,
        clientInfo: { name: "test", version: "1.0.0" },
      });
      await client.call("session/new", { cwd: "/tmp" });

      await expect(
        client.call("session/prompt", {
          sessionId: "sess-timeout",
          prompt: [{ type: "text", text: "test" }],
        }),
      ).rejects.toThrow("timed out");
    } finally {
      await client.shutdown();
    }
  });

  test("shutdown sets stopping flag and drains", async () => {
    const client = new ACPClient({
      command: acpEchoServer(),
      cwd: process.cwd(),
      requestTimeoutMs: 3000,
    });

    await client.start();
    await client.call("initialize", {
      protocolVersion: 1,
      clientInfo: { name: "test", version: "1.0.0" },
    });
    await client.call("session/new", { cwd: "/tmp" });
    await client.call("session/prompt", {
      sessionId: "sess-1",
      prompt: [{ type: "text", text: "test" }],
    });

    // Should not throw
    await client.shutdown();
  });
});

