plugins {
    id("java-library")
    id("maven-publish")
    kotlin("jvm") version "1.9.24"
}

group = "ai.offbit"
version = "0.2.0"

java {
    toolchain {
        languageVersion.set(JavaLanguageVersion.of(17))
    }
    withSourcesJar()
}

repositories {
    mavenCentral()
}

dependencies {
    // Kotlin stdlib is pulled in automatically by the kotlin("jvm") plugin.
    // Coroutines are an opt-in convenience used by the Kotlin suspend /
    // Flow adapters. Users who don't want coroutines can omit them.
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")

    testImplementation(platform("org.junit:junit-bom:5.10.2"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testImplementation("org.jetbrains.kotlin:kotlin-test-junit5")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.8.1")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

kotlin {
    jvmToolchain(17)
}

// Point the JVM at the locally built native library so tests can run
// without installing it globally.
val nativeLibPath = rootDir.resolve("src/native/target/release/libreflow_rt_jvm.dylib")

tasks.withType<Test>().configureEach {
    useJUnitPlatform()
    systemProperty("reflow.native.lib", nativeLibPath.absolutePath)
}

// Convenience task that builds the Rust side first.
val buildNative by tasks.registering(Exec::class) {
    workingDir = rootDir.resolve("src/native")
    commandLine("cargo", "build", "--release")
}
tasks.named("compileJava") { dependsOn(buildNative) }
tasks.named("test") { dependsOn(buildNative) }
