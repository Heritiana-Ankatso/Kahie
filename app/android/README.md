# Kahie Android

The native Android app is built on the Rust core in [`core/`](../../core). Gradle runs `build-core.sh`, which:

1. cross-compiles `nb-ffi` for `arm64-v8a` and `x86_64` with `cargo ndk`;
2. generates the Kotlin bindings with UniFFI (library mode) from a host build of the same crate.

The app loads the core through JNA (`net.java.dev.jna:jna` AAR). It requires **Android 12 (API 31)** or newer (decision D4).

Wet ink (the stroke being drawn) is rendered by **Jetpack Ink** (`androidx.ink`, front-buffered, low latency) in `JetpackWetInk.kt`, stacked over `InkView`. This is a prototype: the Rust core remains the source of truth and draws every committed stroke. Set the shared preference `jetpackInk` to `false` to fall back to `InkView`'s own wet-ink drawing.

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

## CI

`.github/workflows/android.yml` runs Rust lint and tests, then builds the debug APK and uploads it as the `kahie-debug-apk` artifact.

## Release signing

CI signs release builds with the Kahie release key, kept in three repository secrets: `KAHIE_KEYSTORE_B64` (the PKCS12 keystore, base64), `KAHIE_KEYSTORE_PASSWORD` (store and key password) and `KAHIE_KEY_ALIAS`. The `android` workflow uploads that APK as the `kahie-release-apk` artifact, with `versionCode` set to the workflow run number, so every build installs as an update over the previous one. Hand testers this APK, not the debug one (CI debug keys change on every run).

Locally, set `KAHIE_KEYSTORE` (path to the keystore), `KAHIE_KEYSTORE_PASSWORD` and `KAHIE_KEY_ALIAS` before `./gradlew assembleRelease`; without them, release builds fall back to the debug key. Keep a backup of the keystore and password outside the repository: losing them means every tester has to uninstall once more.
