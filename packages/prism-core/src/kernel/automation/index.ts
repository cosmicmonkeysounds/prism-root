export type {
  ObjectTrigger,
  CronTrigger,
  ManualTrigger,
  AutomationTrigger,
  FieldCondition,
  TypeCondition,
  TagCondition,
  AndCondition,
  OrCondition,
  NotCondition,
  AutomationCondition,
  CreateObjectAction,
  UpdateObjectAction,
  DeleteObjectAction,
  NotificationAction,
  DelayAction,
  RunAutomationAction,
  AutomationAction,
  Automation,
  AutomationContext,
  AutomationRunStatus,
  ActionResult,
  AutomationRun,
  ObjectEvent,
  ActionHandlerFn,
  ActionHandlerMap,
} from "./automation-types.js";

export {
  evaluateCondition,
  compare,
  getPath,
  interpolate,
  matchesObjectTrigger,
} from "./condition-evaluator.js";

export { AutomationEngine } from "./automation-engine.js";

export type {
  AutomationStore,
  AutomationEngineOptions,
} from "./automation-engine.js";
