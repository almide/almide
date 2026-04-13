//! Build script for almide-codegen.
//!
//! Generates:
//!   - arg_transforms.rs: stdlib call info (borrow/clone annotations) from stdlib/defs/*.toml
//!   - rust_runtime.rs:   runtime module registry from runtime/rs/src/*.rs

#[path = "buildscript/arg_transforms_gen.rs"]
mod arg_transforms_gen;

#[path = "buildscript/runtime_registry.rs"]
mod runtime_registry;

#[path = "buildscript/stdlib_ret_ty_gen.rs"]
mod stdlib_ret_ty_gen;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = std::path::Path::new(&manifest_dir).join("../..");
    let out_dir = std::path::Path::new("src/generated");
    std::fs::create_dir_all(out_dir).unwrap();

    arg_transforms_gen::generate(&workspace_root, out_dir);
    runtime_registry::generate(&workspace_root, out_dir);
    stdlib_ret_ty_gen::generate(&workspace_root, out_dir);
}
