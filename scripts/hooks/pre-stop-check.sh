#!/usr/bin/env bash
# Hook: Stop
# Before Claude considers a task complete: fmt (auto-fix), clippy, cargo test.
# Exit 2 = blocking (Claude must fix). All output goes to stderr.

export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

CHANGED=$(git diff --name-only HEAD 2>/dev/null || true)
CHANGED_RUST=$(echo "$CHANGED" | grep -E '\.rs$|/Cargo\.(toml|lock)$' || true)

if [ -z "$CHANGED_RUST" ] && [ -z "$CHANGED" ]; then
  echo "No changes detected — skipping checks." >&2
  exit 0
fi

# ── Rust: auto-format, then verify ──────────────────────────────────────────
if [ -n "$CHANGED_RUST" ]; then
  echo "--- cargo fmt --all ---" >&2
  cargo fmt --all >&2 2>&1

  echo "--- cargo fmt --check (verify) ---" >&2
  if ! cargo fmt --all -- --check >&2 2>&1; then
    echo "FAIL: rustfmt diffs remain after auto-format." >&2
    exit 2
  fi

  echo "--- cargo clippy ---" >&2
  if ! cargo clippy --workspace --all-targets -- -D warnings >&2 2>&1; then
    echo "FAIL: clippy errors." >&2
    exit 2
  fi

  echo "--- cargo test ---" >&2
  if ! cargo test --workspace >&2 2>&1; then
    echo "FAIL: cargo test failed." >&2
    exit 2
  fi
fi

echo "All pre-stop checks passed." >&2
