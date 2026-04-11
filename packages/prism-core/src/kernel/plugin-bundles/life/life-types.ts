/**
 * @prism/plugin-life — Life & Wellness Domain Types (Layer 1)
 *
 * Entirely new Flux entity families for personal health tracking:
 * habits, fitness, sleep, journal, meals, cycle tracking, sexual health.
 */

import type { EntityDef, EdgeTypeDef } from "@prism/core/object-model";
import type { FluxAutomationPreset } from "@prism/core/flux";
import type { PrismPlugin } from "@prism/core/plugin";

// ── Categories ───────────────────────────────────────────────────────────

export const LIFE_CATEGORIES = {
  HABITS: "life:habits",
  FITNESS: "life:fitness",
  WELLNESS: "life:wellness",
  JOURNAL: "life:journal",
  NUTRITION: "life:nutrition",
} as const;

export type LifeCategory = typeof LIFE_CATEGORIES[keyof typeof LIFE_CATEGORIES];

// ── Entity Type Strings ──────────────────────────────────────────────────

export const LIFE_TYPES = {
  HABIT: "life:habit",
  HABIT_LOG: "life:habit-log",
  FITNESS_LOG: "life:fitness-log",
  SLEEP_RECORD: "life:sleep-record",
  JOURNAL_ENTRY: "life:journal-entry",
  MEAL_PLAN: "life:meal-plan",
  CYCLE_ENTRY: "life:cycle-entry",
} as const;

export type LifeEntityType = typeof LIFE_TYPES[keyof typeof LIFE_TYPES];

// ── Edge Relation Strings ───────────���────────────────────────────────────

export const LIFE_EDGES = {
  LOG_OF: "life:log-of",
  MEAL_FOR: "life:meal-for",
  RELATED_SYMPTOM: "life:related-symptom",
} as const;

export type LifeEdgeType = typeof LIFE_EDGES[keyof typeof LIFE_EDGES];

// ── Status Values ────────────────��────────────────────────────────��──────

export const HABIT_STATUSES = [
  { value: "active", label: "Active" },
  { value: "paused", label: "Paused" },
  { value: "archived", label: "Archived" },
] as const;

export const HABIT_FREQUENCIES = [
  { value: "daily", label: "Daily" },
  { value: "weekdays", label: "Weekdays" },
  { value: "weekends", label: "Weekends" },
  { value: "weekly", label: "Weekly" },
  { value: "custom", label: "Custom" },
] as const;

export const WORKOUT_TYPES = [
  { value: "strength", label: "Strength" },
  { value: "cardio", label: "Cardio" },
  { value: "flexibility", label: "Flexibility" },
  { value: "hiit", label: "HIIT" },
  { value: "yoga", label: "Yoga" },
  { value: "sport", label: "Sport" },
  { value: "walk", label: "Walk" },
  { value: "other", label: "Other" },
] as const;

export const SLEEP_QUALITY = [
  { value: "excellent", label: "Excellent" },
  { value: "good", label: "Good" },
  { value: "fair", label: "Fair" },
  { value: "poor", label: "Poor" },
  { value: "terrible", label: "Terrible" },
] as const;

export const MEAL_TYPES = [
  { value: "breakfast", label: "Breakfast" },
  { value: "lunch", label: "Lunch" },
  { value: "dinner", label: "Dinner" },
  { value: "snack", label: "Snack" },
] as const;

export const JOURNAL_MOODS = [
  { value: "great", label: "Great" },
  { value: "good", label: "Good" },
  { value: "neutral", label: "Neutral" },
  { value: "low", label: "Low" },
  { value: "bad", label: "Bad" },
] as const;

export const CYCLE_PHASES = [
  { value: "menstrual", label: "Menstrual" },
  { value: "follicular", label: "Follicular" },
  { value: "ovulation", label: "Ovulation" },
  { value: "luteal", label: "Luteal" },
] as const;

export const FLOW_LEVELS = [
  { value: "none", label: "None" },
  { value: "spotting", label: "Spotting" },
  { value: "light", label: "Light" },
  { value: "medium", label: "Medium" },
  { value: "heavy", label: "Heavy" },
] as const;

// ─��� Registry ─────────────────────────────────────────────────────────────

export interface LifeRegistry {
  getEntityDefs(): EntityDef[];
  getEdgeDefs(): EdgeTypeDef[];
  getEntityDef(type: LifeEntityType): EntityDef | undefined;
  getEdgeDef(relation: LifeEdgeType): EdgeTypeDef | undefined;
  getAutomationPresets(): FluxAutomationPreset[];
  getPlugin(): PrismPlugin;
}
