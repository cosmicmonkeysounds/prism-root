#!/usr/bin/env bash
# Cross-compile prism-daemon to wasm32-unknown-emscripten and stage the
# resulting .js/.wasm pair into dist-wasm/{dev,prod}/ alongside the
# Playwright E2E harness.
#
# Usage:
#   ./scripts/build-wasm.sh           # dev profile (default)
#   ./scripts/build-wasm.sh dev
#   ./scripts/build-wasm.sh prod
#
# Dev  = `cargo build`           → debug info, assertions, bigger.
# Prod = `cargo build --release` → LTO, no assertions, shipping.
#
# Both profiles end up in their own subfolder so Playwright can test
# them independently (`test:e2e:dev` vs `test:e2e:prod`).

set -euo pipefail

PROFILE="${1:-dev}"

case "$PROFILE" in
    dev|prod) ;;
    *) echo "usage: $0 [dev|prod]" >&2; exit 1 ;;
esac

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DAEMON_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
EMSDK_DIR="$DAEMON_DIR/.emsdk"
DIST_DIR="$DAEMON_DIR/dist-wasm/$PROFILE"
HARNESS_SRC="$DAEMON_DIR/e2e/fixtures/harness.html"

log() { printf '\033[1;34m[build-wasm:%s]\033[0m %s\n' "$PROFILE" "$*" >&2; }
err() { printf '\033[1;31m[build-wasm:%s]\033[0m %s\n' "$PROFILE" "$*" >&2; exit 1; }

# ── 1. Make sure emsdk is installed + sourced ──────────────────────────
if [ ! -f "$EMSDK_DIR/emsdk_env.sh" ]; then
    log "emsdk not found, running setup-wasm.sh"
    "$SCRIPT_DIR/setup-wasm.sh"
fi

# shellcheck disable=SC1091
source "$EMSDK_DIR/emsdk_env.sh" >/dev/null

command -v emcc >/dev/null 2>&1 || err "emcc not on PATH after sourcing emsdk_env.sh"
log "using $(emcc --version | head -n1)"

# ── 2. Cargo build ──────────────────────────────────────────────────────
# `--no-default-features --features wasm` strips watcher/build/cli and
# keeps only crdt + lua + the C-ABI adapter.
# The .cargo/config.toml for target.wasm32-unknown-emscripten supplies
# the -sMODULARIZE / EXPORTED_FUNCTIONS / EXPORT_ES6 link args.

CARGO_FLAGS=(
    --target wasm32-unknown-emscripten
    --no-default-features
    --features wasm
    --bin prism_daemon_wasm
)

# `-fwasm-exceptions` is forced into every emcc invocation because rustc
# unconditionally passes it for the wasm32-unknown-emscripten target, and
# lua-src's build.rs hard-codes `-fexceptions` when compiling Lua as C++
# for emscripten. Those two EH ABIs are incompatible at link time (emcc
# fails with "invoke_ functions exported but exceptions and longjmp are
# both disabled"). Forcing wasm-exceptions for every C/C++ TU — including
# the vendored Lua sources — makes everything speak the same EH dialect.
BASE_EMCC_CFLAGS="-fwasm-exceptions"

# Dev builds layer assertions on top of the pinned link args so
# `abort("OOB")` etc. surface with a readable message in the browser
# console. Prod builds leave assertions off so the runtime glue stays
# small.
if [ "$PROFILE" = "dev" ]; then
    export EMCC_CFLAGS="${EMCC_CFLAGS:-} $BASE_EMCC_CFLAGS -sASSERTIONS=2"
else
    CARGO_FLAGS+=(--release)
    export EMCC_CFLAGS="${EMCC_CFLAGS:-} $BASE_EMCC_CFLAGS -O3"
fi

log "cargo build ${CARGO_FLAGS[*]}"
(cd "$DAEMON_DIR" && cargo build "${CARGO_FLAGS[@]}")

# ── 3. Locate and stage the outputs ────────────────────────────────────
# Rust with `[[bin]] name = "prism_daemon_wasm"` on this target produces
# `prism_daemon_wasm.js` + `prism_daemon_wasm.wasm` under
# target/wasm32-unknown-emscripten/{debug,release}/.
if [ "$PROFILE" = "dev" ]; then
    TARGET_SUBDIR="debug"
else
    TARGET_SUBDIR="release"
fi
TARGET_DIR="$DAEMON_DIR/target/wasm32-unknown-emscripten/$TARGET_SUBDIR"

JS_SRC="$TARGET_DIR/prism_daemon_wasm.js"
WASM_SRC="$TARGET_DIR/prism_daemon_wasm.wasm"

[ -f "$JS_SRC" ]   || err "expected $JS_SRC after build — emcc did not emit JS glue"
[ -f "$WASM_SRC" ] || err "expected $WASM_SRC after build"

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

# Copy the emcc-produced pair verbatim — we intentionally keep the
# `prism_daemon_wasm.{js,wasm}` name because the JS glue hardcodes the
# `.wasm` filename into its fetch path. Renaming only the JS would
# leave the glue looking for `prism_daemon_wasm.wasm` and 404ing in
# the browser, which is how we learned this the hard way.
cp "$JS_SRC"   "$DIST_DIR/prism_daemon_wasm.js"
cp "$WASM_SRC" "$DIST_DIR/prism_daemon_wasm.wasm"

if [ -f "$HARNESS_SRC" ]; then
    cp "$HARNESS_SRC" "$DIST_DIR/harness.html"
else
    log "harness.html not found at $HARNESS_SRC (skipping copy)"
fi

# ── 4. Report ──────────────────────────────────────────────────────────
WASM_SIZE="$(wc -c < "$DIST_DIR/prism_daemon_wasm.wasm" | tr -d ' ')"
JS_SIZE="$(wc -c < "$DIST_DIR/prism_daemon_wasm.js" | tr -d ' ')"
log "artifacts staged in $DIST_DIR"
log "  prism_daemon_wasm.wasm — $WASM_SIZE bytes"
log "  prism_daemon_wasm.js   — $JS_SIZE bytes"
log "done."
