/**
 * ScriptSteps — FileMaker-style visual scripting for non-programmers.
 *
 * Each ScriptStep is a structured action that maps to Luau code.
 * Steps are arranged in a flat list with control flow (If/Loop/End).
 * Non-programmers configure steps via dropdowns and text fields.
 * The system emits valid Luau that runs in the Prism Luau runtime.
 *
 * Step categories:
 *   Navigation: Go To Layout, Go To Record
 *   Data: Set Field, Set Variable, New Record, Delete Record, Commit Record
 *   Control: If / Else If / Else / End If, Loop / Exit Loop / End Loop
 *   UI: Show Dialog, Show Notification, Refresh Window
 *   Find: Perform Find, Show All Records, Sort Records, Constrain Found Set
 *   Script: Run Script, Exit Script, Halt Script
 *
 * Usage:
 *   const steps = [
 *     createStep('set-field', { target: 'status', value: '"active"' }),
 *     createStep('if', { condition: '[field:amount] > 1000' }),
 *     createStep('show-notification', { title: '"High value!"', kind: '"warning"' }),
 *     createStep('end-if'),
 *     createStep('commit-record'),
 *   ];
 *   const lua = emitStepsLuau(steps);
 */

// ── Step Kind Registry ──────────────────────────────────────────────────────

export type ScriptStepKind =
  // Navigation
  | "go-to-layout"
  | "go-to-record"
  | "go-to-related"
  // Data
  | "set-field"
  | "set-variable"
  | "new-record"
  | "duplicate-record"
  | "delete-record"
  | "commit-record"
  | "revert-record"
  // Control flow
  | "if"
  | "else-if"
  | "else"
  | "end-if"
  | "loop"
  | "exit-loop-if"
  | "end-loop"
  // UI
  | "show-dialog"
  | "show-notification"
  | "refresh-window"
  | "freeze-window"
  | "close-window"
  // Find/Sort
  | "perform-find"
  | "show-all-records"
  | "sort-records"
  | "constrain-found-set"
  | "extend-found-set"
  // Script
  | "run-script"
  | "exit-script"
  | "halt-script"
  | "comment"
  // Custom
  | "custom-luau";

// ── Step Metadata ───────────────────────────────────────────────────────────

export interface StepKindMeta {
  kind: ScriptStepKind;
  label: string;
  category: "navigation" | "data" | "control" | "ui" | "find" | "script" | "custom";
  /** Parameter names this step accepts. */
  params: string[];
  /** Human-readable description. */
  description: string;
  /** Whether this step increases indent (if, loop). */
  opensBlock?: boolean;
  /** Whether this step decreases indent (end-if, end-loop, else). */
  closesBlock?: boolean;
  /** Whether this step is both (else-if: closes then opens). */
  continuesBlock?: boolean;
}

export const STEP_KINDS: StepKindMeta[] = [
  // Navigation
  { kind: "go-to-layout", label: "Go to Layout", category: "navigation", params: ["layoutId"], description: "Switch to a different layout/view" },
  { kind: "go-to-record", label: "Go to Record", category: "navigation", params: ["position"], description: "Navigate to first, last, next, previous, or specific record" },
  { kind: "go-to-related", label: "Go to Related", category: "navigation", params: ["relationshipId", "layoutId"], description: "Navigate to related records via a relationship" },

  // Data
  { kind: "set-field", label: "Set Field", category: "data", params: ["target", "value"], description: "Set a field value on the current record" },
  { kind: "set-variable", label: "Set Variable", category: "data", params: ["name", "value"], description: "Set a local or global variable" },
  { kind: "new-record", label: "New Record", category: "data", params: ["objectType"], description: "Create a new empty record" },
  { kind: "duplicate-record", label: "Duplicate Record", category: "data", params: [], description: "Duplicate the current record" },
  { kind: "delete-record", label: "Delete Record", category: "data", params: [], description: "Delete the current record" },
  { kind: "commit-record", label: "Commit Record", category: "data", params: [], description: "Save pending changes to the current record" },
  { kind: "revert-record", label: "Revert Record", category: "data", params: [], description: "Discard pending changes to the current record" },

  // Control flow
  { kind: "if", label: "If", category: "control", params: ["condition"], description: "Start a conditional block", opensBlock: true },
  { kind: "else-if", label: "Else If", category: "control", params: ["condition"], description: "Alternative condition in an If block", continuesBlock: true },
  { kind: "else", label: "Else", category: "control", params: [], description: "Default branch of an If block", continuesBlock: true },
  { kind: "end-if", label: "End If", category: "control", params: [], description: "Close an If block", closesBlock: true },
  { kind: "loop", label: "Loop", category: "control", params: [], description: "Start a loop block", opensBlock: true },
  { kind: "exit-loop-if", label: "Exit Loop If", category: "control", params: ["condition"], description: "Break out of loop when condition is true" },
  { kind: "end-loop", label: "End Loop", category: "control", params: [], description: "Close a loop block", closesBlock: true },

  // UI
  { kind: "show-dialog", label: "Show Dialog", category: "ui", params: ["title", "message", "buttons"], description: "Show a modal dialog to the user" },
  { kind: "show-notification", label: "Show Notification", category: "ui", params: ["title", "kind"], description: "Show a toast notification" },
  { kind: "refresh-window", label: "Refresh Window", category: "ui", params: [], description: "Refresh the current view" },
  { kind: "freeze-window", label: "Freeze Window", category: "ui", params: [], description: "Pause screen updates during script execution" },
  { kind: "close-window", label: "Close Window", category: "ui", params: [], description: "Close the current window" },

  // Find/Sort
  { kind: "perform-find", label: "Perform Find", category: "find", params: ["field", "value"], description: "Find records matching criteria" },
  { kind: "show-all-records", label: "Show All Records", category: "find", params: [], description: "Clear the current found set filter" },
  { kind: "sort-records", label: "Sort Records", category: "find", params: ["field", "direction"], description: "Sort the current found set" },
  { kind: "constrain-found-set", label: "Constrain Found Set", category: "find", params: ["field", "value"], description: "Narrow the current found set" },
  { kind: "extend-found-set", label: "Extend Found Set", category: "find", params: ["field", "value"], description: "Add records to the current found set" },

  // Script
  { kind: "run-script", label: "Run Script", category: "script", params: ["scriptId", "parameter"], description: "Execute another script" },
  { kind: "exit-script", label: "Exit Script", category: "script", params: ["result"], description: "Exit the current script with an optional result" },
  { kind: "halt-script", label: "Halt Script", category: "script", params: [], description: "Stop all running scripts" },
  { kind: "comment", label: "Comment", category: "script", params: ["text"], description: "Add a comment (no code generated)" },

  // Custom
  { kind: "custom-luau", label: "Custom Luau", category: "custom", params: ["code"], description: "Insert raw Luau code (advanced)" },
];

export function getStepMeta(kind: ScriptStepKind): StepKindMeta {
  return STEP_KINDS.find((s) => s.kind === kind) ?? STEP_KINDS[STEP_KINDS.length - 1] as StepKindMeta;
}

// ── Script Step ─────────────────────────────────────────────────────────────

export interface ScriptStep {
  id: string;
  kind: ScriptStepKind;
  /** Step parameters keyed by param name. */
  params: Record<string, string>;
  /** Whether this step is disabled (skipped during execution). */
  disabled?: boolean;
}

let stepCounter = 0;

export function createStep(
  kind: ScriptStepKind,
  params?: Record<string, string>,
): ScriptStep {
  return {
    id: `step_${++stepCounter}`,
    kind,
    params: params ?? {},
  };
}

// ── Script Definition ───────────────────────────────────────────────────────

export interface VisualScript {
  id: string;
  name: string;
  description?: string;
  steps: ScriptStep[];
  /** Parameter names this script accepts when called by Run Script. */
  parameters?: string[];
}

export function createVisualScript(id: string, name: string): VisualScript {
  return { id, name, steps: [] };
}

// ── Luau Code Generation ─────────────────────────────────────────────────────

function luaValue(raw: string): string {
  if (!raw) return "nil";
  // Already quoted string
  if ((raw.startsWith('"') && raw.endsWith('"')) ||
      (raw.startsWith("'") && raw.endsWith("'"))) {
    return raw;
  }
  // Boolean/nil/number
  if (raw === "true" || raw === "false" || raw === "nil") return raw;
  if (!isNaN(Number(raw)) && raw.trim() !== "") return raw;
  // Expression (contains operators or brackets — but not just hyphens in identifiers)
  if (raw.includes("[") || raw.includes("(") || raw.includes("+") ||
      raw.includes("*") || raw.includes("/") ||
      raw.includes(">") || raw.includes("<") || raw.includes("==") ||
      raw.includes("~=") || raw.includes("..")) {
    return raw;
  }
  // Treat as string literal
  return `"${raw.replace(/"/g, '\\"')}"`;
}

function emitStep(step: ScriptStep): string {
  const p = step.params;

  switch (step.kind) {
    // Navigation
    case "go-to-layout":
      return `Prism.goToLayout(${luaValue(p.layoutId ?? "")})`;
    case "go-to-record":
      return `Prism.goToRecord(${luaValue(p.position ?? "next")})`;
    case "go-to-related":
      return `Prism.goToRelated(${luaValue(p.relationshipId ?? "")}, ${luaValue(p.layoutId ?? "")})`;

    // Data
    case "set-field":
      return `Prism.setField(${luaValue(p.target ?? "")}, ${luaValue(p.value ?? "")})`;
    case "set-variable":
      return `local ${p.name ?? "x"} = ${luaValue(p.value ?? "")}`;
    case "new-record":
      return `Prism.newRecord(${luaValue(p.objectType ?? "")})`;
    case "duplicate-record":
      return "Prism.duplicateRecord()";
    case "delete-record":
      return "Prism.deleteRecord()";
    case "commit-record":
      return "Prism.commitRecord()";
    case "revert-record":
      return "Prism.revertRecord()";

    // Control flow
    case "if":
      return `if ${p.condition ?? "true"} then`;
    case "else-if":
      return `elseif ${p.condition ?? "true"} then`;
    case "else":
      return "else";
    case "end-if":
      return "end";
    case "loop":
      return "while true do";
    case "exit-loop-if":
      return `if ${p.condition ?? "true"} then break end`;
    case "end-loop":
      return "end";

    // UI
    case "show-dialog":
      return `Prism.showDialog(${luaValue(p.title ?? "")}, ${luaValue(p.message ?? "")}, ${luaValue(p.buttons ?? '"OK"')})`;
    case "show-notification":
      return `Prism.notify(${luaValue(p.title ?? "")}, ${luaValue(p.kind ?? '"info"')})`;
    case "refresh-window":
      return "Prism.refreshWindow()";
    case "freeze-window":
      return "Prism.freezeWindow()";
    case "close-window":
      return "Prism.closeWindow()";

    // Find/Sort
    case "perform-find":
      return `Prism.performFind(${luaValue(p.field ?? "")}, ${luaValue(p.value ?? "")})`;
    case "show-all-records":
      return "Prism.showAllRecords()";
    case "sort-records":
      return `Prism.sortRecords(${luaValue(p.field ?? "")}, ${luaValue(p.direction ?? '"asc"')})`;
    case "constrain-found-set":
      return `Prism.constrainFoundSet(${luaValue(p.field ?? "")}, ${luaValue(p.value ?? "")})`;
    case "extend-found-set":
      return `Prism.extendFoundSet(${luaValue(p.field ?? "")}, ${luaValue(p.value ?? "")})`;

    // Script
    case "run-script":
      return p.parameter
        ? `Prism.runScript(${luaValue(p.scriptId ?? "")}, ${luaValue(p.parameter)})`
        : `Prism.runScript(${luaValue(p.scriptId ?? "")})`;
    case "exit-script":
      return p.result ? `return ${luaValue(p.result)}` : "return";
    case "halt-script":
      return "Prism.halt()";
    case "comment":
      return `-- ${p.text ?? ""}`;

    // Custom
    case "custom-luau":
      return p.code ?? "";
  }
}

/**
 * Emit a list of ScriptSteps as formatted Luau code.
 * Handles indentation based on control flow blocks.
 */
export function emitStepsLuau(steps: ScriptStep[]): string {
  return emitStepsLuauWithMap(steps).code;
}

/**
 * Result of emitting visual script steps to Luau, with a bidirectional map
 * linking each step to the 1-based Luau source line it generated.
 *
 * This is how the Luau debugger unifies visual-script debugging with raw
 * Luau debugging: a breakpoint set on a visual step finds the emitted line,
 * and when the debugger pauses on a line it can highlight the owning step.
 */
export interface StepsLuauEmitResult {
  /** The full Luau source. */
  code: string;
  /** Map from ScriptStep.id → 1-based line number in `code`. */
  stepToLine: Map<string, number>;
  /** Map from 1-based line number → ScriptStep.id. */
  lineToStep: Map<number, string>;
}

/**
 * Emit ScriptSteps as Luau code with a source map linking each step
 * to its generated line. Disabled steps produce a comment line and
 * are still recorded in the map so the UI can highlight them.
 */
export function emitStepsLuauWithMap(steps: ScriptStep[]): StepsLuauEmitResult {
  const lines: string[] = [];
  const stepToLine = new Map<string, number>();
  const lineToStep = new Map<number, string>();
  let indent = 0;

  for (const step of steps) {
    if (step.disabled) {
      const lineNo = lines.length + 1;
      lines.push(`${"  ".repeat(indent)}-- [disabled] ${step.kind}`);
      stepToLine.set(step.id, lineNo);
      lineToStep.set(lineNo, step.id);
      continue;
    }

    const meta = getStepMeta(step.kind);

    // Decrease indent before closing/continuing blocks
    if (meta.closesBlock || meta.continuesBlock) {
      indent = Math.max(0, indent - 1);
    }

    const code = emitStep(step);
    if (code) {
      const lineNo = lines.length + 1;
      lines.push(`${"  ".repeat(indent)}${code}`);
      stepToLine.set(step.id, lineNo);
      lineToStep.set(lineNo, step.id);
    }

    // Increase indent after opening/continuing blocks
    if (meta.opensBlock || meta.continuesBlock) {
      indent++;
    }
  }

  return { code: lines.join("\n"), stepToLine, lineToStep };
}

/**
 * Validate script step structure.
 * Returns error messages for mismatched If/Loop blocks.
 */
export function validateSteps(steps: ScriptStep[]): string[] {
  const errors: string[] = [];
  const stack: Array<{ kind: "if" | "loop"; index: number }> = [];

  for (let i = 0; i < steps.length; i++) {
    const step = steps[i] as ScriptStep;
    switch (step.kind) {
      case "if":
        stack.push({ kind: "if", index: i });
        break;
      case "else-if":
      case "else":
        if (stack.length === 0 || stack[stack.length - 1]?.kind !== "if") {
          errors.push(`Step ${i + 1}: "${step.kind}" without matching "If"`);
        }
        break;
      case "end-if":
        if (stack.length === 0 || stack[stack.length - 1]?.kind !== "if") {
          errors.push(`Step ${i + 1}: "End If" without matching "If"`);
        } else {
          stack.pop();
        }
        break;
      case "loop":
        stack.push({ kind: "loop", index: i });
        break;
      case "exit-loop-if":
        if (!stack.some((s) => s.kind === "loop")) {
          errors.push(`Step ${i + 1}: "Exit Loop If" outside of a loop`);
        }
        break;
      case "end-loop":
        if (stack.length === 0 || stack[stack.length - 1]?.kind !== "loop") {
          errors.push(`Step ${i + 1}: "End Loop" without matching "Loop"`);
        } else {
          stack.pop();
        }
        break;
    }
  }

  for (const open of stack) {
    errors.push(`Step ${open.index + 1}: Unclosed "${open.kind === "if" ? "If" : "Loop"}" block`);
  }

  return errors;
}

/**
 * Get step categories for UI grouping.
 */
export function getStepCategories(): Array<{ category: string; steps: StepKindMeta[] }> {
  const categories = new Map<string, StepKindMeta[]>();
  for (const meta of STEP_KINDS) {
    let list = categories.get(meta.category);
    if (!list) {
      list = [];
      categories.set(meta.category, list);
    }
    list.push(meta);
  }
  return [...categories.entries()].map(([category, steps]) => ({ category, steps }));
}
