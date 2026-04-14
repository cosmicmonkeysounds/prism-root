//! Shell mode + permission — the `(shellMode, permission)` runtime
//! context Studio boots into. Straight port of `shell-mode.ts` from
//! the legacy TS tree.
//!
//! * `ShellMode` — which shell tree the user sees (use / build / admin).
//! * `Permission` — frozen-at-boot capability tier (user / dev).
//!
//! A lens declares `available_in_modes` + `min_permission` and
//! [`ShellModeContext::can_see`] returns whether that lens is
//! reachable in the current context.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellMode {
    Use,
    Build,
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    User = 0,
    Dev = 1,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ShellModeContext {
    pub shell_mode: ShellMode,
    pub permission: Permission,
}

impl ShellModeContext {
    pub fn can_see(&self, lens: &LensVisibility) -> bool {
        lens.available_in_modes
            .iter()
            .any(|m| *m == self.shell_mode)
            && self.permission >= lens.min_permission
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LensVisibility {
    pub available_in_modes: Vec<ShellMode>,
    pub min_permission: Permission,
}

impl Default for LensVisibility {
    fn default() -> Self {
        Self {
            available_in_modes: vec![ShellMode::Build, ShellMode::Admin],
            min_permission: Permission::User,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_lens_hidden_in_use_mode() {
        let lens = LensVisibility::default();
        let ctx = ShellModeContext {
            shell_mode: ShellMode::Use,
            permission: Permission::Dev,
        };
        assert!(!ctx.can_see(&lens));
    }

    #[test]
    fn user_permission_cannot_see_dev_lens() {
        let lens = LensVisibility {
            available_in_modes: vec![ShellMode::Build],
            min_permission: Permission::Dev,
        };
        let ctx = ShellModeContext {
            shell_mode: ShellMode::Build,
            permission: Permission::User,
        };
        assert!(!ctx.can_see(&lens));
    }
}
