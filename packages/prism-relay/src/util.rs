//! Shared utility functions used across relay modules.

pub fn default_true() -> bool {
    true
}

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
