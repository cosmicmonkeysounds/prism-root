//! `plugin_bundles::life` — Life & Wellness bundle.
//!
//! Port of `kernel/plugin-bundles/life/life.ts`: habits, fitness, sleep,
//! journal, meals, cycle tracking.

use serde_json::json;

use super::builders::{
    edge_def, entity_def, enum_options, owned_strings, ui_multiline, ui_placeholder, ui_readonly,
    EdgeSpec, EntitySpec, Field,
};
use super::flux_types::{
    FluxActionKind, FluxAutomationAction, FluxAutomationPreset, FluxTriggerKind,
};
use super::install::{PluginBundle, PluginInstallContext};
use crate::foundation::object_model::types::DefaultChildView;
use crate::foundation::object_model::{
    EdgeBehavior, EdgeTypeDef, EntityDef, EntityFieldDef, EntityFieldType, EnumOption,
};
use crate::kernel::plugin::{
    plugin_id, ActivityBarContributionDef, ActivityBarPosition, CommandContributionDef,
    KeybindingContributionDef, PluginContributions, PrismPlugin, ViewContributionDef, ViewZone,
};

// ── Domain constants ────────────────────────────────────────────────────────

pub mod life_categories {
    pub const HABITS: &str = "life:habits";
    pub const FITNESS: &str = "life:fitness";
    pub const WELLNESS: &str = "life:wellness";
    pub const JOURNAL: &str = "life:journal";
    pub const NUTRITION: &str = "life:nutrition";
}

pub mod life_types {
    pub const HABIT: &str = "life:habit";
    pub const HABIT_LOG: &str = "life:habit-log";
    pub const FITNESS_LOG: &str = "life:fitness-log";
    pub const SLEEP_RECORD: &str = "life:sleep-record";
    pub const JOURNAL_ENTRY: &str = "life:journal-entry";
    pub const MEAL_PLAN: &str = "life:meal-plan";
    pub const CYCLE_ENTRY: &str = "life:cycle-entry";
}

pub mod life_edges {
    pub const LOG_OF: &str = "life:log-of";
    pub const MEAL_FOR: &str = "life:meal-for";
    pub const RELATED_SYMPTOM: &str = "life:related-symptom";
}

// ── Shared option tables ────────────────────────────────────────────────────

fn habit_frequencies() -> Vec<EnumOption> {
    enum_options(&[
        ("daily", "Daily"),
        ("weekdays", "Weekdays"),
        ("weekends", "Weekends"),
        ("weekly", "Weekly"),
        ("custom", "Custom"),
    ])
}

fn workout_types() -> Vec<EnumOption> {
    enum_options(&[
        ("strength", "Strength"),
        ("cardio", "Cardio"),
        ("flexibility", "Flexibility"),
        ("hiit", "HIIT"),
        ("yoga", "Yoga"),
        ("sport", "Sport"),
        ("walk", "Walk"),
        ("other", "Other"),
    ])
}

fn sleep_quality() -> Vec<EnumOption> {
    enum_options(&[
        ("excellent", "Excellent"),
        ("good", "Good"),
        ("fair", "Fair"),
        ("poor", "Poor"),
        ("terrible", "Terrible"),
    ])
}

fn meal_types() -> Vec<EnumOption> {
    enum_options(&[
        ("breakfast", "Breakfast"),
        ("lunch", "Lunch"),
        ("dinner", "Dinner"),
        ("snack", "Snack"),
    ])
}

fn journal_moods() -> Vec<EnumOption> {
    enum_options(&[
        ("great", "Great"),
        ("good", "Good"),
        ("neutral", "Neutral"),
        ("low", "Low"),
        ("bad", "Bad"),
    ])
}

fn cycle_phases() -> Vec<EnumOption> {
    enum_options(&[
        ("menstrual", "Menstrual"),
        ("follicular", "Follicular"),
        ("ovulation", "Ovulation"),
        ("luteal", "Luteal"),
    ])
}

fn flow_levels() -> Vec<EnumOption> {
    enum_options(&[
        ("none", "None"),
        ("spotting", "Spotting"),
        ("light", "Light"),
        ("medium", "Medium"),
        ("heavy", "Heavy"),
    ])
}

// ── Fields ──────────────────────────────────────────────────────────────────

fn habit_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("frequency", EntityFieldType::Enum)
            .label("Frequency")
            .enum_values(habit_frequencies())
            .default(json!("daily"))
            .build(),
        Field::new("targetCount", EntityFieldType::Int)
            .label("Target Count")
            .default(json!(1))
            .build(),
        Field::new("unit", EntityFieldType::String)
            .label("Unit")
            .ui(ui_placeholder("e.g. times, minutes, pages"))
            .build(),
        Field::new("streak", EntityFieldType::Int)
            .label("Current Streak")
            .default(json!(0))
            .ui(ui_readonly())
            .build(),
        Field::new("longestStreak", EntityFieldType::Int)
            .label("Longest Streak")
            .default(json!(0))
            .ui(ui_readonly())
            .build(),
        Field::new("totalCompletions", EntityFieldType::Int)
            .label("Total Completions")
            .default(json!(0))
            .ui(ui_readonly())
            .build(),
        Field::new("startDate", EntityFieldType::Date)
            .label("Start Date")
            .build(),
        Field::new("color", EntityFieldType::Color)
            .label("Color")
            .build(),
        Field::new("reminderTime", EntityFieldType::String)
            .label("Reminder Time")
            .ui(ui_placeholder("HH:MM"))
            .build(),
    ]
}

fn habit_log_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("date", EntityFieldType::Date)
            .label("Date")
            .required()
            .build(),
        Field::new("count", EntityFieldType::Int)
            .label("Count")
            .default(json!(1))
            .build(),
        Field::new("note", EntityFieldType::Text)
            .label("Note")
            .ui(ui_multiline())
            .build(),
    ]
}

fn fitness_log_fields() -> Vec<EntityFieldDef> {
    let rpe_options: Vec<EnumOption> = (1..=10)
        .map(|i| EnumOption {
            value: i.to_string(),
            label: i.to_string(),
        })
        .collect();
    vec![
        Field::new("workoutType", EntityFieldType::Enum)
            .label("Workout Type")
            .enum_values(workout_types())
            .required()
            .build(),
        Field::new("date", EntityFieldType::Date)
            .label("Date")
            .required()
            .build(),
        Field::new("durationMinutes", EntityFieldType::Int)
            .label("Duration (min)")
            .build(),
        Field::new("caloriesBurned", EntityFieldType::Int)
            .label("Calories Burned")
            .build(),
        Field::new("heartRateAvg", EntityFieldType::Int)
            .label("Avg Heart Rate")
            .build(),
        Field::new("heartRateMax", EntityFieldType::Int)
            .label("Max Heart Rate")
            .build(),
        Field::new("distance", EntityFieldType::Float)
            .label("Distance")
            .build(),
        Field::new("distanceUnit", EntityFieldType::Enum)
            .label("Distance Unit")
            .enum_values(enum_options(&[
                ("km", "Kilometers"),
                ("mi", "Miles"),
                ("m", "Meters"),
            ]))
            .default(json!("km"))
            .build(),
        Field::new("rpe", EntityFieldType::Enum)
            .label("RPE (1-10)")
            .enum_values(rpe_options)
            .build(),
        Field::new("notes", EntityFieldType::Text)
            .label("Notes")
            .ui(ui_multiline())
            .build(),
    ]
}

fn sleep_record_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("date", EntityFieldType::Date)
            .label("Date")
            .required()
            .build(),
        Field::new("bedtime", EntityFieldType::String)
            .label("Bedtime")
            .ui(ui_placeholder("HH:MM"))
            .build(),
        Field::new("wakeTime", EntityFieldType::String)
            .label("Wake Time")
            .ui(ui_placeholder("HH:MM"))
            .build(),
        Field::new("durationHours", EntityFieldType::Float)
            .label("Duration (hours)")
            .build(),
        Field::new("quality", EntityFieldType::Enum)
            .label("Quality")
            .enum_values(sleep_quality())
            .build(),
        Field::new("interruptions", EntityFieldType::Int)
            .label("Interruptions")
            .default(json!(0))
            .build(),
        Field::new("dreamsRecalled", EntityFieldType::Bool)
            .label("Dreams Recalled")
            .default(json!(false))
            .build(),
        Field::new("dreamNote", EntityFieldType::Text)
            .label("Dream Note")
            .ui(ui_multiline())
            .build(),
        Field::new("notes", EntityFieldType::Text)
            .label("Notes")
            .ui(ui_multiline())
            .build(),
    ]
}

fn journal_entry_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("date", EntityFieldType::Date)
            .label("Date")
            .required()
            .build(),
        Field::new("mood", EntityFieldType::Enum)
            .label("Mood")
            .enum_values(journal_moods())
            .build(),
        Field::new("energyLevel", EntityFieldType::Enum)
            .label("Energy")
            .enum_values(enum_options(&[
                ("high", "High"),
                ("medium", "Medium"),
                ("low", "Low"),
            ]))
            .build(),
        Field::new("gratitude", EntityFieldType::Text)
            .label("Gratitude")
            .ui(ui_multiline())
            .build(),
        Field::new("content", EntityFieldType::Text)
            .label("Entry")
            .ui(ui_multiline())
            .build(),
        Field::new("tags", EntityFieldType::String)
            .label("Tags")
            .ui(ui_placeholder("comma-separated"))
            .build(),
        Field::new("isPrivate", EntityFieldType::Bool)
            .label("Private")
            .default(json!(true))
            .build(),
    ]
}

fn meal_plan_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("date", EntityFieldType::Date)
            .label("Date")
            .required()
            .build(),
        Field::new("mealType", EntityFieldType::Enum)
            .label("Meal Type")
            .enum_values(meal_types())
            .required()
            .build(),
        Field::new("calories", EntityFieldType::Int)
            .label("Calories")
            .build(),
        Field::new("protein", EntityFieldType::Float)
            .label("Protein (g)")
            .build(),
        Field::new("carbs", EntityFieldType::Float)
            .label("Carbs (g)")
            .build(),
        Field::new("fat", EntityFieldType::Float)
            .label("Fat (g)")
            .build(),
        Field::new("fiber", EntityFieldType::Float)
            .label("Fiber (g)")
            .build(),
        Field::new("description", EntityFieldType::Text)
            .label("Description")
            .ui(ui_multiline())
            .build(),
        Field::new("recipe", EntityFieldType::Text)
            .label("Recipe / Notes")
            .ui(super::builders::ui_multiline_group("Details"))
            .build(),
    ]
}

fn cycle_entry_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("date", EntityFieldType::Date)
            .label("Date")
            .required()
            .build(),
        Field::new("phase", EntityFieldType::Enum)
            .label("Phase")
            .enum_values(cycle_phases())
            .build(),
        Field::new("flow", EntityFieldType::Enum)
            .label("Flow")
            .enum_values(flow_levels())
            .build(),
        Field::new("temperature", EntityFieldType::Float)
            .label("Basal Temp")
            .build(),
        Field::new("symptoms", EntityFieldType::String)
            .label("Symptoms")
            .ui(ui_placeholder("comma-separated"))
            .build(),
        Field::new("mood", EntityFieldType::Enum)
            .label("Mood")
            .enum_values(journal_moods())
            .build(),
        Field::new("cervicalMucus", EntityFieldType::Enum)
            .label("Cervical Mucus")
            .enum_values(enum_options(&[
                ("dry", "Dry"),
                ("sticky", "Sticky"),
                ("creamy", "Creamy"),
                ("watery", "Watery"),
                ("egg_white", "Egg White"),
            ]))
            .build(),
        Field::new("notes", EntityFieldType::Text)
            .label("Notes")
            .ui(ui_multiline())
            .build(),
        Field::new("isPrivate", EntityFieldType::Bool)
            .label("Private")
            .default(json!(true))
            .build(),
    ]
}

// ── Entity + edge defs ──────────────────────────────────────────────────────

pub fn build_entity_defs() -> Vec<EntityDef> {
    vec![
        entity_def(EntitySpec {
            type_name: life_types::HABIT,
            nsid: "io.prismapp.life.habit",
            category: life_categories::HABITS,
            label: "Habit",
            plural_label: "Habits",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: Some(owned_strings([life_types::HABIT_LOG])),
            fields: habit_fields(),
        }),
        entity_def(EntitySpec {
            type_name: life_types::HABIT_LOG,
            nsid: "io.prismapp.life.habit-log",
            category: life_categories::HABITS,
            label: "Habit Log",
            plural_label: "Habit Logs",
            default_child_view: None,
            child_only: true,
            extra_child_types: None,
            fields: habit_log_fields(),
        }),
        entity_def(EntitySpec {
            type_name: life_types::FITNESS_LOG,
            nsid: "io.prismapp.life.fitness-log",
            category: life_categories::FITNESS,
            label: "Workout",
            plural_label: "Workouts",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: None,
            fields: fitness_log_fields(),
        }),
        entity_def(EntitySpec {
            type_name: life_types::SLEEP_RECORD,
            nsid: "io.prismapp.life.sleep-record",
            category: life_categories::WELLNESS,
            label: "Sleep Record",
            plural_label: "Sleep Records",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: None,
            fields: sleep_record_fields(),
        }),
        entity_def(EntitySpec {
            type_name: life_types::JOURNAL_ENTRY,
            nsid: "io.prismapp.life.journal-entry",
            category: life_categories::JOURNAL,
            label: "Journal Entry",
            plural_label: "Journal Entries",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: None,
            fields: journal_entry_fields(),
        }),
        entity_def(EntitySpec {
            type_name: life_types::MEAL_PLAN,
            nsid: "io.prismapp.life.meal-plan",
            category: life_categories::NUTRITION,
            label: "Meal",
            plural_label: "Meals",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: None,
            fields: meal_plan_fields(),
        }),
        entity_def(EntitySpec {
            type_name: life_types::CYCLE_ENTRY,
            nsid: "io.prismapp.life.cycle-entry",
            category: life_categories::WELLNESS,
            label: "Cycle Entry",
            plural_label: "Cycle Entries",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: None,
            fields: cycle_entry_fields(),
        }),
    ]
}

pub fn build_edge_defs() -> Vec<EdgeTypeDef> {
    vec![
        edge_def(EdgeSpec {
            relation: life_edges::LOG_OF,
            nsid: "io.prismapp.life.log-of",
            label: "Log Of",
            behavior: EdgeBehavior::Membership,
            source_types: owned_strings([life_types::HABIT_LOG]),
            target_types: Some(owned_strings([life_types::HABIT])),
            description: None,
            suggest_inline: false,
            undirected: false,
        }),
        edge_def(EdgeSpec {
            relation: life_edges::MEAL_FOR,
            nsid: "io.prismapp.life.meal-for",
            label: "Meal For",
            behavior: EdgeBehavior::Weak,
            source_types: owned_strings([life_types::MEAL_PLAN]),
            target_types: Some(owned_strings([life_types::FITNESS_LOG])),
            description: Some("Links meals to workout recovery / fueling"),
            suggest_inline: false,
            undirected: false,
        }),
        edge_def(EdgeSpec {
            relation: life_edges::RELATED_SYMPTOM,
            nsid: "io.prismapp.life.related-symptom",
            label: "Related Symptom",
            behavior: EdgeBehavior::Weak,
            source_types: owned_strings([life_types::CYCLE_ENTRY, life_types::SLEEP_RECORD]),
            target_types: Some(owned_strings([
                life_types::JOURNAL_ENTRY,
                life_types::FITNESS_LOG,
            ])),
            description: Some("Cross-references wellness data for pattern discovery"),
            suggest_inline: false,
            undirected: true,
        }),
    ]
}

// ── Automation presets ──────────────────────────────────────────────────────

pub fn build_automation_presets() -> Vec<FluxAutomationPreset> {
    vec![
        FluxAutomationPreset {
            id: "life:auto:habit-streak".into(),
            name: "Update habit streak on log".into(),
            entity_type: life_types::HABIT.into(),
            trigger: FluxTriggerKind::OnUpdate,
            condition: Some("totalCompletions > 0".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::SetField,
                target: "streak".into(),
                value: "{{consecutiveDays(children)}}".into(),
            }],
        },
        FluxAutomationPreset {
            id: "life:auto:habit-reminder".into(),
            name: "Daily habit reminder".into(),
            entity_type: life_types::HABIT.into(),
            trigger: FluxTriggerKind::OnSchedule,
            condition: Some("status == 'active' and reminderTime != ''".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::SendNotification,
                target: "owner".into(),
                value: "Time for '{{name}}'!".into(),
            }],
        },
        FluxAutomationPreset {
            id: "life:auto:sleep-quality-alert".into(),
            name: "Poor sleep streak alert".into(),
            entity_type: life_types::SLEEP_RECORD.into(),
            trigger: FluxTriggerKind::OnCreate,
            condition: Some("quality == 'poor' or quality == 'terrible'".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::SendNotification,
                target: "owner".into(),
                value: "Sleep quality has been low — consider adjusting your routine".into(),
            }],
        },
    ]
}

// ── Plugin ──────────────────────────────────────────────────────────────────

fn view(id: &str, label: &str, component_id: &str, description: &str) -> ViewContributionDef {
    ViewContributionDef {
        id: id.into(),
        label: label.into(),
        zone: ViewZone::Content,
        component_id: component_id.into(),
        icon: None,
        default_visible: None,
        description: Some(description.into()),
        tags: None,
    }
}

fn command(id: &str, label: &str, action: &str) -> CommandContributionDef {
    CommandContributionDef {
        id: id.into(),
        label: label.into(),
        category: "Life".into(),
        shortcut: None,
        description: None,
        action: action.into(),
        payload: None,
        when: None,
    }
}

pub fn build_plugin() -> PrismPlugin {
    PrismPlugin::new(plugin_id("prism.plugin.life"), "Life").with_contributes(PluginContributions {
        views: Some(vec![
            view(
                "life:habits",
                "Habits",
                "HabitTrackerView",
                "Habit tracker with streaks",
            ),
            view("life:fitness", "Fitness", "FitnessLogView", "Workout log"),
            view("life:sleep", "Sleep", "SleepLogView", "Sleep tracker"),
            view("life:journal", "Journal", "JournalView", "Personal journal"),
            view(
                "life:meals",
                "Meals",
                "MealPlanView",
                "Meal planner & nutrition",
            ),
            view("life:cycle", "Cycle", "CycleTrackerView", "Cycle tracker"),
            view(
                "life:dashboard",
                "Life Dashboard",
                "LifeDashboardView",
                "Wellness overview",
            ),
        ]),
        commands: Some(vec![
            command("life:log-habit", "Log Habit", "life.logHabit"),
            command("life:log-workout", "Log Workout", "life.logWorkout"),
            command("life:new-journal", "New Journal Entry", "life.newJournal"),
            command("life:log-sleep", "Log Sleep", "life.logSleep"),
            command("life:log-meal", "Log Meal", "life.logMeal"),
        ]),
        keybindings: Some(vec![KeybindingContributionDef {
            command: "life:new-journal".into(),
            key: "ctrl+shift+j".into(),
            when: None,
        }]),
        activity_bar: Some(vec![ActivityBarContributionDef {
            id: "life:activity".into(),
            label: "Life".into(),
            icon: None,
            position: Some(ActivityBarPosition::Top),
            priority: Some(25),
        }]),
        context_menus: None,
        settings: None,
        toolbar: None,
        status_bar: None,
        weak_ref_providers: None,
        immersive: None,
    })
}

pub struct LifeBundle;

impl PluginBundle for LifeBundle {
    fn id(&self) -> &str {
        "prism.plugin.life"
    }

    fn name(&self) -> &str {
        "Life"
    }

    fn install(&self, ctx: &mut PluginInstallContext<'_>) {
        ctx.object_registry.register_all(build_entity_defs());
        ctx.object_registry.register_edges(build_edge_defs());
        ctx.plugin_registry.register(build_plugin());
    }
}

pub fn create_life_bundle() -> Box<dyn PluginBundle> {
    Box::new(LifeBundle)
}
