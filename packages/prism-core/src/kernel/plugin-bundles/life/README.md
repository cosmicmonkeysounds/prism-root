# plugin-bundles/life

Life & Wellness bundle. Registers entirely new Flux entity families for personal health tracking: habits, fitness logs, sleep records, journal entries, meal plans, and cycle tracking.

```ts
import { createLifeBundle } from "@prism/core/plugin-bundles";
```

## What it registers

- **Categories** (`LIFE_CATEGORIES`): `life:habits`, `life:fitness`, `life:wellness`, `life:journal`, `life:nutrition`.
- **Entity types** (`LIFE_TYPES`): `Habit`, `HabitLog`, `FitnessLog`, `SleepRecord`, `JournalEntry`, `MealPlan`, `CycleEntry`.
- **Edges** (`LIFE_EDGES`): `log-of`, `meal-for`, `related-symptom`.
- **Status / enum sets**: `HABIT_STATUSES`, `HABIT_FREQUENCIES` (daily/weekdays/weekends/weekly/custom), `WORKOUT_TYPES` (strength/cardio/flexibility/hiit/yoga/sport/walk/other), `SLEEP_QUALITY`, `MEAL_TYPES`, `JOURNAL_MOODS`, `CYCLE_PHASES`, `FLOW_LEVELS`.
- **Plugin contributions**: life/wellness views, commands, and activity-bar entries.

## Key exports

- `createLifeBundle()` — self-registering `PluginBundle`.
- `createLifeRegistry()` — lower-level `LifeRegistry` exposing entity/edge defs, automation presets, and the `PrismPlugin`.
- Constants: `LIFE_CATEGORIES`, `LIFE_TYPES`, `LIFE_EDGES`, `HABIT_STATUSES`, `HABIT_FREQUENCIES`, `WORKOUT_TYPES`, `SLEEP_QUALITY`, `MEAL_TYPES`, `JOURNAL_MOODS`, `CYCLE_PHASES`, `FLOW_LEVELS`.
- Types: `LifeCategory`, `LifeEntityType`, `LifeEdgeType`, `LifeRegistry`.

## Usage

```ts
import {
  createLifeBundle,
  installPluginBundles,
} from "@prism/core/plugin-bundles";

installPluginBundles([createLifeBundle()], {
  objectRegistry,
  pluginRegistry,
});
```
