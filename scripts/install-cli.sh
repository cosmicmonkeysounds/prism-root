#!/usr/bin/env bash
# Install the unified `prism` CLI as a real binary on PATH.
#
# After this, `prism dev shell` (no `cargo ` prefix) works from
# anywhere. Idempotent — safe to re-run after pulling CLI changes.
#
# Zero-setup alternative (no install, works the instant you clone):
#   cargo prism dev shell      # uses the [alias] in .cargo/config.toml
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "Installing prism CLI from ${repo_root}/packages/prism-cli ..."
cargo install --path "${repo_root}/packages/prism-cli" --force

bin="$(cargo install --list 2>/dev/null | grep -A1 '^prism-cli ' | tail -1 | xargs || true)"
cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"

echo
echo "Installed: ${cargo_bin}/prism"
if ! command -v prism >/dev/null 2>&1; then
  echo
  echo "WARNING: '${cargo_bin}' is not on your PATH."
  echo "Add this to your shell profile (~/.zshrc or ~/.bashrc):"
  echo "    export PATH=\"${cargo_bin}:\$PATH\""
else
  echo "Verified: $(command -v prism)"
fi
