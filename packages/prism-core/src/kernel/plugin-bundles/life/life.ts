/**
 * @prism/plugin-life — Life & Wellness Domain Registry (Layer 1)
 *
 * Registers habits, fitness, sleep, journal, meals, and cycle tracking.
 */

import type { EntityDef, EntityFieldDef, EdgeTypeDef } from "@prism/core/object-model";
import type { FluxAutomationPreset } from "@prism/core/flux";
import type { PrismPlugin } from "@prism/core/plugin";
import { pluginId } from "@prism/core/plugin";
import type { LifeRegistry, LifeEntityType, LifeEdgeType } from "./life-types.js";
import {
  LIFE_CATEGORIES, LIFE_TYPES, LIFE_EDGES,
  HABIT_FREQUENCIES, WORKOUT_TYPES, SLEEP_QUALITY,
  MEAL_TYPES, JOURNAL_MOODS, CYCLE_PHASES, FLOW_LEVELS,
} from "./life-types.js";

// ── Field Definitions ────────────────────────────────────────────────────

function enumOptions(values: ReadonlyArray<{ value: string; label: string }>): Array<{ value: string; label: string }> {
  return values.map(v => ({ value: v.value, label: v.label }));
}

const HABIT_FIELDS: EntityFieldDef[] = [
  { id: "frequency", type: "enum", label: "Frequency", enumOptions: enumOptions(HABIT_FREQUENCIES), default: "daily" },
  { id: "targetCount", type: "int", label: "Target Count", default: 1 },
  { id: "unit", type: "string", label: "Unit", ui: { placeholder: "e.g. times, minutes, pages" } },
  { id: "streak", type: "int", label: "Current Streak", default: 0, ui: { readonly: true } },
  { id: "longestStreak", type: "int", label: "Longest Streak", default: 0, ui: { readonly: true } },
  { id: "totalCompletions", type: "int", label: "Total Completions", default: 0, ui: { readonly: true } },
  { id: "startDate", type: "date", label: "Start Date" },
  { id: "color", type: "color", label: "Color" },
  { id: "reminderTime", type: "string", label: "Reminder Time", ui: { placeholder: "HH:MM" } },
];

const HABIT_LOG_FIELDS: EntityFieldDef[] = [
  { id: "date", type: "date", label: "Date", required: true },
  { id: "count", type: "int", label: "Count", default: 1 },
  { id: "note", type: "text", label: "Note", ui: { multiline: true } },
];

const FITNESS_LOG_FIELDS: EntityFieldDef[] = [
  { id: "workoutType", type: "enum", label: "Workout Type", enumOptions: enumOptions(WORKOUT_TYPES), required: true },
  { id: "date", type: "date", label: "Date", required: true },
  { id: "durationMinutes", type: "int", label: "Duration (min)" },
  { id: "caloriesBurned", type: "int", label: "Calories Burned" },
  { id: "heartRateAvg", type: "int", label: "Avg Heart Rate" },
  { id: "heartRateMax", type: "int", label: "Max Heart Rate" },
  { id: "distance", type: "float", label: "Distance" },
  { id: "distanceUnit", type: "enum", label: "Distance Unit", enumOptions: [
    { value: "km", label: "Kilometers" },
    { value: "mi", label: "Miles" },
    { value: "m", label: "Meters" },
  ], default: "km" },
  { id: "rpe", type: "enum", label: "RPE (1-10)", enumOptions: Array.from({ length: 10 }, (_, i) => ({ value: String(i + 1), label: String(i + 1) })) },
  { id: "notes", type: "text", label: "Notes", ui: { multiline: true } },
];

const SLEEP_RECORD_FIELDS: EntityFieldDef[] = [
  { id: "date", type: "date", label: "Date", required: true },
  { id: "bedtime", type: "string", label: "Bedtime", ui: { placeholder: "HH:MM" } },
  { id: "wakeTime", type: "string", label: "Wake Time", ui: { placeholder: "HH:MM" } },
  { id: "durationHours", type: "float", label: "Duration (hours)" },
  { id: "quality", type: "enum", label: "Quality", enumOptions: enumOptions(SLEEP_QUALITY) },
  { id: "interruptions", type: "int", label: "Interruptions", default: 0 },
  { id: "dreamsRecalled", type: "bool", label: "Dreams Recalled", default: false },
  { id: "dreamNote", type: "text", label: "Dream Note", ui: { multiline: true } },
  { id: "notes", type: "text", label: "Notes", ui: { multiline: true } },
];

const JOURNAL_ENTRY_FIELDS: EntityFieldDef[] = [
  { id: "date", type: "date", label: "Date", required: true },
  { id: "mood", type: "enum", label: "Mood", enumOptions: enumOptions(JOURNAL_MOODS) },
  { id: "energyLevel", type: "enum", label: "Energy", enumOptions: [
    { value: "high", label: "High" },
    { value: "medium", label: "Medium" },
    { value: "low", label: "Low" },
  ] },
  { id: "gratitude", type: "text", label: "Gratitude", ui: { multiline: true } },
  { id: "content", type: "text", label: "Entry", ui: { multiline: true } },
  { id: "tags", type: "string", label: "Tags", ui: { placeholder: "comma-separated" } },
  { id: "isPrivate", type: "bool", label: "Private", default: true },
];

const MEAL_PLAN_FIELDS: EntityFieldDef[] = [
  { id: "date", type: "date", label: "Date", required: true },
  { id: "mealType", type: "enum", label: "Meal Type", enumOptions: enumOptions(MEAL_TYPES), required: true },
  { id: "calories", type: "int", label: "Calories" },
  { id: "protein", type: "float", label: "Protein (g)" },
  { id: "carbs", type: "float", label: "Carbs (g)" },
  { id: "fat", type: "float", label: "Fat (g)" },
  { id: "fiber", type: "float", label: "Fiber (g)" },
  { id: "description", type: "text", label: "Description", ui: { multiline: true } },
  { id: "recipe", type: "text", label: "Recipe / Notes", ui: { multiline: true, group: "Details" } },
];

const CYCLE_ENTRY_FIELDS: EntityFieldDef[] = [
  { id: "date", type: "date", label: "Date", required: true },
  { id: "phase", type: "enum", label: "Phase", enumOptions: enumOptions(CYCLE_PHASES) },
  { id: "flow", type: "enum", label: "Flow", enumOptions: enumOptions(FLOW_LEVELS) },
  { id: "temperature", type: "float", label: "Basal Temp" },
  { id: "symptoms", type: "string", label: "Symptoms", ui: { placeholder: "comma-separated" } },
  { id: "mood", type: "enum", label: "Mood", enumOptions: enumOptions(JOURNAL_MOODS) },
  { id: "cervicalMucus", type: "enum", label: "Cervical Mucus", enumOptions: [
    { value: "dry", label: "Dry" },
    { value: "sticky", label: "Sticky" },
    { value: "creamy", label: "Creamy" },
    { value: "watery", label: "Watery" },
    { value: "egg_white", label: "Egg White" },
  ] },
  { id: "notes", type: "text", label: "Notes", ui: { multiline: true } },
  { id: "isPrivate", type: "bool", label: "Private", default: true },
];

// ── Entity Definitions ───────────────────────────────────────────────────

function buildEntityDefs(): EntityDef[] {
  return [
    {
      type: LIFE_TYPES.HABIT,
      nsid: "io.prismapp.life.habit",
      category: LIFE_CATEGORIES.HABITS,
      label: "Habit",
      pluralLabel: "Habits",
      defaultChildView: "list",
      fields: HABIT_FIELDS,
      extraChildTypes: [LIFE_TYPES.HABIT_LOG],
    },
    {
      type: LIFE_TYPES.HABIT_LOG,
      nsid: "io.prismapp.life.habit-log",
      category: LIFE_CATEGORIES.HABITS,
      label: "Habit Log",
      pluralLabel: "Habit Logs",
      childOnly: true,
      fields: HABIT_LOG_FIELDS,
    },
    {
      type: LIFE_TYPES.FITNESS_LOG,
      nsid: "io.prismapp.life.fitness-log",
      category: LIFE_CATEGORIES.FITNESS,
      label: "Workout",
      pluralLabel: "Workouts",
      defaultChildView: "list",
      fields: FITNESS_LOG_FIELDS,
    },
    {
      type: LIFE_TYPES.SLEEP_RECORD,
      nsid: "io.prismapp.life.sleep-record",
      category: LIFE_CATEGORIES.WELLNESS,
      label: "Sleep Record",
      pluralLabel: "Sleep Records",
      defaultChildView: "list",
      fields: SLEEP_RECORD_FIELDS,
    },
    {
      type: LIFE_TYPES.JOURNAL_ENTRY,
      nsid: "io.prismapp.life.journal-entry",
      category: LIFE_CATEGORIES.JOURNAL,
      label: "Journal Entry",
      pluralLabel: "Journal Entries",
      defaultChildView: "list",
      fields: JOURNAL_ENTRY_FIELDS,
    },
    {
      type: LIFE_TYPES.MEAL_PLAN,
      nsid: "io.prismapp.life.meal-plan",
      category: LIFE_CATEGORIES.NUTRITION,
      label: "Meal",
      pluralLabel: "Meals",
      defaultChildView: "list",
      fields: MEAL_PLAN_FIELDS,
    },
    {
      type: LIFE_TYPES.CYCLE_ENTRY,
      nsid: "io.prismapp.life.cycle-entry",
      category: LIFE_CATEGORIES.WELLNESS,
      label: "Cycle Entry",
      pluralLabel: "Cycle Entries",
      defaultChildView: "list",
      fields: CYCLE_ENTRY_FIELDS,
    },
  ];
}

// ── Edge Definitions ─────────────────────────────────────────────────────

function buildEdgeDefs(): EdgeTypeDef[] {
  return [
    {
      relation: LIFE_EDGES.LOG_OF,
      nsid: "io.prismapp.life.log-of",
      label: "Log Of",
      behavior: "membership",
      sourceTypes: [LIFE_TYPES.HABIT_LOG],
      targetTypes: [LIFE_TYPES.HABIT],
    },
    {
      relation: LIFE_EDGES.MEAL_FOR,
      nsid: "io.prismapp.life.meal-for",
      label: "Meal For",
      behavior: "weak",
      sourceTypes: [LIFE_TYPES.MEAL_PLAN],
      targetTypes: [LIFE_TYPES.FITNESS_LOG],
      description: "Links meals to workout recovery / fueling",
    },
    {
      relation: LIFE_EDGES.RELATED_SYMPTOM,
      nsid: "io.prismapp.life.related-symptom",
      label: "Related Symptom",
      behavior: "weak",
      undirected: true,
      sourceTypes: [LIFE_TYPES.CYCLE_ENTRY, LIFE_TYPES.SLEEP_RECORD],
      targetTypes: [LIFE_TYPES.JOURNAL_ENTRY, LIFE_TYPES.FITNESS_LOG],
      description: "Cross-references wellness data for pattern discovery",
    },
  ];
}

// ── Automation Presets ────────────────────────────────────────────────────

function buildAutomationPresets(): FluxAutomationPreset[] {
  return [
    {
      id: "life:auto:habit-streak",
      name: "Update habit streak on log",
      entityType: LIFE_TYPES.HABIT as string,
      trigger: "on_update",
      condition: "totalCompletions > 0",
      actions: [
        { kind: "set_field", target: "streak", value: "{{consecutiveDays(children)}}" },
      ],
    },
    {
      id: "life:auto:habit-reminder",
      name: "Daily habit reminder",
      entityType: LIFE_TYPES.HABIT as string,
      trigger: "on_schedule",
      condition: "status == 'active' and reminderTime != ''",
      actions: [
        { kind: "send_notification", target: "owner", value: "Time for '{{name}}'!" },
      ],
    },
    {
      id: "life:auto:sleep-quality-alert",
      name: "Poor sleep streak alert",
      entityType: LIFE_TYPES.SLEEP_RECORD as string,
      trigger: "on_create",
      condition: "quality == 'poor' or quality == 'terrible'",
      actions: [
        { kind: "send_notification", target: "owner", value: "Sleep quality has been low — consider adjusting your routine" },
      ],
    },
  ];
}

// ── Plugin ───────────────────────────────────────────────────────────────

function buildPlugin(): PrismPlugin {
  return {
    id: pluginId("prism.plugin.life"),
    name: "Life",
    contributes: {
      views: [
        { id: "life:habits", label: "Habits", zone: "content", componentId: "HabitTrackerView", description: "Habit tracker with streaks" },
        { id: "life:fitness", label: "Fitness", zone: "content", componentId: "FitnessLogView", description: "Workout log" },
        { id: "life:sleep", label: "Sleep", zone: "content", componentId: "SleepLogView", description: "Sleep tracker" },
        { id: "life:journal", label: "Journal", zone: "content", componentId: "JournalView", description: "Personal journal" },
        { id: "life:meals", label: "Meals", zone: "content", componentId: "MealPlanView", description: "Meal planner & nutrition" },
        { id: "life:cycle", label: "Cycle", zone: "content", componentId: "CycleTrackerView", description: "Cycle tracker" },
        { id: "life:dashboard", label: "Life Dashboard", zone: "content", componentId: "LifeDashboardView", description: "Wellness overview" },
      ],
      commands: [
        { id: "life:log-habit", label: "Log Habit", category: "Life", action: "life.logHabit" },
        { id: "life:log-workout", label: "Log Workout", category: "Life", action: "life.logWorkout" },
        { id: "life:new-journal", label: "New Journal Entry", category: "Life", action: "life.newJournal" },
        { id: "life:log-sleep", label: "Log Sleep", category: "Life", action: "life.logSleep" },
        { id: "life:log-meal", label: "Log Meal", category: "Life", action: "life.logMeal" },
      ],
      keybindings: [
        { command: "life:new-journal", key: "ctrl+shift+j" },
      ],
      activityBar: [
        { id: "life:activity", label: "Life", position: "top", priority: 25 },
      ],
    },
  };
}

// ── Factory ──────────────────────────────────────────────────────────────

export function createLifeRegistry(): LifeRegistry {
  const entityDefs = buildEntityDefs();
  const edgeDefs = buildEdgeDefs();
  const presets = buildAutomationPresets();
  const plugin = buildPlugin();

  return {
    getEntityDefs: () => entityDefs,
    getEdgeDefs: () => edgeDefs,
    getEntityDef: (type: LifeEntityType) => entityDefs.find(d => d.type === type),
    getEdgeDef: (relation: LifeEdgeType) => edgeDefs.find(d => d.relation === relation),
    getAutomationPresets: () => presets,
    getPlugin: () => plugin,
  };
}

// ── Self-Registering Bundle ──────────────────────────────────────────────

import type { PluginBundle, PluginInstallContext } from "../plugin-install.js";

export function createLifeBundle(): PluginBundle {
  return {
    id: "prism.plugin.life",
    name: "Life",
    install(ctx: PluginInstallContext) {
      const reg = createLifeRegistry();
      ctx.objectRegistry.registerAll(reg.getEntityDefs());
      ctx.objectRegistry.registerEdges(reg.getEdgeDefs());
      return ctx.pluginRegistry.register(reg.getPlugin());
    },
  };
}
