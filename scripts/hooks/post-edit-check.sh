#!/usr/bin/env bash
# Hook: PostToolUse (Write|Edit|MultiEdit)
# Immediate feedback after every file edit.

TOOL_INPUT="$1"

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

FILE_PATH=$(echo "$TOOL_INPUT" \
  | grep -oE '"file_path"[[:space:]]*:[[:space:]]*"[^"]+"' \
  | head -1 \
  | sed 's/"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)"/\1/' || true)

if [ -z "$FILE_PATH" ]; then
  exit 0
fi

BASENAME=$(basename "$FILE_PATH")

# ── Clay migration plan → cross-reference reminder ──────────────────────────
if [[ "$FILE_PATH" == */clay-migration-plan.md ]]; then
  echo "REMINDER: clay-migration-plan.md changed — update CLAUDE.md if the plan's phasing or decisions moved."
  exit 0
fi

# ── Cargo.toml → lockfile + workspace reminder ───────────────────────────────
if [[ "$BASENAME" == "Cargo.toml" ]]; then
  echo "REMINDER: Cargo.toml changed — run 'cargo fetch' to update Cargo.lock, and confirm the workspace root at /Cargo.toml still resolves."
  exit 0
fi

# ── Relay package.json → pnpm install reminder ───────────────────────────────
if [[ "$FILE_PATH" == */prism-relay/package.json ]]; then
  echo "REMINDER: relay package.json changed — run 'pnpm install --filter @prism/relay' to refresh pnpm-lock.yaml."
  exit 0
fi

# ── Rust source ──────────────────────────────────────────────────────────────
if [[ "$FILE_PATH" == *.rs ]] && [[ "$FILE_PATH" != *target/* ]]; then
  PKG_DIR=$(echo "$FILE_PATH" | grep -oE "^.+/packages/[^/]+" || true)
  if [ -n "$PKG_DIR" ]; then
    CRATE=$(basename "$PKG_DIR")
    echo "REMINDER: Rust source changed in $CRATE — run 'cargo test -p $CRATE' and 'cargo clippy -p $CRATE -- -D warnings'."
    [ -f "$PKG_DIR/CLAUDE.md" ] && echo "REMINDER: Update $PKG_DIR/CLAUDE.md if behaviour/API changed."
    [ -f "$PKG_DIR/README.md" ] && echo "REMINDER: Update $PKG_DIR/README.md if behaviour/API changed."
  fi
fi

# ── Relay TS source ──────────────────────────────────────────────────────────
if [[ "$FILE_PATH" == *prism-relay*.ts || "$FILE_PATH" == *prism-relay*.tsx ]] && \
   [[ "$FILE_PATH" != *.test.* ]] && \
   [[ "$FILE_PATH" != *.d.ts ]]; then
  EXT="${FILE_PATH##*.}"
  BASE="${FILE_PATH%.*}"
  TEST_PATH="${BASE}.test.${EXT}"
  if [ -f "$TEST_PATH" ]; then
    echo "REMINDER: Update existing tests at: $TEST_PATH"
  else
    echo "REMINDER: Add tests at: $TEST_PATH"
  fi
fi
