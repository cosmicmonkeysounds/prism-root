//! Shared error type for the object model. Shared between
//! `tree_model` and `edge_model` so callers can match on a single
//! enum regardless of which mutator produced the error.

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectModelErrorCode {
    NotFound,
    CircularRef,
    ContainmentViolation,
    Cancelled,
}

#[derive(Debug, Error)]
#[error("{code:?}: {message}")]
pub struct ObjectModelError {
    pub code: ObjectModelErrorCode,
    pub message: String,
}

impl ObjectModelError {
    pub fn new(code: ObjectModelErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(ObjectModelErrorCode::NotFound, msg)
    }

    pub fn circular_ref(msg: impl Into<String>) -> Self {
        Self::new(ObjectModelErrorCode::CircularRef, msg)
    }

    pub fn containment_violation(msg: impl Into<String>) -> Self {
        Self::new(ObjectModelErrorCode::ContainmentViolation, msg)
    }
}
