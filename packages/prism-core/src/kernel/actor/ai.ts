/**
 * @prism/core — Intelligence Layer (Layer 0C)
 *
 * The Sovereign Mind. Pluggable AI providers with object-aware context.
 *
 * Features:
 *   - AiProviderRegistry — manage and switch between providers
 *   - createOllamaProvider — local Ollama inference via HTTP
 *   - createExternalProvider — API bridge for Claude, OpenAI, etc.
 *   - createContextBuilder — feed graph neighbors to prompts
 *   - Inline ghost-text completions for CodeMirror
 */

import type {
  AiMessage,
  AiCompletionRequest,
  AiCompletion,
  InlineCompletionRequest,
  InlineCompletion,
  ObjectContext,
  AiProvider,
  AiProviderRegistry,
  OllamaProviderOptions,
  ExternalProviderOptions,
  ContextBuilderOptions,
  AiHttpClient,
} from "./ai-types.js";

// ── AI Provider Registry ────────────────────────────────────────────────────

export function createAiProviderRegistry(): AiProviderRegistry {
  const providers = new Map<string, AiProvider>();
  let activeName: string | undefined;

  return {
    register(provider: AiProvider): void {
      providers.set(provider.name, provider);
      if (!activeName) activeName = provider.name;
    },

    get(name: string): AiProvider | undefined {
      return providers.get(name);
    },

    list(): string[] {
      return [...providers.keys()];
    },

    get active(): AiProvider | undefined {
      return activeName ? providers.get(activeName) : undefined;
    },

    setActive(name: string): void {
      if (!providers.has(name)) {
        throw new Error(`AI provider "${name}" not registered`);
      }
      activeName = name;
    },

    async complete(request: AiCompletionRequest): Promise<AiCompletion> {
      const provider = activeName ? providers.get(activeName) : undefined;
      if (!provider) throw new Error("No active AI provider");
      return provider.complete(request);
    },

    async completeInline(request: InlineCompletionRequest): Promise<InlineCompletion> {
      const provider = activeName ? providers.get(activeName) : undefined;
      if (!provider) throw new Error("No active AI provider");
      return provider.completeInline(request);
    },
  };
}

// ── Ollama Provider ─────────────────────────────────────────────────────────

export function createOllamaProvider(
  options: OllamaProviderOptions = {},
): AiProvider {
  const {
    baseUrl = "http://localhost:11434",
    defaultModel = "qwen3.5",
    httpClient,
  } = options;

  function getClient(): AiHttpClient {
    if (httpClient) return httpClient;
    throw new Error("Ollama provider requires an httpClient (no default fetch in Layer 1)");
  }

  return {
    name: "ollama",
    target: "local",
    defaultModel,

    async listModels(): Promise<string[]> {
      const client = getClient();
      const resp = await client.get(`${baseUrl}/api/tags`, {});
      if (resp.status !== 200) return [];
      const data = JSON.parse(resp.body) as { models?: Array<{ name: string }> };
      return (data.models ?? []).map(m => m.name);
    },

    async complete(request: AiCompletionRequest): Promise<AiCompletion> {
      const client = getClient();
      const model = request.model ?? defaultModel;
      const start = performance.now();

      const body = JSON.stringify({
        model,
        messages: request.messages.map(m => ({ role: m.role, content: m.content })),
        stream: false,
        options: {
          ...(request.temperature !== undefined ? { temperature: request.temperature } : {}),
          ...(request.maxTokens !== undefined ? { num_predict: request.maxTokens } : {}),
        },
        ...(request.stop ? { stop: request.stop } : {}),
      });

      const resp = await client.post(
        `${baseUrl}/api/chat`,
        body,
        { "Content-Type": "application/json" },
      );

      const durationMs = performance.now() - start;

      if (resp.status !== 200) {
        throw new Error(`Ollama request failed: ${resp.status} ${resp.body}`);
      }

      const data = JSON.parse(resp.body) as {
        message?: { content: string };
        prompt_eval_count?: number;
        eval_count?: number;
      };

      return {
        content: data.message?.content ?? "",
        model,
        usage: {
          promptTokens: data.prompt_eval_count ?? 0,
          completionTokens: data.eval_count ?? 0,
          totalTokens: (data.prompt_eval_count ?? 0) + (data.eval_count ?? 0),
        },
        durationMs,
      };
    },

    async completeInline(request: InlineCompletionRequest): Promise<InlineCompletion> {
      const client = getClient();
      const model = defaultModel;
      const start = performance.now();

      const prompt = request.suffix
        ? `${request.prefix}<FILL>${request.suffix}`
        : request.prefix;

      const body = JSON.stringify({
        model,
        prompt,
        stream: false,
        options: {
          ...(request.maxTokens !== undefined ? { num_predict: request.maxTokens } : {}),
        },
      });

      const resp = await client.post(
        `${baseUrl}/api/generate`,
        body,
        { "Content-Type": "application/json" },
      );

      const durationMs = performance.now() - start;

      if (resp.status !== 200) {
        throw new Error(`Ollama inline request failed: ${resp.status}`);
      }

      const data = JSON.parse(resp.body) as { response?: string };

      return {
        text: data.response ?? "",
        model,
        durationMs,
      };
    },

    async isAvailable(): Promise<boolean> {
      try {
        const client = getClient();
        const resp = await client.get(`${baseUrl}/api/tags`, {});
        return resp.status === 200;
      } catch {
        return false;
      }
    },
  };
}

// ── External Provider (Claude, OpenAI, etc.) ────────────────────────────────

export function createExternalProvider(
  options: ExternalProviderOptions,
): AiProvider {
  const { name, baseUrl, defaultModel, apiKey, httpClient } = options;

  function getClient(): AiHttpClient {
    if (httpClient) return httpClient;
    throw new Error(`External provider "${name}" requires an httpClient`);
  }

  function authHeaders(): Record<string, string> {
    return {
      "Content-Type": "application/json",
      "Authorization": `Bearer ${apiKey}`,
    };
  }

  return {
    name,
    target: "external",
    defaultModel,

    async listModels(): Promise<string[]> {
      // Most external APIs support GET /models
      try {
        const client = getClient();
        const resp = await client.get(`${baseUrl}/models`, authHeaders());
        if (resp.status !== 200) return [defaultModel];
        const data = JSON.parse(resp.body) as { data?: Array<{ id: string }> };
        return (data.data ?? []).map(m => m.id);
      } catch {
        return [defaultModel];
      }
    },

    async complete(request: AiCompletionRequest): Promise<AiCompletion> {
      const client = getClient();
      const model = request.model ?? defaultModel;
      const start = performance.now();

      const body = JSON.stringify({
        model,
        messages: request.messages.map(m => ({ role: m.role, content: m.content })),
        ...(request.maxTokens !== undefined ? { max_tokens: request.maxTokens } : {}),
        ...(request.temperature !== undefined ? { temperature: request.temperature } : {}),
        ...(request.stop ? { stop: request.stop } : {}),
      });

      const resp = await client.post(
        `${baseUrl}/chat/completions`,
        body,
        authHeaders(),
      );

      const durationMs = performance.now() - start;

      if (resp.status !== 200) {
        throw new Error(`${name} request failed: ${resp.status} ${resp.body}`);
      }

      const data = JSON.parse(resp.body) as {
        choices?: Array<{ message?: { content: string } }>;
        usage?: { prompt_tokens: number; completion_tokens: number; total_tokens: number };
      };

      return {
        content: data.choices?.[0]?.message?.content ?? "",
        model,
        usage: {
          promptTokens: data.usage?.prompt_tokens ?? 0,
          completionTokens: data.usage?.completion_tokens ?? 0,
          totalTokens: data.usage?.total_tokens ?? 0,
        },
        durationMs,
      };
    },

    async completeInline(request: InlineCompletionRequest): Promise<InlineCompletion> {
      // Use chat completion with a fill-in-the-middle prompt
      const completion = await this.complete({
        messages: [
          { role: "system", content: "Complete the code. Return ONLY the completion text, no explanation." },
          { role: "user", content: `${request.prefix}[CURSOR]${request.suffix ?? ""}` },
        ],
        model: defaultModel,
        maxTokens: request.maxTokens ?? 100,
        temperature: 0,
      });

      return {
        text: completion.content,
        model: completion.model,
        durationMs: completion.durationMs,
      };
    },

    async isAvailable(): Promise<boolean> {
      try {
        const client = getClient();
        const resp = await client.get(`${baseUrl}/models`, authHeaders());
        return resp.status === 200;
      } catch {
        return false;
      }
    },
  };
}

// ── Context Builder ─────────────────────────────────────────────────────────

/**
 * Build object-aware context from graph data for AI prompts.
 * Serializes the focal object and its graph neighborhood into
 * a structured ObjectContext that can be fed to any AiProvider.
 */
export function createContextBuilder(
  options: ContextBuilderOptions = {},
) {
  const {
    maxAncestorDepth = 5,
    maxChildren = 20,
    maxEdges = 20,
  } = options;

  return {
    /**
     * Build context from raw graph data.
     * This is a pure function — the caller provides the data.
     */
    build(params: {
      object: Record<string, unknown>;
      objectType: string;
      ancestors?: Array<{ id: string; type: string; name: string }>;
      children?: Array<{ id: string; type: string; name: string }>;
      edges?: Array<{ id: string; type: string; targetId: string; targetType: string }>;
      collection?: { id: string; name: string } | null;
    }): ObjectContext {
      return {
        object: params.object,
        objectType: params.objectType,
        ancestors: (params.ancestors ?? []).slice(0, maxAncestorDepth),
        children: (params.children ?? []).slice(0, maxChildren),
        edges: (params.edges ?? []).slice(0, maxEdges),
        collection: params.collection ?? null,
      };
    },

    /**
     * Format an ObjectContext into system message text for AI prompts.
     */
    toSystemMessage(ctx: ObjectContext): AiMessage {
      const parts: string[] = [
        `You are assisting with a "${ctx.objectType}" object.`,
      ];

      if (ctx.collection) {
        parts.push(`Collection: "${ctx.collection.name}" (${ctx.collection.id})`);
      }

      if (ctx.ancestors.length > 0) {
        parts.push(`Path: ${ctx.ancestors.map(a => a.name).join(" → ")}`);
      }

      if (ctx.children.length > 0) {
        parts.push(`Children (${ctx.children.length}): ${ctx.children.map(c => `${c.name} [${c.type}]`).join(", ")}`);
      }

      if (ctx.edges.length > 0) {
        parts.push(`Connections (${ctx.edges.length}): ${ctx.edges.map(e => `→ ${e.targetType}:${e.targetId}`).join(", ")}`);
      }

      parts.push(`\nObject data:\n${JSON.stringify(ctx.object, null, 2)}`);

      return { role: "system", content: parts.join("\n") };
    },
  };
}

// ── In-memory Test Provider ─────────────────────────────────────────────────

/**
 * Test AI provider that returns canned responses.
 */
export function createTestAiProvider(
  name: string,
  responses: { complete?: string; inline?: string } = {},
): AiProvider {
  const completeText = responses.complete ?? "test response";
  const inlineText = responses.inline ?? "test completion";

  return {
    name,
    target: "local",
    defaultModel: "test-model",

    async listModels() { return ["test-model"]; },

    async complete(request: AiCompletionRequest): Promise<AiCompletion> {
      return {
        content: completeText,
        model: request.model ?? "test-model",
        usage: { promptTokens: 10, completionTokens: 5, totalTokens: 15 },
        durationMs: 1,
      };
    },

    async completeInline(_request: InlineCompletionRequest): Promise<InlineCompletion> {
      return { text: inlineText, model: "test-model", durationMs: 1 };
    },

    async isAvailable() { return true; },
  };
}
