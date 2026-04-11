// ── Actor System (0B) ───────────────────────────────────────────────────────
export {
  DEFAULT_CAPABILITY_SCOPE,
} from "./actor-types.js";

export type {
  ExecutionTarget,
  CapabilityScope,
  TaskStatus,
  ProcessTask,
  RuntimeResult,
  ActorRuntime,
  QueueEventType,
  QueueEvent,
  QueueListener,
  ProcessQueue,
  ProcessQueueOptions,
} from "./actor-types.js";

export {
  createProcessQueue,
  createLuauActorRuntime,
  createSidecarRuntime,
  createTestRuntime,
} from "./actor.js";

export type {
  LuauPayload,
  SidecarPayload,
  SidecarExecutor,
} from "./actor.js";

// ── Intelligence Layer (0C) ─────────────────────────────────────────────────
export type {
  AiRole,
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

export {
  createAiProviderRegistry,
  createOllamaProvider,
  createExternalProvider,
  createContextBuilder,
  createTestAiProvider,
} from "./ai.js";
