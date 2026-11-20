# Kahie Android

The native Android app is built on the Rust core in [`core/`](../../core). Gradle runs `build-core.sh`, which:

1. cross-compiles `nb-ffi` for `arm64-v8a` and `x86_64` with `cargo ndk`;
2. generates the Kotlin bindings with UniFFI (library mode) from a host build of the same crate.

The app loads the core through JNA (`net.java.dev.jna:jna` AAR).

## Prerequisites

| Tool | Notes |
|---|---|
| Rust (stable) | `rustup target add aarch64-linux-android x86_64-linux-android` |
| cargo-ndk | `cargo install cargo-ndk --locked` |
| Android SDK | Platform 35 and build tools. Set `ANDROID_HOME` or create `local.properties` with `sdk.dir=…` |
| Android NDK | Any version under `$ANDROID_HOME/ndk/`; the newest is picked, or set `ANDROID_NDK_HOME` |
| JDK 17+ | The project targets Java 17 |

Gradle downloads the Android Gradle Plugin from Google's Maven repository (`maven.google.com`, served from `dl.google.com`), so that host must be reachable.

## Build

```bash
cd app/android
./gradlew assembleDebug        # app/build/outputs/apk/debug/app-debug.apk
```

## Check the core without the Android SDK

```bash
cd core
cargo test --workspace
cargo check -p nb-ffi --target aarch64-linux-android --release
```

## Release signing

Release builds are signed with the debug key until a release key exists.
