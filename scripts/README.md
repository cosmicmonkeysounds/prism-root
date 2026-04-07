# scripts/

Build and development automation scripts.

## Contents

### `hooks/`

Claude Code hooks that run automatically during development sessions:

| Script | When | Purpose |
|--------|------|---------|
| `post-edit-check.sh` | After file edits | Reminds about updating test files |
| `pre-stop-check.sh` | Before task completion | Runs `pnpm typecheck` + `pnpm lint` |
| `pre-bash-guard.sh` | Before bash commands | Safety guardrails for destructive operations |

These hooks are configured in `.claude/settings.json`.
