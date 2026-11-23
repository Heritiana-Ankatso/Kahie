plugins {
    id("com.android.application")
}

val coreDir = rootDir.resolve("../../core")
val generatedKotlin = layout.buildDirectory.dir("generated/uniffi/kotlin")
val generatedJni = layout.buildDirectory.dir("generated/jniLibs")

val buildRustCore by tasks.registering(Exec::class) {
    description = "Builds the Rust core (nb-ffi) for Android and generates Kotlin bindings."
    inputs.dir(coreDir.resolve("crates"))
    inputs.file(coreDir.resolve("Cargo.toml"))
    inputs.file(coreDir.resolve("Cargo.lock"))
    outputs.dir(generatedKotlin)
    outputs.dir(generatedJni)
    commandLine(
        "bash", rootDir.resolve("build-core.sh").path,
        generatedJni.get().asFile.path, generatedKotlin.get().asFile.path, "release",
    )
}

android {
    namespace = "app.kahie"
    compileSdk = 35

    defaultConfig {
        applicationId = "app.kahie"
        minSdk = 31
        targetSdk = 35
        // CI passes its run number so each build installs as an update over the previous one.
        versionCode = System.getenv("KAHIE_VERSION_CODE")?.toIntOrNull() ?: 1
        versionName = "0.1.0"
    }

    // Release key from the environment (CI secrets; see app/android/README.md). Without it,
    // release builds fall back to the debug key and cannot update a properly signed install.
    val releaseKeystore = System.getenv("KAHIE_KEYSTORE")?.takeIf { it.isNotBlank() }
    signingConfigs {
        if (releaseKeystore != null) {
            create("kahieRelease") {
                storeFile = file(releaseKeystore)
                storePassword = System.getenv("KAHIE_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("KAHIE_KEY_ALIAS")
                keyPassword = System.getenv("KAHIE_KEYSTORE_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName(if (releaseKeystore != null) "kahieRelease" else "debug")
        }
    }

    sourceSets {
        getByName("main") {
            // AGP 9 compiles Kotlin itself: generated .kt files must be Kotlin source dirs.
            kotlin.srcDir(generatedKotlin.get().asFile)
            jniLibs.srcDir(generatedJni.get().asFile)
            // MathJax (Apache-2.0) shared with Kahie Web: assets/mathjax/tex-svg-full.js
            assets.srcDir(rootDir.resolve("../web/vendor"))
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("net.java.dev.jna:jna:5.17.0@aar")
    // Low-latency wet ink (prototype; see JetpackWetInk.kt).
    implementation("androidx.ink:ink-authoring:1.0.0")
    implementation("androidx.ink:ink-brush:1.0.0")
    implementation("androidx.ink:ink-strokes:1.0.0")
    // SVG → Picture for maths rendered by MathJax (Apache-2.0).
    implementation("com.caverock:androidsvg-aar:1.4")
}

tasks.named("preBuild") { dependsOn(buildRustCore) }
