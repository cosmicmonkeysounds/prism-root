/**
 * @prism/core — Intelligence Layer Types (Layer 0C)
 *
 * The Sovereign Mind. Users toggle between:
 *   - Sovereign Local (Ollama) — zero latency, private
 *   - Federated Delegate — home-server GPU via E2EE Relay
 *   - External API (Claude, Gemini, etc.) — Capability Token wrapped
 *
 * No data is sent to any AI provider unless the Manifest opts in
 * and the user signs a Capability Token.
 */

import type { ExecutionTarget } from "./actor-types.js";

// ── Messages ────────────────────────────────────────────────────────────────

export type AiRole = "system" | "user" | "assistant";

export interface AiMessage {
  role: AiRole;
  content: string;
}

// ── Completion ──────────────────────────────────────────────────────────────

export interface AiCompletionRequest {
  /** Messages for the conversation. */
  messages: AiMessage[];
  /** Model identifier (e.g. "qwen3.5", "claude-sonnet-4-20250514"). */
  model?: string;
  /** Maximum tokens to generate. */
  maxTokens?: number;
  /** Temperature (0-2). */
  temperature?: number;
  /** Stop sequences. */
  stop?: string[];
}

export interface AiCompletion {
  /** Generated text. */
  content: string;
  /** Model used. */
  model: string;
  /** Tokens used (prompt + completion). */
  usage: {
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
  };
  /** Duration in milliseconds. */
  durationMs: number;
}

// ── Inline Completions (Ghost-Text) ─────────────────────────────────────────

export interface InlineCompletionRequest {
  /** Text before the cursor. */
  prefix: string;
  /** Text after the cursor. */
  suffix: string;
  /** Language/file type hint. */
  language?: string;
  /** Maximum tokens to generate. */
  maxTokens?: number;
}

export interface InlineCompletion {
  /** The suggested insertion text. */
  text: string;
  /** Model used. */
  model: string;
  /** Duration in milliseconds. */
  durationMs: number;
}

// ── Object-Aware Context ────────────────────────────────────────────────────

/**
 * Context built from graph neighbors and collection state.
 * Fed to AI providers for object-aware reasoning.
 */
export interface ObjectContext {
  /** The focal object's serialized data. */
  object: Record<string, unknown>;
  /** Object type name. */
  objectType: string;
  /** Parent chain (root → parent → self). */
  ancestors: Array<{ id: string; type: string; name: string }>;
  /** Direct children summaries. */
  children: Array<{ id: string; type: string; name: string }>;
  /** Connected edges. */
  edges: Array<{ id: string; type: string; targetId: string; targetType: string }>;
  /** Collection-level metadata. */
  collection: { id: string; name: string } | null;
}

// ── AI Provider ─────────────────────────────────────────────────────────────

export interface AiProvider {
  /** Provider identifier (e.g. "ollama", "claude", "openai"). */
  readonly name: string;
  /** Execution target. */
  readonly target: ExecutionTarget;
  /** Default model for this provider. */
  readonly defaultModel: string;
  /** List available models. */
  listModels(): Promise<string[]>;
  /** Generate a completion. */
  complete(request: AiCompletionRequest): Promise<AiCompletion>;
  /** Generate an inline completion (ghost-text). */
  completeInline(request: InlineCompletionRequest): Promise<InlineCompletion>;
  /** Check if the provider is reachable. */
  isAvailable(): Promise<boolean>;
}

// ── AI Provider Registry ────────────────────────────────────────────────────

export interface AiProviderRegistry {
  /** Register a provider. */
  register(provider: AiProvider): void;
  /** Get a provider by name. */
  get(name: string): AiProvider | undefined;
  /** List all provider names. */
  list(): string[];
  /** Get the active (default) provider. */
  readonly active: AiProvider | undefined;
  /** Set the active provider by name. */
  setActive(name: string): void;
  /** Complete using the active provider. */
  complete(request: AiCompletionRequest): Promise<AiCompletion>;
  /** Inline complete using the active provider. */
  completeInline(request: InlineCompletionRequest): Promise<InlineCompletion>;
}

// ── Context Builder ─────────────────────────────────────────────────────────

export interface ContextBuilderOptions {
  /** Maximum ancestor depth. Default: 5. */
  maxAncestorDepth?: number;
  /** Maximum children to include. Default: 20. */
  maxChildren?: number;
  /** Maximum edges to include. Default: 20. */
  maxEdges?: number;
}

// ── Provider Options ────────────────────────────────────────────────────────

export interface OllamaProviderOptions {
  /** Ollama HTTP base URL. Default: "http://localhost:11434". */
  baseUrl?: string;
  /** Default model. Default: "qwen3.5". */
  defaultModel?: string;
  /** HTTP client for testing. */
  httpClient?: AiHttpClient;
}

export interface ExternalProviderOptions {
  /** Provider name (e.g. "claude", "openai"). */
  name: string;
  /** API base URL. */
  baseUrl: string;
  /** Default model. */
  defaultModel: string;
  /** API key or bearer token. */
  apiKey: string;
  /** HTTP client for testing. */
  httpClient?: AiHttpClient;
}

/** Minimal HTTP client interface for AI provider requests. */
export interface AiHttpClient {
  post(
    url: string,
    body: string,
    headers: Record<string, string>,
  ): Promise<{ status: number; body: string }>;
  get(
    url: string,
    headers: Record<string, string>,
  ): Promise<{ status: number; body: string }>;
}
