//! Loom v3 Language Server Protocol implementation.
//!
//! Library surface: a [`workspace::Workspace`] that reparses on every
//! change, builds project-wide indices (characters, traits, beats,
//! anchors, ` ```todo``` ` fences — spec §17), and answers LSP
//! requests as pure function calls — `Workspace::completion_at`,
//! `hover_at`, `definition_at`, `document_symbols`,
//! `diagnostics_for`.
//!
//! The native `loom-lsp` binary (default `stdio` feature on) consumes
//! that surface: [`run_stdio`] pumps JSON-RPC over stdin/stdout for
//! Zed / VSCode. (The web editor no longer goes through this crate — it
//! consumes the TypeScript port `@loom/core/lsp` directly; the former
//! `loom-wasm` bridge was deleted.)

pub mod completion;
pub mod definition;
pub mod hover;
pub mod references;
pub mod symbols;
pub mod workspace;

pub use workspace::{OpenDoc, Workspace};

#[cfg(feature = "stdio")]
mod stdio_server;

#[cfg(feature = "stdio")]
pub use stdio_server::{run_stdio, server_capabilities};
