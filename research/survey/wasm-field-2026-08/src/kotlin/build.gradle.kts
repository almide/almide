import org.jetbrains.kotlin.gradle.ExperimentalWasmDsl
import org.jetbrains.kotlin.gradle.dsl.KotlinMultiplatformExtension

plugins {
    kotlin("multiplatform") version "2.3.21" apply false
}

subprojects {
    apply(plugin = "org.jetbrains.kotlin.multiplatform")
    repositories { mavenCentral() }
    @OptIn(ExperimentalWasmDsl::class)
    configure<KotlinMultiplatformExtension> {
        wasmWasi {
            nodejs()
            binaries.executable()
        }
    }
}
