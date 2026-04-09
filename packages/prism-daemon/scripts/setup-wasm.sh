#!/usr/bin/env bash
# Install everything prism-daemon needs to cross-compile to
# wasm32-unknown-emscripten: the emscripten SDK (pinned) and the rustup
# target. Idempotent — running it twice is a no-op after the first time.
#
# Usage:
#   ./scripts/setup-wasm.sh          # install if missing, then exit
#   source ./scripts/setup-wasm.sh   # install + leave emsdk env active
#
# The emsdk lives in ./.emsdk/ (gitignored) so each clone of the repo
# gets its own, reproducible toolchain. We pin to a known-good version
# instead of tracking `latest` so the build doesn't silently drift.

set -euo pipefail

# ── Config ──────────────────────────────────────────────────────────────
# Pinned emscripten version. Bump deliberately and re-run e2e.
EMSDK_VERSION="${EMSDK_VERSION:-3.1.74}"

# Resolve paths relative to this script so it works from anywhere.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DAEMON_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
EMSDK_DIR="$DAEMON_DIR/.emsdk"

# ── Helpers ─────────────────────────────────────────────────────────────
log() { printf '\033[1;34m[setup-wasm]\033[0m %s\n' "$*" >&2; }
err() { printf '\033[1;31m[setup-wasm]\033[0m %s\n' "$*" >&2; exit 1; }

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || err "required tool missing: $1"
}

need_cmd git
need_cmd rustup
need_cmd cargo

# ── 1. Rustup target ────────────────────────────────────────────────────
if ! rustup target list --installed | grep -q '^wasm32-unknown-emscripten$'; then
    log "installing rustup target: wasm32-unknown-emscripten"
    rustup target add wasm32-unknown-emscripten
else
    log "rustup target wasm32-unknown-emscripten already installed"
fi

# ── 2. emsdk clone ──────────────────────────────────────────────────────
if [ ! -d "$EMSDK_DIR/.git" ]; then
    log "cloning emsdk into $EMSDK_DIR"
    git clone --depth=1 https://github.com/emscripten-core/emsdk.git "$EMSDK_DIR"
else
    log "emsdk clone already present at $EMSDK_DIR"
fi

# ── 3. emsdk version install + activate ─────────────────────────────────
# `emsdk install <ver>` downloads the sdk tarball for that version; it's
# a no-op if already downloaded. `emsdk activate <ver>` rewrites the
# .emscripten config. We do both on every run because they're cheap.
pushd "$EMSDK_DIR" >/dev/null
log "installing emsdk $EMSDK_VERSION (cached after first run)"
./emsdk install "$EMSDK_VERSION" >/dev/null
log "activating emsdk $EMSDK_VERSION"
./emsdk activate "$EMSDK_VERSION" >/dev/null
popd >/dev/null

# ── 4. Source the env into the caller's shell (if sourced) ─────────────
# `emsdk_env.sh` sets PATH, EMSDK, EM_CONFIG, etc. so `emcc`, `emar`,
# etc. are on PATH for `cargo build` to pick up.
# shellcheck disable=SC1091
source "$EMSDK_DIR/emsdk_env.sh" >/dev/null 2>&1 || true

if command -v emcc >/dev/null 2>&1; then
    EMCC_VER="$(emcc --version | head -n1)"
    log "ready: $EMCC_VER"
else
    log "warning: emcc not on PATH in this shell"
    log "run: source '$EMSDK_DIR/emsdk_env.sh'"
    log "or invoke scripts/build-wasm.sh which sources it for you"
fi

log "done."
