#!/usr/bin/env bash
# One-shot orchestrator that exercises every daemon build and test
# surface in sequence:
#
#   1. Host unit + integration tests under every feature combo
#      (default/full, mobile, embedded, wasm).
#   2. `cargo clippy -D warnings` under every feature combo.
#   3. `cargo fmt --check`.
#   4. WASM cross-compile (dev + prod) via scripts/build-wasm.sh.
#   5. Playwright E2E suite against both WASM profiles.
#   6. iOS cross-compile (device + simulator) via scripts/build-ios.sh,
#      with a symbol check confirming the C ABI is still exported.
#   7. Android cross-compile (all ABIs) via scripts/build-android.sh,
#      with a matching symbol check.
#
# The script hard-fails on the first failure but otherwise walks the
# whole matrix so a single run is a complete answer to "does this
# build / pass on every target".
#
# Usage:
#   ./scripts/test-all.sh                 # everything
#   ./scripts/test-all.sh --skip-mobile   # skip ios/android
#   ./scripts/test-all.sh --skip-e2e      # skip Playwright
#   ./scripts/test-all.sh --skip-wasm     # skip wasm cross-compile + e2e
#
# Skips compose: `--skip-mobile --skip-wasm` limits the run to host
# `cargo` coverage so you can iterate quickly on a plane.

set -euo pipefail

SKIP_MOBILE=0
SKIP_E2E=0
SKIP_WASM=0
for arg in "$@"; do
    case "$arg" in
        --skip-mobile) SKIP_MOBILE=1 ;;
        --skip-e2e)    SKIP_E2E=1 ;;
        --skip-wasm)   SKIP_WASM=1; SKIP_E2E=1 ;;
        -h|--help)
            sed -n '2,/^set/p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) echo "unknown arg: $arg" >&2; exit 1 ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DAEMON_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$DAEMON_DIR"

hr() { printf '\033[1;36m────── %s ──────\033[0m\n' "$*" >&2; }
ok() { printf '\033[1;32m✓\033[0m %s\n' "$*" >&2; }
err() { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }

# ── 1. Host cargo test matrix ──────────────────────────────────────────
hr "cargo test (default / full)"
cargo test
ok "default features green"

hr "cargo test --no-default-features --features mobile"
cargo test --no-default-features --features mobile
ok "mobile features green"

hr "cargo test --no-default-features --features embedded"
cargo test --no-default-features --features embedded
ok "embedded features green"

hr "cargo test --no-default-features --features wasm --lib"
cargo test --no-default-features --features wasm --lib
ok "wasm features green (host simulated)"

# ── 2. Clippy matrix ───────────────────────────────────────────────────
hr "cargo clippy --all-targets --all-features -- -D warnings"
cargo clippy --all-targets --all-features -- -D warnings
ok "clippy (all features) clean"

hr "cargo clippy --no-default-features --features mobile -- -D warnings"
cargo clippy --all-targets --no-default-features --features mobile -- -D warnings
ok "clippy (mobile) clean"

hr "cargo clippy --no-default-features --features embedded -- -D warnings"
cargo clippy --all-targets --no-default-features --features embedded -- -D warnings
ok "clippy (embedded) clean"

hr "cargo clippy --no-default-features --features wasm -- -D warnings"
cargo clippy --all-targets --no-default-features --features wasm -- -D warnings
ok "clippy (wasm) clean"

# ── 3. Formatting ──────────────────────────────────────────────────────
hr "cargo fmt --check"
cargo fmt --check
ok "rustfmt clean"

# ── 4. WASM cross-compile ──────────────────────────────────────────────
if [ "$SKIP_WASM" = "0" ]; then
    hr "wasm32-unknown-emscripten (dev)"
    "$SCRIPT_DIR/build-wasm.sh" dev
    ok "wasm dev build produced dist-wasm/dev/prism_daemon_wasm.{js,wasm}"

    hr "wasm32-unknown-emscripten (prod)"
    "$SCRIPT_DIR/build-wasm.sh" prod
    ok "wasm prod build produced dist-wasm/prod/prism_daemon_wasm.{js,wasm}"
fi

# ── 5. Playwright E2E ──────────────────────────────────────────────────
if [ "$SKIP_E2E" = "0" ]; then
    command -v pnpm >/dev/null 2>&1 || err "pnpm not on PATH"

    hr "playwright (dev profile)"
    DAEMON_WASM_PROFILE=dev pnpm exec playwright test --config=e2e/playwright.config.ts
    ok "playwright dev suite passed"

    hr "playwright (prod profile)"
    DAEMON_WASM_PROFILE=prod pnpm exec playwright test --config=e2e/playwright.config.ts
    ok "playwright prod suite passed"
fi

# ── 6. iOS cross-compile + symbol check ────────────────────────────────
if [ "$SKIP_MOBILE" = "0" ]; then
    if [ "$(uname -s)" = "Darwin" ]; then
        hr "ios xcframework (release)"
        "$SCRIPT_DIR/build-ios.sh"
        XCF="$DAEMON_DIR/mobile/ios/Frameworks/PrismDaemon.xcframework"
        IOS_LIB="$XCF/ios-arm64/libprism_daemon.a"
        [ -f "$IOS_LIB" ] || err "missing iOS device slice: $IOS_LIB"
        for sym in _prism_daemon_create _prism_daemon_destroy _prism_daemon_invoke _prism_daemon_free_string; do
            nm -g "$IOS_LIB" 2>/dev/null | grep -q " T $sym$" \
                || err "iOS slice missing C ABI symbol: $sym"
        done
        ok "iOS xcframework built + C ABI symbols exported"
    else
        hr "ios skipped (host is not macOS)"
    fi

    # ── 7. Android cross-compile + symbol check ────────────────────────
    hr "android cdylibs (debug)"
    "$SCRIPT_DIR/build-android.sh" debug
    JNI_DIR="$DAEMON_DIR/mobile/android/src/main/jniLibs"
    for abi in arm64-v8a armeabi-v7a x86_64; do
        SO="$JNI_DIR/$abi/libprism_daemon.so"
        [ -f "$SO" ] || err "missing Android slice: $SO"
        for sym in prism_daemon_create prism_daemon_destroy prism_daemon_invoke prism_daemon_free_string; do
            nm -g "$SO" 2>/dev/null | grep -q " T $sym$" \
                || err "Android $abi missing C ABI symbol: $sym"
        done
    done
    ok "Android cdylibs built + C ABI symbols exported on every ABI"
fi

printf '\n\033[1;32mAll surfaces green.\033[0m\n'
