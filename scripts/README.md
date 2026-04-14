# scripts/

Build and development automation scripts.

## Contents

### `hooks/`

Claude Code hooks that run automatically during development sessions:

| Script | When | Purpose |
|--------|------|---------|
| `post-edit-check.sh` | After file edits | Reminds about tests, CLAUDE.md/README.md updates, lockfile refresh |
| `pre-stop-check.sh` | Before task completion | Runs `cargo fmt --check`, `cargo clippy`, `cargo test`, plus relay pnpm checks if relay files changed |
| `pre-bash-guard.sh` | Before bash commands | Safety guardrails for destructive operations |

These hooks are configured in `.claude/settings.json`.
