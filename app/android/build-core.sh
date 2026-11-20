#!/usr/bin/env bash
# Builds the Rust core for Android and generates its Kotlin bindings.
# Usage: build-core.sh <jniLibs out dir> <kotlin out dir> [debug|release]
set -euo pipefail
JNI_OUT=$1
KT_OUT=$2
PROFILE=${3:-release}
export PATH="$HOME/.cargo/bin:$PATH"
SDK=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  ANDROID_NDK_HOME=$(ls -d "$SDK"/ndk/* 2>/dev/null | sort -V | tail -1)
  export ANDROID_NDK_HOME
fi
cd "$(dirname "$0")/../../core"
FLAG=$([[ $PROFILE == release ]] && echo --release || true)
cargo ndk -t arm64-v8a -t x86_64 --platform 31 -o "$JNI_OUT" build $FLAG -p nb-ffi
# Bindings are generated from a host build of the same crate (UniFFI library mode).
cargo build $FLAG -p nb-ffi
rm -rf "$KT_OUT"
cargo run -q $FLAG -p nb-ffi --bin uniffi-bindgen -- generate \
  --library "target/$PROFILE/libnb_ffi.so" --language kotlin --no-format --out-dir "$KT_OUT"
