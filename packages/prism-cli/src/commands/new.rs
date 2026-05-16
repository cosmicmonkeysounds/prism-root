//! `prism new widget <name>` — scaffold a §5.10-canonical widget.
//!
//! Wave H.5 of `docs/dev/prui-luau-fusion.md`. Emits a sibling-paired
//! `<name>.prui` + `<name>.prss` + `<name>.luau` (or a single inline
//! `.prui` with `--single-file`). The skeleton is written in the
//! canonical surface syntax (§5.10): comma-separated attributes, bare
//! values, `{expr}` value slots, `$` handler bodies, `class=…`, and
//! PRSS bare-expression braces. The Luau opens `--!strict` so typed
//! authoring is the path of least resistance (§6.6).

use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};

use crate::workspace::Workspace;

#[derive(Debug, Args)]
pub struct NewArgs {
    #[command(subcommand)]
    pub kind: NewKind,
}

#[derive(Debug, Subcommand)]
pub enum NewKind {
    /// Scaffold a new widget (markup + styles + behaviour).
    Widget(WidgetArgs),
}

#[derive(Debug, Args)]
pub struct WidgetArgs {
    /// Widget name. Used as the file basename and the PRSS class —
    /// kebab-case (`task-card`), no path separators.
    pub name: String,
    /// Directory to write into. Defaults to the current directory.
    #[arg(long)]
    pub dir: Option<PathBuf>,
    /// Emit one `.prui` with inline `<script>` / `<style>` instead of
    /// three sibling files.
    #[arg(long)]
    pub single_file: bool,
    /// Overwrite existing files instead of refusing.
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: &NewArgs, _workspace: &Workspace, dry_run: bool) -> Result<u8> {
    match &args.kind {
        NewKind::Widget(wa) => widget(wa, dry_run),
    }
}

fn widget(args: &WidgetArgs, dry_run: bool) -> Result<u8> {
    let name = args.name.trim();
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("invalid widget name `{name}` — use kebab-case, no path separators");
    }
    let dir = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));

    let files: Vec<(PathBuf, String)> = if args.single_file {
        vec![(dir.join(format!("{name}.prui")), single_file_prui(name))]
    } else {
        vec![
            (dir.join(format!("{name}.prui")), multi_prui()),
            (dir.join(format!("{name}.prss")), prss(name)),
            (dir.join(format!("{name}.luau")), luau()),
        ]
    };

    if dry_run {
        for (path, _) in &files {
            println!("would write {}", path.display());
        }
        return Ok(0);
    }

    if !args.force {
        for (path, _) in &files {
            if path.exists() {
                bail!(
                    "{} already exists — pass --force to overwrite",
                    path.display()
                );
            }
        }
    }
    if !dir.as_os_str().is_empty() {
        fs::create_dir_all(&dir).with_context(|| format!("create dir {}", dir.display()))?;
    }
    for (path, body) in &files {
        fs::write(path, body).with_context(|| format!("write {}", path.display()))?;
        println!("created {}", path.display());
    }
    Ok(0)
}

/// Sibling-paired `.prui` — markup only; `<name>.prss` /
/// `<name>.luau` are auto-attached by basename convention (§5.2), no
/// `<import>` needed.
fn multi_prui() -> String {
    "\
<container class=card, direction=column, gap=8>
  <text style:color={priority_color(state.tone)}>{title}</text>
  <button on:click=$state.expanded = not state.expanded>
    {state.expanded and \"Hide\" or \"Show\"}
  </button>
</container>
"
    .to_string()
}

fn prss(name: &str) -> String {
    format!(
        "\
[class.{name}]
background = {{ tokens.colors.surface }}
radius = 8
padding = 12 16
"
    )
}

fn luau() -> String {
    "\
--!strict

-- Flat sibling: top-level locals merge into document scope (§5.2
-- tier 1), so `title` / `state` / `priority_color` are visible to
-- every {expr} slot in the paired .prui.

local title = \"New widget\"

local state = prism.state {
  expanded = false,
  tone = \"neutral\",
}

local function priority_color(tone: string): string
  if tone == \"high\" then return tokens.colors.danger end
  return tokens.colors.text_secondary
end
"
    .to_string()
}

fn single_file_prui(name: &str) -> String {
    format!(
        "\
<style>
  [class.{name}]
  background = {{ tokens.colors.surface }}
  radius = 8
  padding = 12 16
</style>

<script>
  --!strict
  local title = \"New widget\"
  local state = prism.state {{ expanded = false, tone = \"neutral\" }}
  local function priority_color(tone: string): string
    if tone == \"high\" then return tokens.colors.danger end
    return tokens.colors.text_secondary
  end
</script>

<container class={name}, direction=column, gap=8>
  <text style:color={{priority_color(state.tone)}}>{{title}}</text>
  <button on:click=$state.expanded = not state.expanded>
    {{state.expanded and \"Hide\" or \"Show\"}}
  </button>
</container>
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> Workspace {
        Workspace::new(".")
    }

    #[test]
    fn rejects_path_traversal_name() {
        let args = NewArgs {
            kind: NewKind::Widget(WidgetArgs {
                name: "../evil".into(),
                dir: None,
                single_file: false,
                force: false,
            }),
        };
        assert!(run(&args, &ws(), false).is_err());
    }

    #[test]
    fn dry_run_writes_nothing() {
        let tmp = std::env::temp_dir().join(format!("prism-new-{}", std::process::id()));
        let args = NewArgs {
            kind: NewKind::Widget(WidgetArgs {
                name: "task-card".into(),
                dir: Some(tmp.clone()),
                single_file: false,
                force: false,
            }),
        };
        assert_eq!(run(&args, &ws(), true).unwrap(), 0);
        assert!(!tmp.join("task-card.prui").exists());
    }

    #[test]
    fn writes_sibling_trio_and_single_file() {
        let tmp = std::env::temp_dir().join(format!("prism-new-trio-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        // Trio.
        let args = NewArgs {
            kind: NewKind::Widget(WidgetArgs {
                name: "task-card".into(),
                dir: Some(tmp.clone()),
                single_file: false,
                force: false,
            }),
        };
        assert_eq!(run(&args, &ws(), false).unwrap(), 0);
        for ext in ["prui", "prss", "luau"] {
            assert!(tmp.join(format!("task-card.{ext}")).exists());
        }
        // §5.10 canonical surface markers.
        let prui = fs::read_to_string(tmp.join("task-card.prui")).unwrap();
        assert!(prui.contains("class=card, direction=column, gap=8"));
        assert!(prui.contains("on:click=$state.expanded = not state.expanded"));
        let prss = fs::read_to_string(tmp.join("task-card.prss")).unwrap();
        assert!(prss.contains("background = { tokens.colors.surface }"));
        let luau = fs::read_to_string(tmp.join("task-card.luau")).unwrap();
        assert!(luau.starts_with("--!strict"));
        // Refuse to clobber without --force.
        assert!(run(&args, &ws(), false).is_err());
        // Single-file variant.
        let sf = NewArgs {
            kind: NewKind::Widget(WidgetArgs {
                name: "badge".into(),
                dir: Some(tmp.clone()),
                single_file: true,
                force: false,
            }),
        };
        assert_eq!(run(&sf, &ws(), false).unwrap(), 0);
        let body = fs::read_to_string(tmp.join("badge.prui")).unwrap();
        assert!(body.contains("<script>") && body.contains("<style>"));
        assert!(body.contains("[class.badge]"));
        let _ = fs::remove_dir_all(&tmp);
    }
}
