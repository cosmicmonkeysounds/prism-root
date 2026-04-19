#!/usr/bin/env bash
# Hook: PostToolUse (Write|Edit|MultiEdit)
# Immediate feedback after every file edit. Auto-formats Rust files.

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

# ── Slint migration plan → cross-reference reminder ─────────────────────────
if [[ "$FILE_PATH" == */slint-migration-plan.md ]]; then
  echo "REMINDER: slint-migration-plan.md changed — update CLAUDE.md if the plan's phasing or decisions moved."
  exit 0
fi

# ── Cargo.toml → lockfile + workspace reminder ──────────────────────────────
if [[ "$BASENAME" == "Cargo.toml" ]]; then
  echo "REMINDER: Cargo.toml changed — run 'cargo fetch' to update Cargo.lock, and confirm the workspace root at /Cargo.toml still resolves."
  exit 0
fi

# ── .slint files → compile check reminder ────────────────────────────────────
if [[ "$FILE_PATH" == *.slint ]]; then
  echo "REMINDER: .slint file changed — run 'cargo check -p prism-shell' to verify Slint compilation."
  exit 0
fi

# ── Rust source: auto-format + reminders ─────────────────────────────────────
if [[ "$FILE_PATH" == *.rs ]] && [[ "$FILE_PATH" != *target/* ]]; then
  # Auto-format the edited file
  if command -v rustfmt >/dev/null 2>&1; then
    rustfmt "$FILE_PATH" 2>/dev/null && echo "Auto-formatted: $BASENAME"
  fi

  PKG_DIR=$(echo "$FILE_PATH" | grep -oE "^.+/packages/[^/]+" || true)
  if [ -n "$PKG_DIR" ]; then
    CRATE=$(basename "$PKG_DIR")
    echo "REMINDER: Rust source changed in $CRATE — run 'cargo test -p $CRATE' and 'cargo clippy -p $CRATE -- -D warnings'."
    [ -f "$PKG_DIR/CLAUDE.md" ] && echo "REMINDER: Update $PKG_DIR/CLAUDE.md if behaviour/API changed."
  fi
fi
