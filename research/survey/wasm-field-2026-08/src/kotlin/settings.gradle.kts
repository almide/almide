pluginManagement {
    repositories {
        mavenCentral()
        gradlePluginPortal()
    }
}

rootProject.name = "wasm-field-kotlin"

include(
    ":empty",
    ":int_loop",
    ":float_math",
    ":str_build",
    ":recursion",
    ":list_sort",
    ":sort_by",
    ":list_pipeline",
)
