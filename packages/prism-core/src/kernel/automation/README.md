# automation

AutomationEngine: orchestrates trigger evaluation (object events / cron / manual) and dispatches actions through a handler map. Pure orchestration — action implementations are provided by the host app.

```ts
import { AutomationEngine } from "@prism/core/automation";
```

## Key exports

- `AutomationEngine` — class wrapping an `AutomationStore` with `handleObjectEvent`, `run`, cron scheduling, and per-run result tracking.
- `evaluateCondition(condition, ctx)` — pure boolean evaluation over `AutomationCondition` (field/type/tag/and/or/not).
- `matchesObjectTrigger(trigger, event)` — checks whether an `ObjectEvent` satisfies an `ObjectTrigger`.
- `interpolate(template, ctx)` — `{{path.to.field}}` template expansion over an `AutomationContext`.
- `compare(a, op, b)` / `getPath(obj, path)` — condition-evaluator primitives.
- Types: `Automation`, `AutomationTrigger` (`ObjectTrigger`/`CronTrigger`/`ManualTrigger`), `AutomationCondition`, `AutomationAction` (`CreateObjectAction`/`UpdateObjectAction`/`DeleteObjectAction`/`NotificationAction`/`DelayAction`/`RunAutomationAction`), `AutomationRun`, `ActionHandlerMap`, `ObjectEvent`, `AutomationStore`, `AutomationEngineOptions`.

## Usage

```ts
import { AutomationEngine } from "@prism/core/automation";

const handlers = {
  notification: async (_action, ctx) => {
    console.log("notify", ctx.object?.id);
  },
};
const engine = new AutomationEngine(store, handlers, {
  onRunComplete: (run) => console.log(run.status),
});
engine.start();
await engine.handleObjectEvent({ type: "object:created", object });
```
