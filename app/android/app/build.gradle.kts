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
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // Signed with the debug key until a release key exists (see app/android/README.md).
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    sourceSets {
        getByName("main") {
            // AGP 9 compiles Kotlin itself: generated .kt files must be Kotlin source dirs.
            kotlin.srcDir(generatedKotlin.get().asFile)
            jniLibs.srcDir(generatedJni.get().asFile)
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
}

tasks.named("preBuild") { dependsOn(buildRustCore) }
