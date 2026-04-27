plugins {
    java
    application
}

group = "ai.offbit.reflow.tutorial06"
version = "0.0.1"

java {
    toolchain {
        languageVersion.set(JavaLanguageVersion.of(17))
    }
}

repositories {
    mavenCentral()
    mavenLocal()
}

dependencies {
    implementation("ai.offbit:reflow:0.2.6")
    implementation("org.apache.kafka:kafka-clients:3.7.1")
    implementation("org.slf4j:slf4j-simple:2.0.13")

    testImplementation("org.junit.jupiter:junit-jupiter:5.10.3")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

application {
    mainClass.set("ai.offbit.reflow.tutorial06.Tutorial06Application")
    // Local-development override: point at the freshly-built native
    // lib rather than the one bundled in the published jar (which
    // hasn't been re-cut yet for 0.2.6). Drop this line when running
    // against a published artifact that ships the native bundle.
    val devLib = file("../../src/native/target/release/libreflow_rt_jvm.dylib")
    if (devLib.exists()) {
        applicationDefaultJvmArgs = listOf("-Dreflow.native.lib=${devLib.absolutePath}")
    }
}

tasks.test {
    useJUnitPlatform()
}
