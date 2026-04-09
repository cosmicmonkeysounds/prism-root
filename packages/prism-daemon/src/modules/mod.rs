//! Built-in daemon modules. Each module is feature-gated and self-registers
//! its commands into the [`CommandRegistry`](crate::registry::CommandRegistry)
//! when installed via the [`DaemonBuilder`](crate::builder::DaemonBuilder).

#[cfg(feature = "crdt")]
pub mod crdt_module;

#[cfg(feature = "lua")]
pub mod lua_module;

#[cfg(feature = "build")]
pub mod build_module;

#[cfg(feature = "watcher")]
pub mod watcher_module;
