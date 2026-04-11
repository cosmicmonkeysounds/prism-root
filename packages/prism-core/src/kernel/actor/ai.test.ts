import { describe, it, expect } from "vitest";
import {
  createAiProviderRegistry,
  createOllamaProvider,
  createExternalProvider,
  createContextBuilder,
  createTestAiProvider,
} from "./ai.js";
import type { AiHttpClient } from "./ai-types.js";

// ── AiProviderRegistry ──────────────────────────────────────────────────────

describe("AiProviderRegistry", () => {
  it("registers and retrieves providers", () => {
    const registry = createAiProviderRegistry();
    const p1 = createTestAiProvider("test-a");
    const p2 = createTestAiProvider("test-b");

    registry.register(p1);
    registry.register(p2);

    expect(registry.list()).toEqual(["test-a", "test-b"]);
    expect(registry.get("test-a")).toBeDefined();
    expect(registry.get("test-a")?.name).toBe("test-a");
  });

  it("first registered provider becomes active", () => {
    const registry = createAiProviderRegistry();
    registry.register(createTestAiProvider("first"));
    registry.register(createTestAiProvider("second"));

    expect(registry.active?.name).toBe("first");
  });

  it("setActive switches the provider", () => {
    const registry = createAiProviderRegistry();
    registry.register(createTestAiProvider("a"));
    registry.register(createTestAiProvider("b"));

    registry.setActive("b");
    expect(registry.active?.name).toBe("b");
  });

  it("setActive throws for unknown provider", () => {
    const registry = createAiProviderRegistry();
    expect(() => registry.setActive("nope")).toThrow("not registered");
  });

  it("complete delegates to active provider", async () => {
    const registry = createAiProviderRegistry();
    registry.register(createTestAiProvider("test", { complete: "hello world" }));

    const result = await registry.complete({
      messages: [{ role: "user", content: "hi" }],
    });
    expect(result.content).toBe("hello world");
  });

  it("completeInline delegates to active provider", async () => {
    const registry = createAiProviderRegistry();
    registry.register(createTestAiProvider("test", { inline: "completed code" }));

    const result = await registry.completeInline({ prefix: "const x = " });
    expect(result.text).toBe("completed code");
  });

  it("throws when no active provider for complete", async () => {
    const registry = createAiProviderRegistry();
    await expect(
      registry.complete({ messages: [{ role: "user", content: "hi" }] }),
    ).rejects.toThrow("No active AI provider");
  });
});

// ── Ollama Provider ─────────────────────────────────────────────────────────

describe("OllamaProvider", () => {
  function mockOllamaClient(): AiHttpClient {
    return {
      async post(url: string, _body: string) {
        if (url.includes("/api/chat")) {
          return {
            status: 200,
            body: JSON.stringify({
              message: { content: "ollama response" },
              prompt_eval_count: 10,
              eval_count: 20,
            }),
          };
        }
        if (url.includes("/api/generate")) {
          return {
            status: 200,
            body: JSON.stringify({ response: "inline completion" }),
          };
        }
        return { status: 404, body: "" };
      },
      async get(url: string) {
        if (url.includes("/api/tags")) {
          return {
            status: 200,
            body: JSON.stringify({ models: [{ name: "qwen3.5" }, { name: "llama3" }] }),
          };
        }
        return { status: 404, body: "" };
      },
    };
  }

  it("completes a chat request", async () => {
    const provider = createOllamaProvider({ httpClient: mockOllamaClient() });
    const result = await provider.complete({
      messages: [{ role: "user", content: "hello" }],
    });

    expect(result.content).toBe("ollama response");
    expect(result.model).toBe("qwen3.5");
    expect(result.usage.promptTokens).toBe(10);
    expect(result.usage.completionTokens).toBe(20);
    expect(result.durationMs).toBeGreaterThanOrEqual(0);
  });

  it("completes an inline request", async () => {
    const provider = createOllamaProvider({ httpClient: mockOllamaClient() });
    const result = await provider.completeInline({
      prefix: "function add(",
      suffix: ") { }",
    });

    expect(result.text).toBe("inline completion");
    expect(result.model).toBe("qwen3.5");
  });

  it("lists models", async () => {
    const provider = createOllamaProvider({ httpClient: mockOllamaClient() });
    const models = await provider.listModels();
    expect(models).toEqual(["qwen3.5", "llama3"]);
  });

  it("checks availability", async () => {
    const provider = createOllamaProvider({ httpClient: mockOllamaClient() });
    expect(await provider.isAvailable()).toBe(true);
  });

  it("handles unavailable server", async () => {
    const failClient: AiHttpClient = {
      async post() { throw new Error("connection refused"); },
      async get() { throw new Error("connection refused"); },
    };
    const provider = createOllamaProvider({ httpClient: failClient });
    expect(await provider.isAvailable()).toBe(false);
  });

  it("uses custom model and baseUrl", async () => {
    const posts: string[] = [];
    const client: AiHttpClient = {
      async post(url: string, _body: string) {
        posts.push(url);
        return { status: 200, body: JSON.stringify({ message: { content: "ok" } }) };
      },
      async get() { return { status: 200, body: JSON.stringify({ models: [] }) }; },
    };

    const provider = createOllamaProvider({
      baseUrl: "http://gpu-server:11434",
      defaultModel: "llama3",
      httpClient: client,
    });

    await provider.complete({
      messages: [{ role: "user", content: "test" }],
    });

    expect(posts[0]).toContain("gpu-server:11434");
    expect(provider.defaultModel).toBe("llama3");
  });

  it("name and target are correct", () => {
    const provider = createOllamaProvider({ httpClient: mockOllamaClient() });
    expect(provider.name).toBe("ollama");
    expect(provider.target).toBe("local");
  });
});

// ── External Provider ───────────────────────────────────────────────────────

describe("ExternalProvider", () => {
  function mockExternalClient(): AiHttpClient {
    return {
      async post(url: string, _body: string, _headers: Record<string, string>) {
        if (url.includes("/chat/completions")) {
          return {
            status: 200,
            body: JSON.stringify({
              choices: [{ message: { content: "external response" } }],
              usage: { prompt_tokens: 5, completion_tokens: 10, total_tokens: 15 },
            }),
          };
        }
        return { status: 404, body: "" };
      },
      async get(url: string, headers: Record<string, string>) {
        if (url.includes("/models")) {
          expect(headers["Authorization"]).toBe("Bearer test-key");
          return {
            status: 200,
            body: JSON.stringify({ data: [{ id: "claude-sonnet-4-20250514" }] }),
          };
        }
        return { status: 404, body: "" };
      },
    };
  }

  it("completes a chat request", async () => {
    const provider = createExternalProvider({
      name: "claude",
      baseUrl: "https://api.anthropic.com/v1",
      defaultModel: "claude-sonnet-4-20250514",
      apiKey: "test-key",
      httpClient: mockExternalClient(),
    });

    const result = await provider.complete({
      messages: [{ role: "user", content: "hello" }],
    });

    expect(result.content).toBe("external response");
    expect(result.usage.totalTokens).toBe(15);
  });

  it("lists models with auth header", async () => {
    const provider = createExternalProvider({
      name: "claude",
      baseUrl: "https://api.anthropic.com/v1",
      defaultModel: "claude-sonnet-4-20250514",
      apiKey: "test-key",
      httpClient: mockExternalClient(),
    });

    const models = await provider.listModels();
    expect(models).toContain("claude-sonnet-4-20250514");
  });

  it("name and target are correct", () => {
    const provider = createExternalProvider({
      name: "openai",
      baseUrl: "https://api.openai.com/v1",
      defaultModel: "gpt-4",
      apiKey: "sk-xxx",
      httpClient: mockExternalClient(),
    });

    expect(provider.name).toBe("openai");
    expect(provider.target).toBe("external");
  });

  it("inline completion uses chat endpoint", async () => {
    const provider = createExternalProvider({
      name: "claude",
      baseUrl: "https://api.anthropic.com/v1",
      defaultModel: "claude-sonnet-4-20250514",
      apiKey: "test-key",
      httpClient: mockExternalClient(),
    });

    const result = await provider.completeInline({
      prefix: "function hello(",
      suffix: ") {}",
    });

    expect(result.text).toBe("external response");
  });
});

// ── Context Builder ─────────────────────────────────────────────────────────

describe("ContextBuilder", () => {
  it("builds context from graph data", () => {
    const builder = createContextBuilder();
    const ctx = builder.build({
      object: { name: "My Task", status: "active" },
      objectType: "Task",
      ancestors: [
        { id: "root", type: "Project", name: "Prism" },
        { id: "sprint", type: "Sprint", name: "Sprint 5" },
      ],
      children: [
        { id: "sub1", type: "Subtask", name: "Design" },
      ],
      edges: [
        { id: "e1", type: "assigned-to", targetId: "user1", targetType: "User" },
      ],
      collection: { id: "coll-tasks", name: "Tasks" },
    });

    expect(ctx.objectType).toBe("Task");
    expect(ctx.ancestors).toHaveLength(2);
    expect(ctx.children).toHaveLength(1);
    expect(ctx.edges).toHaveLength(1);
    expect(ctx.collection?.name).toBe("Tasks");
  });

  it("respects max limits", () => {
    const builder = createContextBuilder({
      maxAncestorDepth: 1,
      maxChildren: 2,
      maxEdges: 1,
    });

    const ctx = builder.build({
      object: {},
      objectType: "Test",
      ancestors: [
        { id: "a1", type: "A", name: "A1" },
        { id: "a2", type: "A", name: "A2" },
        { id: "a3", type: "A", name: "A3" },
      ],
      children: [
        { id: "c1", type: "C", name: "C1" },
        { id: "c2", type: "C", name: "C2" },
        { id: "c3", type: "C", name: "C3" },
      ],
      edges: [
        { id: "e1", type: "E", targetId: "t1", targetType: "T" },
        { id: "e2", type: "E", targetId: "t2", targetType: "T" },
      ],
    });

    expect(ctx.ancestors).toHaveLength(1);
    expect(ctx.children).toHaveLength(2);
    expect(ctx.edges).toHaveLength(1);
  });

  it("handles missing optional fields", () => {
    const builder = createContextBuilder();
    const ctx = builder.build({
      object: { x: 1 },
      objectType: "Node",
    });

    expect(ctx.ancestors).toEqual([]);
    expect(ctx.children).toEqual([]);
    expect(ctx.edges).toEqual([]);
    expect(ctx.collection).toBeNull();
  });

  it("formats context as system message", () => {
    const builder = createContextBuilder();
    const ctx = builder.build({
      object: { name: "Test" },
      objectType: "Task",
      ancestors: [{ id: "p1", type: "Project", name: "Prism" }],
      children: [{ id: "s1", type: "Subtask", name: "Sub" }],
      edges: [{ id: "e1", type: "ref", targetId: "doc1", targetType: "Doc" }],
      collection: { id: "coll-1", name: "Work" },
    });

    const msg = builder.toSystemMessage(ctx);
    expect(msg.role).toBe("system");
    expect(msg.content).toContain("Task");
    expect(msg.content).toContain("Work");
    expect(msg.content).toContain("Prism");
    expect(msg.content).toContain("Sub [Subtask]");
    expect(msg.content).toContain("Doc:doc1");
    expect(msg.content).toContain('"name": "Test"');
  });
});

// ── TestAiProvider ──────────────────────────────────────────────────────────

describe("TestAiProvider", () => {
  it("returns canned responses", async () => {
    const provider = createTestAiProvider("mock", {
      complete: "mocked answer",
      inline: "mocked completion",
    });

    const chat = await provider.complete({
      messages: [{ role: "user", content: "hi" }],
    });
    expect(chat.content).toBe("mocked answer");

    const inline = await provider.completeInline({ prefix: "x" });
    expect(inline.text).toBe("mocked completion");
  });

  it("is always available", async () => {
    const provider = createTestAiProvider("mock");
    expect(await provider.isAvailable()).toBe(true);
  });

  it("lists test model", async () => {
    const provider = createTestAiProvider("mock");
    expect(await provider.listModels()).toEqual(["test-model"]);
  });
});
