use std::process::ExitCode;

use clap::Parser;
use prism_cli::{commands, Cli};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let workspace = match prism_cli::Workspace::discover() {
        Ok(ws) => ws,
        Err(err) => {
            eprintln!("prism: failed to locate workspace root: {err:#}");
            return ExitCode::from(2);
        }
    };

    match commands::run(&cli, &workspace) {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            eprintln!("prism: {err:#}");
            ExitCode::from(1)
        }
    }
}
