#!/usr/bin/env bash
# Hook: Stop
# Before Claude considers a task complete: fmt, clippy, cargo test.
# Relay changes additionally run the relay's pnpm scripts.
# Exit 2 = blocking (Claude must fix). All output goes to stderr.

export PATH="$HOME/.local/share/pnpm:$HOME/.volta/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

CHANGED=$(git diff --name-only HEAD 2>/dev/null || true)
CHANGED_RUST=$(echo "$CHANGED" | grep -E '\.rs$|/Cargo\.(toml|lock)$' || true)
CHANGED_RELAY=$(echo "$CHANGED" | grep -E '^packages/prism-relay/.*\.(ts|tsx|js|jsx|mjs|cjs|json)$' || true)

# ── Rust: fmt ────────────────────────────────────────────────────────────────
if [ -n "$CHANGED_RUST" ] || [ -z "$CHANGED" ]; then
  echo "--- cargo fmt --check ---" >&2
  if ! cargo fmt --all -- --check >&2 2>&1; then
    echo "FAIL: rustfmt diffs. Run 'cargo fmt --all'." >&2
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

# ── Relay (Hono JSX SSR, still JS) ───────────────────────────────────────────
if [ -n "$CHANGED_RELAY" ]; then
  echo "--- relay typecheck ---" >&2
  if ! pnpm --filter @prism/relay typecheck >&2 2>&1; then
    echo "FAIL: relay typecheck errors." >&2
    exit 2
  fi

  echo "--- relay lint ---" >&2
  if ! pnpm --filter @prism/relay lint >&2 2>&1; then
    echo "FAIL: relay lint errors." >&2
    exit 2
  fi
fi

echo "All pre-stop checks passed." >&2
