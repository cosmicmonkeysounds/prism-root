#!/usr/bin/env bash
# Hook: Stop
# Before Claude considers a task complete: typecheck, lint, unit tests, and audits.
# Exit 2 = blocking (Claude must fix). All output goes to stderr so Claude sees it.

export PATH="$HOME/.local/share/pnpm:$HOME/.volta/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

# ── TypeScript ────────────────────────────────────────────────────────────────
echo "--- TypeScript ---" >&2
if ! pnpm typecheck >&2 2>&1; then
  echo "FAIL: TypeScript errors found. Fix before completing." >&2
  exit 2
fi

# ── ESLint ────────────────────────────────────────────────────────────────────
echo "--- ESLint ---" >&2
if ! pnpm lint >&2 2>&1; then
  echo "FAIL: Lint errors found. Fix before completing." >&2
  exit 2
fi

# ── Unit tests ────────────────────────────────────────────────────────────────
echo "--- Unit tests ---" >&2
if ! pnpm test --reporter=verbose >&2 2>&1; then
  echo "FAIL: Unit tests failed. Fix before completing." >&2
  exit 2
fi

# ── `any` type guard ──────────────────────────────────────────────────────────
echo "--- any type audit ---" >&2
ANY_HITS=$(git diff HEAD -- "*.ts" "*.tsx" 2>/dev/null \
  | grep -E "^\+" \
  | grep -v "^+++" \
  | grep -E ":[[:space:]]*any[[:space:]]*[,;>|)=]|as[[:space:]]+any[[:space:]]*[,;>|)=]" || true)
if [ -n "$ANY_HITS" ]; then
  echo "FAIL: Explicit 'any' type introduced. Use proper types or 'unknown':" >&2
  echo "$ANY_HITS" >&2
  exit 2
fi

# ── Cross-package relative import guard ───────────────────────────────────────
echo "--- Cross-package import audit ---" >&2
# Resolve each ../../ import and fail only if it escapes the file's own package root.
BAD_IMPORTS=""
while IFS= read -r hit; do
  FILE=$(echo "$hit" | cut -d: -f1)
  REL=$(echo "$hit" \
    | grep -oE "from ['\"][^'\"]+['\"]" \
    | sed "s/from ['\"]\\([^'\"]*\\)['\"].*/\\1/")
  [ -z "$REL" ] && continue
  RESOLVED=$(realpath "$(dirname "$FILE")/$REL" 2>/dev/null || true)
  PKG_ROOT=$(echo "$FILE" | grep -oE "^.+/packages/[^/]+")
  if [ -n "$RESOLVED" ] && [ -n "$PKG_ROOT" ] && [[ "$RESOLVED" != "$PKG_ROOT"/* ]]; then
    BAD_IMPORTS="$BAD_IMPORTS\n$hit"
  fi
done < <(grep -rn --include="*.ts" --include="*.tsx" \
  -E "from ['\"](\.\./){2,}" \
  "$REPO_ROOT/packages" 2>/dev/null \
  | grep -v "node_modules" | grep -v "\.test\.")
if [ -n "$BAD_IMPORTS" ]; then
  echo "FAIL: Cross-package relative imports found. Use @prism/* aliases instead:" >&2
  echo -e "$BAD_IMPORTS" >&2
  exit 2
fi

# ── Debug artifact scan ───────────────────────────────────────────────────────
echo "--- Debug artifact scan ---" >&2
DEBUG_HITS=$(grep -rn --include="*.ts" --include="*.tsx" \
  -E "console\.(log|warn|error|debug)\(|debugger;" \
  "$REPO_ROOT/packages" 2>/dev/null \
  | grep -v "node_modules" \
  | grep -v "\.emsdk/" \
  | grep -v "\.test\." \
  | grep -v "\.spec\." \
  | grep -v "//.*console\." \
  | grep -vE "['\"].*console\.(log|warn|error|debug)" \
  || true)
if [ -n "$DEBUG_HITS" ]; then
  echo "FAIL: Debug statements found in source (non-test) files. Remove before completing:" >&2
  echo "$DEBUG_HITS" >&2
  exit 2
fi

TODO_HITS=$(git diff HEAD -- "*.ts" "*.tsx" 2>/dev/null \
  | grep -E "^\+" \
  | grep -v "^+++" \
  | grep -iE "[[:space:]](TODO|FIXME|HACK|XXX)([[:space:]]|:)" || true)
if [ -n "$TODO_HITS" ]; then
  echo "WARNING: TODO/FIXME/HACK markers introduced in this session:" >&2
  echo "$TODO_HITS" >&2
fi

# ── Mock audit ────────────────────────────────────────────────────────────────
echo "--- Mock audit ---" >&2
MOCK_FILES=$(grep -rl --include="*.test.*" \
  -E "^[[:space:]]*(vi|jest)\.mock\(" \
  "$REPO_ROOT/packages" 2>/dev/null || true)
if [ -n "$MOCK_FILES" ]; then
  echo "WARNING: Module-level mocks found. Replace with real implementations or in-memory fakes where possible:" >&2
  echo "$MOCK_FILES" >&2
fi

# ── Rust check ───────────────────────────────────────────────────────────────
echo "--- Rust check ---" >&2
CHANGED_RUST=$(git diff --name-only HEAD 2>/dev/null | grep -E "\.(rs)$|Cargo\.(toml|lock)$" || true)
if [ -n "$CHANGED_RUST" ]; then
  echo "REMINDER: Rust files changed — run the following before final sign-off:" >&2
  echo "  cd packages/prism-daemon && cargo clippy -- -D warnings && cargo test" >&2
  echo "Changed: $CHANGED_RUST" >&2
fi

# ── Playwright reminder ───────────────────────────────────────────────────────
echo "--- Playwright reminder ---" >&2
CHANGED_TSX=$(git diff --name-only HEAD 2>/dev/null | grep "\.tsx$" || true)
if [ -n "$CHANGED_TSX" ]; then
  echo "REMINDER: UI files changed — run 'pnpm test:e2e' to verify with Playwright:" >&2
  echo "$CHANGED_TSX" >&2
  while IFS= read -r f; do
    COMPONENT=$(basename "$f" .tsx)
    E2E_HIT=$(find "$REPO_ROOT/e2e" -name "*.spec.*" -exec grep -l "$COMPONENT" {} \; 2>/dev/null || true)
    if [ -z "$E2E_HIT" ]; then
      echo "  WARNING: No e2e spec references '$COMPONENT' — add a Playwright test in e2e/tests/." >&2
    fi
  done <<< "$CHANGED_TSX"
fi

echo "All pre-stop checks passed." >&2
