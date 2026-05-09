//! Native dev binary — boots `prism_shell::Shell` on
//! `prism_ui_runtime`'s femtovg backend.
//!
//! Legacy CLI flags (`--app`, `--panel`, `--scene`, `--viewport`,
//! `--zoom`, `--screenshot`, `--e2e*`) are off the table while §17's
//! per-feature ports land. See `docs/dev/clay-migration-plan.md` §17.

use prism_shell::Shell;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let shell = Shell::new()?;
    shell.run()
}
