# actor

Actor System and Intelligence Layer: an ordered, priority-aware process queue for automation/script tasks backed by pluggable runtimes (Luau, sidecar, test), plus a multi-provider AI registry with object-aware context building.

```ts
import { createProcessQueue } from "@prism/core/actor";
```

## Key exports

- `createProcessQueue(options?)` — build a `ProcessQueue` with `enqueue`/`cancel`/`prune`/`start`/events, priority ordering and configurable concurrency.
- `createLuauActorRuntime()` / `createSidecarRuntime(executor)` / `createTestRuntime()` — pluggable `ActorRuntime` implementations registered on a queue.
- `DEFAULT_CAPABILITY_SCOPE`, `CapabilityScope` — zero-trust sandbox profile applied per task.
- Types: `ProcessTask`, `TaskStatus`, `RuntimeResult`, `QueueEvent`, `ExecutionTarget`, `LuauPayload`, `SidecarPayload`, `SidecarExecutor`.
- `createAiProviderRegistry()` — multi-provider AI registry (completion + inline completion).
- `createOllamaProvider(options)` / `createExternalProvider(options)` / `createTestAiProvider()` — built-in providers, pluggable via the `AiHttpClient` interface.
- `createContextBuilder(options)` — assembles `ObjectContext` from the graph for LLM prompts.
- Types: `AiProvider`, `AiCompletionRequest`, `AiCompletion`, `InlineCompletionRequest`, `ObjectContext`, `AiHttpClient`.

## Usage

```ts
import {
  createProcessQueue,
  createLuauActorRuntime,
} from "@prism/core/actor";

const queue = createProcessQueue({ concurrency: 2 });
queue.registerRuntime(createLuauActorRuntime());
queue.enqueue({
  name: "calc",
  runtime: "luau",
  payload: { script: "return 1 + 1" },
});
queue.start();
```
