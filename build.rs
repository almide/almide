//! Build script entry point.
//!
//! Generates:
//!   - stdlib_sigs: TOML definitions → type signature lookup table

#[path = "buildscript/stdlib_codegen.rs"]
mod stdlib_codegen;

fn main() {
    let out_dir = std::path::Path::new("src/generated");
    std::fs::create_dir_all(out_dir).unwrap();

    stdlib_codegen::generate_stdlib();
}
