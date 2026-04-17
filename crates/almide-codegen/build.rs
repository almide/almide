//! Build script for almide-codegen.
//!
//! Generates `rust_runtime.rs`: the runtime-module registry (source
//! text embedded into the compiler) from `runtime/rs/src/*.rs`. The
//! Stdlib Declarative Unification arc retired the TOML-derived
//! `arg_transforms` / `stdlib_ret_ty` tables, so only the runtime
//! registry remains.

#[path = "buildscript/runtime_registry.rs"]
mod runtime_registry;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = std::path::Path::new(&manifest_dir).join("../..");
    let out_dir = std::path::Path::new("src/generated");
    std::fs::create_dir_all(out_dir).unwrap();

    runtime_registry::generate(&workspace_root, out_dir);
}
