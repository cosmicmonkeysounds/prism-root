//! `prism-cli` — the unified Prism Framework CLI.
//!
//! A single `prism` binary replaces the ad-hoc mix of `cargo`,
//! `pnpm`, and `trunk` commands the workspace used to require. It
//! is the umbrella runner for:
//!
//! - `prism test`  — run Rust unit tests (`cargo test`) and the
//!   relay E2E suite (`playwright`) with per-package filtering and
//!   unified reporting. Relay TS unit tests (`vitest`) are on hold
//!   until the relay rewrite lands a real test runner.
//! - `prism build` — build every target (desktop / web / relay /
//!   all), in debug or release.
//! - `prism dev`   — spawn one or many dev servers behind a process
//!   supervisor with colored prefixed logs and Ctrl+C fan-out.
//! - `prism lint`  — `cargo clippy --workspace --all-targets -- -D warnings`.
//! - `prism fmt`   — `cargo fmt --all`.
//!
//! The library half of this crate exposes the [`builder`] module
//! (the [`CommandBuilder`] fluent API used to construct every shelled
//! command), the [`workspace`] module (filesystem discovery of the
//! workspace root), the [`supervisor`] module (multi-process dev
//! runner), and the [`commands`] module (argument types and
//! dispatch). Everything here is reachable from `#[cfg(test)]` code
//! and from the `tests/` integration suite — the `prism` binary
//! itself is a thin `clap` wrapper over [`commands::run`].

pub mod builder;
pub mod commands;
pub mod dev_loop;
pub mod supervisor;
pub mod watch;
pub mod workspace;

pub use builder::{CommandBuilder, Program};
pub use commands::{Cli, Command};
pub use dev_loop::{DevLoop, DevLoopOutcome};
pub use supervisor::{Supervisor, SupervisorOutcome};
pub use watch::{WatchBatch, WatchLoop};
pub use workspace::Workspace;
