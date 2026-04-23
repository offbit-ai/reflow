plugins {
    id("java-library")
    id("maven-publish")
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
    testImplementation(platform("org.junit:junit-bom:5.10.2"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
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
