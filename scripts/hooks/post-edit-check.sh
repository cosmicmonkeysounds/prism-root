#!/usr/bin/env bash
# Hook: PostToolUse (Write|Edit|MultiEdit)
# Immediate feedback after every file edit: tests, docs, mocks, e2e, deps, ADRs.

TOOL_INPUT="$1"

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

# Extract file path from tool input (macOS grep -E, no -P)
FILE_PATH=$(echo "$TOOL_INPUT" \
  | grep -oE '"file_path"[[:space:]]*:[[:space:]]*"[^"]+"' \
  | head -1 \
  | sed 's/"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)"/\1/' || true)

if [ -z "$FILE_PATH" ]; then
  exit 0
fi

# ── SPEC.md → ADR reminder ────────────────────────────────────────────────────
if [[ "$FILE_PATH" == */SPEC.md ]]; then
  echo "REMINDER: SPEC.md changed — does this decision need a new or updated ADR in docs/adr/?"
  exit 0
fi

# ── package.json / Cargo.toml → lockfile reminder ─────────────────────────────
BASENAME=$(basename "$FILE_PATH")
if [[ "$BASENAME" == "package.json" ]]; then
  echo "REMINDER: package.json changed — run 'pnpm install' to update pnpm-lock.yaml."
  exit 0
fi
if [[ "$BASENAME" == "Cargo.toml" ]]; then
  echo "REMINDER: Cargo.toml changed — run 'cargo fetch' in packages/prism-daemon to update Cargo.lock."
  exit 0
fi

# ── TypeScript source files ───────────────────────────────────────────────────
if [[ "$FILE_PATH" == *.ts || "$FILE_PATH" == *.tsx ]] && \
   [[ "$FILE_PATH" != *.test.* ]] && \
   [[ "$FILE_PATH" != *.config.* ]] && \
   [[ "$FILE_PATH" != *.d.ts ]] && \
   [[ "$FILE_PATH" == */src/* ]]; then

  EXT="${FILE_PATH##*.}"
  BASE="${FILE_PATH%.*}"
  TEST_PATH="${BASE}.test.${EXT}"

  if [ -f "$TEST_PATH" ]; then
    echo "REMINDER: Update existing tests at: $TEST_PATH"
  else
    echo "REMINDER: Add tests at: $TEST_PATH"
  fi

  # Walk up to package root reminding about README.md and CLAUDE.md
  PKG_DIR=$(echo "$FILE_PATH" | grep -oE "^.+/packages/[^/]+" || true)
  DIR="$(dirname "$FILE_PATH")"
  while [[ "$DIR" != "/" ]]; do
    [ -f "$DIR/README.md" ] && echo "REMINDER: Update $DIR/README.md if behaviour/API changed."
    [ -f "$DIR/CLAUDE.md" ] && echo "REMINDER: Update $DIR/CLAUDE.md if behaviour/API changed."
    [[ "$DIR" == "$PKG_DIR" ]] && break
    DIR="$(dirname "$DIR")"
  done

  if [ -f "$REPO_ROOT/docs/dev/current-plan.md" ]; then
    echo "REMINDER: Update docs/dev/current-plan.md to reflect progress."
  fi

  # Playwright coverage check for UI components
  if [[ "$FILE_PATH" == *.tsx ]]; then
    COMPONENT=$(basename "$FILE_PATH" .tsx)
    E2E_HIT=$(find "$REPO_ROOT/e2e" -name "*.spec.*" -exec grep -l "$COMPONENT" {} \; 2>/dev/null || true)
    if [ -z "$E2E_HIT" ]; then
      echo "REMINDER: No Playwright spec references '$COMPONENT'. Add e2e coverage in e2e/tests/."
    fi
  fi
fi

# ── Test files: mock audit ────────────────────────────────────────────────────
if [[ "$FILE_PATH" == *.test.ts || "$FILE_PATH" == *.test.tsx ]] && [ -f "$FILE_PATH" ]; then
  if grep -qE "^[[:space:]]*(vi|jest)\.mock\(" "$FILE_PATH" 2>/dev/null; then
    echo "WARNING: $FILE_PATH uses module-level mocks. Prefer real implementations or in-memory fakes. Only mock at the boundary (e.g. Tauri IPC, native APIs)."
  fi
fi
