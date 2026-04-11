#!/usr/bin/env bash
# Cross-compile prism-daemon for iOS (device + simulator) and assemble
# a PrismDaemon.xcframework that the Capacitor plugin in
# packages/prism-capacitor-daemon consumes as a binary target.
#
# The xcframework bundles two slices:
#
#   - aarch64-apple-ios       (physical arm64 iPhone/iPad)
#   - aarch64-apple-ios-sim   (arm64 iOS Simulator on Apple Silicon)
#
# Both are built with `--no-default-features --features mobile`, which
# pulls in CRDT + Luau and the C ABI adapter, but leaves out `watcher`
# (no inotify in iOS) and `build` (process spawning is banned by the
# App Store sandbox).
#
# Usage:  ./scripts/build-ios.sh
#
# Assumes `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`
# has already been run.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DAEMON_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PLUGIN_DIR="$(cd "$DAEMON_DIR/../prism-capacitor-daemon" && pwd)"

FEATURE_FLAGS=(--no-default-features --features mobile)
CARGO_FLAGS=(build --release "${FEATURE_FLAGS[@]}")

log() { printf '\033[1;34m[build-ios]\033[0m %s\n' "$*" >&2; }

log "building device slice (aarch64-apple-ios)"
(cd "$DAEMON_DIR" && cargo "${CARGO_FLAGS[@]}" --target aarch64-apple-ios)

log "building simulator slice (aarch64-apple-ios-sim)"
(cd "$DAEMON_DIR" && cargo "${CARGO_FLAGS[@]}" --target aarch64-apple-ios-sim)

DEVICE_A="$DAEMON_DIR/target/aarch64-apple-ios/release/libprism_daemon.a"
SIM_A="$DAEMON_DIR/target/aarch64-apple-ios-sim/release/libprism_daemon.a"

for lib in "$DEVICE_A" "$SIM_A"; do
    [ -f "$lib" ] || { echo "missing static lib: $lib" >&2; exit 1; }
done

# Stage the headers next to each static library so xcodebuild's
# `-headers` argument can pick them up into the xcframework.
STAGE_DEVICE="$DAEMON_DIR/target/xcframework-stage/device"
STAGE_SIM="$DAEMON_DIR/target/xcframework-stage/sim"
rm -rf "$STAGE_DEVICE" "$STAGE_SIM"
mkdir -p "$STAGE_DEVICE/Headers" "$STAGE_SIM/Headers"

cp "$DEVICE_A" "$STAGE_DEVICE/libprism_daemon.a"
cp "$SIM_A"    "$STAGE_SIM/libprism_daemon.a"
cp "$PLUGIN_DIR/ios/include/PrismDaemon.h"   "$STAGE_DEVICE/Headers/"
cp "$PLUGIN_DIR/ios/include/PrismDaemon.h"   "$STAGE_SIM/Headers/"
cp "$PLUGIN_DIR/ios/include/module.modulemap" "$STAGE_DEVICE/Headers/"
cp "$PLUGIN_DIR/ios/include/module.modulemap" "$STAGE_SIM/Headers/"

OUT="$PLUGIN_DIR/ios/Frameworks/PrismDaemon.xcframework"
rm -rf "$OUT"
mkdir -p "$(dirname "$OUT")"

log "assembling $OUT"
xcodebuild -create-xcframework \
    -library "$STAGE_DEVICE/libprism_daemon.a" -headers "$STAGE_DEVICE/Headers" \
    -library "$STAGE_SIM/libprism_daemon.a"    -headers "$STAGE_SIM/Headers" \
    -output "$OUT"

log "done — $OUT"
