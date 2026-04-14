//! Runtime boot-config resolver. Legacy TS version lived at
//! `prism-studio/src/boot/load-boot-config.ts`.
//!
//! In the Clay era the sources shrink from four to two — there is
//! no more JS runtime so `import.meta.env` and URL query params go
//! away. Remaining sources:
//!
//! 1. `PRISM_BOOT_CONFIG` env var, if the Tauri shell (or a test
//!    harness) set one.
//! 2. Build-time default compiled into the binary.
//! 3. [`DEFAULT_BOOT_CONFIG`] as the last-resort fallback.

use serde::{Deserialize, Serialize};

use crate::shell_mode::{Permission, ShellMode, ShellModeContext};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BootConfig {
    pub shell_mode: ShellMode,
    pub permission: Permission,
}

impl From<BootConfig> for ShellModeContext {
    fn from(value: BootConfig) -> Self {
        Self {
            shell_mode: value.shell_mode,
            permission: value.permission,
        }
    }
}

pub const DEFAULT_BOOT_CONFIG: BootConfig = BootConfig {
    shell_mode: ShellMode::Build,
    permission: Permission::Dev,
};

/// Resolve the runtime boot config.
///
/// Today this only checks the `PRISM_BOOT_CONFIG` env var (JSON). Returns
/// the default if the var is missing, empty, or fails to parse. Parse
/// errors are swallowed rather than propagated — a broken env var should
/// not block boot.
pub fn resolve_boot_config() -> BootConfig {
    std::env::var("PRISM_BOOT_CONFIG")
        .ok()
        .and_then(|raw| serde_json::from_str::<BootConfig>(&raw).ok())
        .unwrap_or(DEFAULT_BOOT_CONFIG)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_build_dev() {
        assert!(matches!(DEFAULT_BOOT_CONFIG.shell_mode, ShellMode::Build));
        assert!(matches!(DEFAULT_BOOT_CONFIG.permission, Permission::Dev));
    }
}
