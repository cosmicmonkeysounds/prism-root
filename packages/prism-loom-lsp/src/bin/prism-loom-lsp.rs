//! `prism-loom-lsp` binary entry point — a thin shell over the LSP
//! library. Reads JSON-RPC from stdin, writes responses to stdout.

use std::process::ExitCode;

fn main() -> ExitCode {
    match prism_loom_lsp::run_stdio() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("prism-loom-lsp: {err:#}");
            ExitCode::from(1)
        }
    }
}
