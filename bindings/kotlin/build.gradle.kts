import com.github.jengelman.gradle.plugins.shadow.tasks.ShadowJar
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

version = versionFromCargo("../../Cargo.toml")

plugins {
    kotlin("jvm") version "2.1.10"
    id("com.github.johnrengelman.shadow") version "8.1.1"

}
kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_1_8)
    }
}
java {
    sourceCompatibility = JavaVersion.VERSION_1_8
    targetCompatibility = JavaVersion.VERSION_1_8
}


repositories {
    mavenCentral()
}

dependencies {
    val co_routines_version = "1.10.1"
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:$co_routines_version")
    val jna_version = "5.16.0";
    implementation("net.java.dev.jna:jna:$jna_version")
    val junit_version = "5.11.4"
    testImplementation("org.junit.jupiter:junit-jupiter:$junit_version")
}
val nativeDir = layout.buildDirectory.dir("generated/sources/native").get().asFile.absolutePath

tasks {
    val generateUniFFIBindings by registering(Exec::class) {
        group = "build"
        description = "Generates UniFFI bindings for Kotlin."

        workingDir = layout.projectDirectory.dir("../../target/release/").asFile

        var libExtension = "dylib"
        if (System.getProperty("os.name").lowercase().contains("linux")) {
            libExtension = "so"
        }

        //generate the bindings
        commandLine(
            "./uniffi-bindgen",
            "generate",
            "--library",
            "libeinvoice.${libExtension}",
            "--language",
            "kotlin",
            "--out-dir",
            nativeDir
        )

    }

    val compileKotlin by getting {
        dependsOn(generateUniFFIBindings)
    }
}

tasks.processResources {
    from("../../target/release") {
        include("*.dylib")
        into("darwin-aarch64")
        duplicatesStrategy = DuplicatesStrategy.EXCLUDE
    }
    from("../../target/release") {
        include("*.so")
        into("linux-x86-64")
        duplicatesStrategy = DuplicatesStrategy.EXCLUDE
    }
}

tasks.withType<ShadowJar> {
    archiveClassifier.set("") // Replace default "all" classifier
    relocate("org.jetbrains.annotations", "com.schneppe.einvoice.annotations.jetbrains")
    relocate("org.intellij.lang.annotations", "com.schneppe.einvoice.annotations.intellij")
    relocate("kotlinx.coroutines", "com.schneppe.einvoice.coroutines")
    relocate("kotlin", "com.schneppe.einvoice.kotlin")
    mergeServiceFiles()
}


tasks.build {
    dependsOn("shadowJar")
}
tasks.test {
    useJUnitPlatform()
}

sourceSets {
    val main by getting {
        kotlin.srcDir(nativeDir)
    }
}

fun versionFromCargo(s: String): String {
    val file = File(s)
    val version = file.readLines().find { it.startsWith("version") } ?: error("Version not found in Cargo.toml")
    return version.split("=")[1].replace("\"", "").trim()
}
