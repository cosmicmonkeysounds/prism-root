#!/usr/bin/env bash
# Cross-compile prism-daemon to `libprism_daemon.so` for every Android
# ABI the Capacitor plugin supports, and stage the results into the
# plugin's `jniLibs/` folder.
#
# ## What this produces
#
#   packages/prism-capacitor-daemon/android/src/main/jniLibs/
#     arm64-v8a/libprism_daemon.so    (physical arm64 devices)
#     armeabi-v7a/libprism_daemon.so  (older 32-bit arm devices)
#     x86_64/libprism_daemon.so       (Android Emulator on Intel / AVD)
#
# ## Why --crate-type cdylib instead of the default staticlib
#
# Cargo.toml declares `crate-type = ["staticlib", "rlib"]` because iOS
# wants a .a (staticlib) inside the xcframework and desktop/tests want
# the rlib. Android's System.loadLibrary / JNA both need a .so
# (cdylib). We override the crate-type at build time with
# `cargo rustc --lib --crate-type cdylib` so we get a .so for Android
# without forcing every other target to also emit one.
#
# ## Requirements
#
#   - rustup targets: aarch64-linux-android, armv7-linux-androideabi,
#                     x86_64-linux-android
#   - cargo-ndk (cargo install cargo-ndk)
#   - Android NDK 27+ at $ANDROID_NDK_HOME or $ANDROID_HOME/ndk/<ver>
#
# ## Usage
#
#   ./scripts/build-android.sh           # release
#   ./scripts/build-android.sh debug     # debug
#

set -euo pipefail

PROFILE="${1:-release}"
case "$PROFILE" in
    debug|release) ;;
    *) echo "usage: $0 [debug|release]" >&2; exit 1 ;;
esac

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DAEMON_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PLUGIN_JNI="$(cd "$DAEMON_DIR/../prism-capacitor-daemon/android/src/main" && pwd)/jniLibs"

log() { printf '\033[1;34m[build-android]\033[0m %s\n' "$*" >&2; }
err() { printf '\033[1;31m[build-android]\033[0m %s\n' "$*" >&2; exit 1; }

# ── Locate the NDK ─────────────────────────────────────────────────────
# cargo-ndk honors ANDROID_NDK_HOME / ANDROID_NDK_ROOT. If neither is
# set but we find an NDK under $ANDROID_HOME/ndk, pick the highest
# version automatically so the developer doesn't have to export the
# env var by hand.
if [ -z "${ANDROID_NDK_HOME:-}" ] && [ -z "${ANDROID_NDK_ROOT:-}" ]; then
    SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
    if [ -d "$SDK/ndk" ]; then
        LATEST_NDK="$(ls -1 "$SDK/ndk" | sort -V | tail -n1)"
        if [ -n "$LATEST_NDK" ]; then
            export ANDROID_NDK_HOME="$SDK/ndk/$LATEST_NDK"
            log "auto-selected NDK: $ANDROID_NDK_HOME"
        fi
    fi
fi

[ -n "${ANDROID_NDK_HOME:-}${ANDROID_NDK_ROOT:-}" ] || err "set ANDROID_NDK_HOME or install NDK via sdkmanager"
command -v cargo-ndk >/dev/null 2>&1 || err "cargo-ndk not found — install with: cargo install cargo-ndk"

# ── Build each ABI ─────────────────────────────────────────────────────
# `cargo ndk -t <abi>` injects the right linker and CC env vars, then
# forwards to cargo. We call `rustc --lib --crate-type cdylib` so the
# library is built as a shared object even though the default crate-
# type is staticlib.
RUSTC_FLAGS=(--lib --crate-type cdylib --no-default-features --features mobile)
if [ "$PROFILE" = "release" ]; then
    RUSTC_FLAGS=(--release "${RUSTC_FLAGS[@]}")
fi

# The --output-dir flag tells cargo-ndk to copy the produced .so files
# into per-ABI subdirectories, which is exactly the jniLibs layout
# Gradle wants. We wipe the destination first so stale .sos from a
# previous run don't stick around.
rm -rf "$PLUGIN_JNI"
mkdir -p "$PLUGIN_JNI"

log "building cdylib for arm64-v8a, armeabi-v7a, x86_64 ($PROFILE)"
(cd "$DAEMON_DIR" && cargo ndk \
    -t arm64-v8a \
    -t armeabi-v7a \
    -t x86_64 \
    --output-dir "$PLUGIN_JNI" \
    --platform 23 \
    rustc "${RUSTC_FLAGS[@]}")

# ── Copy libc++_shared.so alongside the daemon ─────────────────────────
# mlua's vendored Luau is C++ and links libc++_shared.so dynamically.
# The NDK ships libc++_shared.so per-arch inside the toolchain sysroot;
# we have to copy it into jniLibs ourselves because cargo-ndk only
# copies the Rust-produced artifact. Without this the runtime loader
# fails with:
#   dlopen failed: library "libc++_shared.so" not found
#     needed by libprism_daemon.so
# Alternative fix would be linking libc++_static, but Luau's CMake
# wants the shared runtime and working around that is more fragile
# than just shipping the 1 MB .so.
NDK_ROOT="${ANDROID_NDK_HOME:-$ANDROID_NDK_ROOT}"
HOST_TAG=""
case "$(uname -s)" in
    Darwin) HOST_TAG="darwin-x86_64" ;;
    Linux)  HOST_TAG="linux-x86_64" ;;
    *) err "unsupported host: $(uname -s)" ;;
esac
SYSROOT_LIB="$NDK_ROOT/toolchains/llvm/prebuilt/$HOST_TAG/sysroot/usr/lib"
# Plain positional parallel arrays; `declare -A` on macOS's bash 3.2 is
# a pain to use with `set -u`.
ABIS=(arm64-v8a armeabi-v7a x86_64)
TRIPLES=(aarch64-linux-android arm-linux-androideabi x86_64-linux-android)
for i in 0 1 2; do
    abi="${ABIS[$i]}"
    triple="${TRIPLES[$i]}"
    src="$SYSROOT_LIB/$triple/libc++_shared.so"
    [ -f "$src" ] || err "missing libc++_shared.so for $abi at $src"
    cp "$src" "$PLUGIN_JNI/$abi/libc++_shared.so"
done

log "staged artifacts:"
for abi in arm64-v8a armeabi-v7a x86_64; do
    so="$PLUGIN_JNI/$abi/libprism_daemon.so"
    cxx="$PLUGIN_JNI/$abi/libc++_shared.so"
    if [ -f "$so" ] && [ -f "$cxx" ]; then
        size="$(wc -c < "$so" | tr -d ' ')"
        cxxsize="$(wc -c < "$cxx" | tr -d ' ')"
        log "  $abi: libprism_daemon.so ($size) + libc++_shared.so ($cxxsize)"
    else
        err "missing artifacts in $PLUGIN_JNI/$abi"
    fi
done

log "done."
