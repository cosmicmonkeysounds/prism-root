#!/usr/bin/env bash
# Hook: PreToolUse (Bash)
# Block dangerous shell commands before they execute.
# Exit 2 = blocking (Claude must not proceed).

TOOL_INPUT="$1"

# Extract the command string from tool input JSON (macOS grep -E, no -P)
COMMAND=$(echo "$TOOL_INPUT" \
  | grep -oE '"command"[[:space:]]*:[[:space:]]*"[^"]+"' \
  | head -1 \
  | sed 's/"command"[[:space:]]*:[[:space:]]*"\(.*\)"/\1/' || true)

if [ -z "$COMMAND" ]; then
  exit 0
fi

fail() {
  echo "BLOCKED: $1" >&2
  echo "Command was: $COMMAND" >&2
  exit 2
}

# Destructive filesystem operations
if echo "$COMMAND" | grep -qE "rm[[:space:]]+-[a-zA-Z]*rf|rm[[:space:]]+-[a-zA-Z]*f[a-zA-Z]*[[:space:]]"; then
  fail "Destructive 'rm -rf' or 'rm -f' detected. Confirm with the user before deleting files."
fi

# Force git operations
if echo "$COMMAND" | grep -qE "git[[:space:]]+push[[:space:]]+.*--force|git[[:space:]]+push[[:space:]].*-f([[:space:]]|$)"; then
  fail "Force push detected. This can overwrite upstream history. Get explicit user confirmation."
fi

if echo "$COMMAND" | grep -qE "git[[:space:]]+reset[[:space:]]+--hard"; then
  fail "'git reset --hard' discards uncommitted work. Get explicit user confirmation."
fi

if echo "$COMMAND" | grep -qE "git[[:space:]]+clean[[:space:]]+-[a-zA-Z]*f"; then
  fail "'git clean -f' permanently deletes untracked files. Get explicit user confirmation."
fi

if echo "$COMMAND" | grep -qE "git[[:space:]]+checkout[[:space:]]+--[[:space:]]"; then
  fail "'git checkout --' discards working-tree changes. Get explicit user confirmation."
fi

# Bypassing hooks
if echo "$COMMAND" | grep -qE "git[[:space:]]+(commit|push).*--no-verify"; then
  fail "'--no-verify' skips hooks. Never bypass hooks — fix the underlying issue instead."
fi

# Dropping databases / truncating data
if echo "$COMMAND" | grep -qiE "drop[[:space:]]+database|drop[[:space:]]+table|truncate[[:space:]]+table"; then
  fail "Destructive SQL operation detected. Get explicit user confirmation."
fi

# Package removals
if echo "$COMMAND" | grep -qE "pnpm[[:space:]]+remove|npm[[:space:]]+uninstall|yarn[[:space:]]+remove"; then
  fail "Package removal detected. Confirm the dependency should be removed."
fi

exit 0
