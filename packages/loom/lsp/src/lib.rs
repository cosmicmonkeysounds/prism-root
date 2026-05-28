//! Loom v3 Language Server Protocol implementation.
//!
//! Library surface: a [`workspace::Workspace`] that reparses on every
//! change, builds project-wide indices (characters, traits, beats,
//! anchors, ` ```todo``` ` fences — spec §17), and answers LSP
//! requests as pure function calls — `Workspace::completion_at`,
//! `hover_at`, `definition_at`, `document_symbols`,
//! `diagnostics_for`.
//!
//! Two front ends consume that surface:
//!
//! - **Native** (`loom-lsp` binary, default `stdio` feature on) —
//!   [`run_stdio`] pumps JSON-RPC over stdin/stdout for Zed / VSCode.
//! - **Web** (`loom-wasm`, this crate compiled with
//!   `default-features = false`) — `loom-wasm` exposes thin
//!   wasm-bindgen wrappers around the same `Workspace` so the
//!   browser editor's CodeMirror integration gets hover / completion
//!   / definition / outline without any stdio plumbing.

pub mod completion;
pub mod definition;
pub mod hover;
pub mod symbols;
pub mod workspace;

pub use workspace::{OpenDoc, Workspace};

#[cfg(feature = "stdio")]
mod stdio_server;

#[cfg(feature = "stdio")]
pub use stdio_server::{run_stdio, server_capabilities};
