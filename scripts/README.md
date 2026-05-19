# scripts/

Build and development automation scripts.

## Contents

### `install-cli.sh`

One-time installer for the unified `prism` CLI. Runs
`cargo install --path packages/prism-cli --force` so a real `prism`
binary lands on PATH (`~/.cargo/bin/prism`) and `prism dev shell`
works with no `cargo ` prefix. Idempotent — re-run after pulling CLI
changes. Zero-setup alternative (no install): `cargo prism dev shell`,
which uses the committed `[alias]` in `.cargo/config.toml`.

### `hooks/`

Claude Code hooks that run automatically during development sessions:

| Script | When | Purpose |
|--------|------|---------|
| `post-edit-check.sh` | After file edits | Reminds about tests, CLAUDE.md/README.md updates, lockfile refresh |
| `pre-stop-check.sh` | Before task completion | Runs `cargo fmt --check`, `cargo clippy`, `cargo test`, plus relay pnpm checks if relay files changed |
| `pre-bash-guard.sh` | Before bash commands | Safety guardrails for destructive operations |

These hooks are configured in `.claude/settings.json`.
