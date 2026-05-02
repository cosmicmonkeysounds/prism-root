//! Pure data types for the fitness engine.
//!
//! Port of `@helm/fitness` types. Defines fitness logs, exercise
//! sets, calorie estimates, personal bests, and common MET constants.

use serde::{Deserialize, Serialize};

// ── MET Constants ────────────────────────────────────────────────

/// Metabolic Equivalent of Task for walking (~3.5 mph).
pub const MET_WALKING: f64 = 3.5;

/// MET for running (~6 min/mile pace).
pub const MET_RUNNING: f64 = 9.8;

/// MET for cycling (moderate effort).
pub const MET_CYCLING: f64 = 7.5;

/// MET for swimming (moderate effort).
pub const MET_SWIMMING: f64 = 8.0;

/// MET for weight training (general).
pub const MET_WEIGHT_TRAINING: f64 = 6.0;

/// MET for yoga (hatha).
pub const MET_YOGA: f64 = 3.0;

// ── FitnessLog ───────────────────────────────────────────────────

/// A single workout / exercise log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitnessLog {
    pub id: String,
    pub date: String,
    pub activity: String,
    pub duration_minutes: f64,
    pub weight_kg: Option<f64>,
    pub met_value: Option<f64>,
    pub sets: Option<Vec<ExerciseSet>>,
    pub notes: Option<String>,
}

// ── ExerciseSet ──────────────────────────────────────────────────

/// A single set within a strength-training exercise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExerciseSet {
    pub reps: u32,
    pub weight_kg: f64,
    pub duration_seconds: Option<f64>,
}

// ── CalorieEstimate ──────────────────────────────────────────────

/// Result of a MET-based calorie estimation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalorieEstimate {
    pub calories: f64,
    pub met_value: f64,
    pub duration_minutes: f64,
}

// ── PersonalBest ─────────────────────────────────────────────────

/// All-time records for a single exercise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalBest {
    pub exercise: String,
    pub max_weight_kg: f64,
    pub max_reps: u32,
    /// Best single-set volume (weight * reps).
    pub max_volume: f64,
    pub achieved_date: String,
}

// ── FitnessSummary ───────────────────────────────────────────────

/// Aggregate fitness statistics across a collection of logs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitnessSummary {
    pub total_workouts: u32,
    pub total_duration_minutes: f64,
    pub total_calories: f64,
    pub personal_bests: Vec<PersonalBest>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fitness_log_serde_round_trip() {
        let log = FitnessLog {
            id: "log-1".into(),
            date: "2026-05-01".into(),
            activity: "running".into(),
            duration_minutes: 30.0,
            weight_kg: Some(75.0),
            met_value: Some(MET_RUNNING),
            sets: None,
            notes: Some("Morning run".into()),
        };
        let json = serde_json::to_string(&log).unwrap();
        let back: FitnessLog = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "log-1");
        assert_eq!(back.activity, "running");
        assert_eq!(back.duration_minutes, 30.0);
    }

    #[test]
    fn exercise_set_serde_round_trip() {
        let set = ExerciseSet {
            reps: 10,
            weight_kg: 60.0,
            duration_seconds: None,
        };
        let json = serde_json::to_string(&set).unwrap();
        let back: ExerciseSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back.reps, 10);
        assert_eq!(back.weight_kg, 60.0);
    }

    #[test]
    fn calorie_estimate_serde_round_trip() {
        let est = CalorieEstimate {
            calories: 350.0,
            met_value: 9.8,
            duration_minutes: 30.0,
        };
        let json = serde_json::to_string(&est).unwrap();
        let back: CalorieEstimate = serde_json::from_str(&json).unwrap();
        assert_eq!(back.calories, 350.0);
        assert_eq!(back.met_value, 9.8);
    }

    #[test]
    fn personal_best_serde_round_trip() {
        let pb = PersonalBest {
            exercise: "bench press".into(),
            max_weight_kg: 100.0,
            max_reps: 8,
            max_volume: 800.0,
            achieved_date: "2026-04-15".into(),
        };
        let json = serde_json::to_string(&pb).unwrap();
        let back: PersonalBest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.exercise, "bench press");
        assert_eq!(back.max_volume, 800.0);
    }

    #[test]
    fn fitness_summary_serde_round_trip() {
        let summary = FitnessSummary {
            total_workouts: 5,
            total_duration_minutes: 150.0,
            total_calories: 1200.0,
            personal_bests: vec![],
        };
        let json = serde_json::to_string(&summary).unwrap();
        let back: FitnessSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total_workouts, 5);
        assert_eq!(back.total_calories, 1200.0);
    }

    #[test]
    fn met_constants_are_positive() {
        assert!(MET_WALKING > 0.0);
        assert!(MET_RUNNING > 0.0);
        assert!(MET_CYCLING > 0.0);
        assert!(MET_SWIMMING > 0.0);
        assert!(MET_WEIGHT_TRAINING > 0.0);
        assert!(MET_YOGA > 0.0);
    }
}
