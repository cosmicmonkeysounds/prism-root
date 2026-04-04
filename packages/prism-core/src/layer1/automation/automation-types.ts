/**
 * Automation types — universal trigger/condition/action rules.
 *
 * An Automation is: "when X happens, if Y, do Z".
 *
 *   Trigger    → the event or schedule that starts the automation
 *   Conditions → optional guards (all must pass)
 *   Actions    → what to do (executed in sequence)
 *
 * Ported from @core/automation in legacy Helm. Adapted for Prism:
 *   - Removed Node-specific webhook/integration triggers (Prism uses Tauri IPC)
 *   - Kept object, cron, manual triggers
 *   - Kept core actions: create/update/delete object, delay, run-automation, notification
 *   - Removed HTTP webhook action (no raw HTTP in Prism)
 */

// ── Triggers ──────────────────────────────────────────────────────────────────

/** Object lifecycle trigger — fires when objects change. */
export interface ObjectTrigger {
  type: "object:created" | "object:updated" | "object:deleted";
  /** Filter to specific object types. Empty/absent = any type. */
  objectTypes?: string[] | undefined;
  /** Filter to objects with specific tags. All listed tags must be present. */
  tags?: string[] | undefined;
  /** Filter to objects whose field matches a value. */
  fieldMatch?: Record<string, unknown> | undefined;
}

/** Cron / scheduled trigger. */
export interface CronTrigger {
  type: "cron";
  /** Standard cron expression: '0 9 * * 1-5' (weekdays at 9am). */
  cron: string;
  /** IANA timezone. Default: 'UTC'. */
  timezone?: string | undefined;
}

/** Manual trigger — only runs when explicitly invoked. */
export interface ManualTrigger {
  type: "manual";
}

export type AutomationTrigger = ObjectTrigger | CronTrigger | ManualTrigger;

// ── Conditions ────────────────────────────────────────────────────────────────

/** Compare a field value (dot-path) to an expected value. */
export interface FieldCondition {
  type: "field";
  /** Dot-path into the trigger payload. E.g. 'object.status'. */
  path: string;
  operator:
    | "eq"
    | "neq"
    | "gt"
    | "gte"
    | "lt"
    | "lte"
    | "contains"
    | "startsWith"
    | "endsWith"
    | "matches";
  value: unknown;
}

/** Check the type of the triggering object. */
export interface TypeCondition {
  type: "type";
  objectType: string;
}

/** Check if the triggering object has listed tags. */
export interface TagCondition {
  type: "tags";
  tags: string[];
  mode: "all" | "any";
}

/** Logical combinators. */
export interface AndCondition {
  type: "and";
  conditions: AutomationCondition[];
}

export interface OrCondition {
  type: "or";
  conditions: AutomationCondition[];
}

export interface NotCondition {
  type: "not";
  condition: AutomationCondition;
}

export type AutomationCondition =
  | FieldCondition
  | TypeCondition
  | TagCondition
  | AndCondition
  | OrCondition
  | NotCondition;

// ── Actions ───────────────────────────────────────────────────────────────────

/** Create a new graph object. */
export interface CreateObjectAction {
  type: "object:create";
  objectType: string;
  /** Template for the new object. Supports {{field}} interpolation from context. */
  template: Record<string, unknown>;
  /** If set, parent the new object under the triggering object. */
  parentFromTrigger?: boolean | undefined;
}

/** Update fields on an existing object. */
export interface UpdateObjectAction {
  type: "object:update";
  /** 'trigger' = the object that triggered the automation, or an explicit id. */
  target: "trigger" | string;
  /** Fields to set. Supports {{field}} interpolation. */
  patch: Record<string, unknown>;
}

/** Delete an object. */
export interface DeleteObjectAction {
  type: "object:delete";
  target: "trigger" | string;
}

/** Send an in-app notification. */
export interface NotificationAction {
  type: "notification:send";
  /** 'trigger-owner' = owner of the triggering object, or explicit user id. */
  target: "trigger-owner" | string;
  title: string;
  body: string;
}

/** Delay subsequent actions. */
export interface DelayAction {
  type: "delay";
  /** Delay in seconds. */
  seconds: number;
}

/** Run another automation. */
export interface RunAutomationAction {
  type: "automation:run";
  automationId: string;
}

export type AutomationAction =
  | CreateObjectAction
  | UpdateObjectAction
  | DeleteObjectAction
  | NotificationAction
  | DelayAction
  | RunAutomationAction;

// ── Automation ────────────────────────────────────────────────────────────────

export interface Automation {
  id: string;
  name: string;
  description?: string | undefined;
  enabled: boolean;
  trigger: AutomationTrigger;
  /** All conditions must pass for actions to run. Empty = always run. */
  conditions: AutomationCondition[];
  actions: AutomationAction[];
  createdAt: string;
  updatedAt: string;
  /** ISO timestamp of last successful execution. */
  lastRunAt?: string | undefined;
  /** Total successful execution count. */
  runCount: number;
}

// ── Execution context ─────────────────────────────────────────────────────────

/** Runtime context available during condition evaluation and action interpolation. */
export interface AutomationContext {
  automationId: string;
  triggeredAt: string;
  triggerType: AutomationTrigger["type"];
  /** The object that triggered the automation (if object trigger). */
  object?: Record<string, unknown> | undefined;
  /** The previous state of the object (if object:updated). */
  previousObject?: Record<string, unknown> | undefined;
  /** Arbitrary extra context from the trigger source. */
  extra?: Record<string, unknown> | undefined;
}

// ── Execution result ──────────────────────────────────────────────────────────

export type AutomationRunStatus = "success" | "failed" | "skipped" | "partial";

export interface ActionResult {
  actionIndex: number;
  actionType: string;
  status: "success" | "failed" | "skipped";
  error?: string | undefined;
  elapsedMs?: number | undefined;
}

export interface AutomationRun {
  id: string;
  automationId: string;
  status: AutomationRunStatus;
  triggeredAt: string;
  completedAt?: string | undefined;
  conditionPassed: boolean;
  actionResults: ActionResult[];
  error?: string | undefined;
}

// ── Object event shape ────────────────────────────────────────────────────────

export interface ObjectEvent {
  type: "object:created" | "object:updated" | "object:deleted";
  object: Record<string, unknown>;
  previous?: Record<string, unknown> | undefined;
}

// ── Action handler interface ──────────────────────────────────────────────────

export type ActionHandlerFn = (
  action: AutomationAction,
  context: AutomationContext,
) => Promise<void>;

export type ActionHandlerMap = Partial<
  Record<AutomationAction["type"], ActionHandlerFn>
>;
