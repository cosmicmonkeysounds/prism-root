//! `network::session` — session transcripts and playback.
//!
//! Port of the session management concepts from the legacy TS tree.
//! `SessionManager` tracks active collaboration sessions.
//! `TranscriptTimeline` records an ordered log of session events
//! (joins, leaves, operations, messages) for audit/replay.
//! `PlaybackController` drives timeline replay at variable speed.

pub mod manager;
pub mod playback;
pub mod transcript;

pub use manager::{Session, SessionManager, SessionManagerOptions, SessionStatus};
pub use playback::{PlaybackController, PlaybackState};
pub use transcript::{TranscriptEntry, TranscriptEntryKind, TranscriptTimeline};
