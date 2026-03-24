//! Build script entry point.
//!
//! Split into three modules:
//!   - stdlib_codegen:    TOML definitions → codegen dispatch tables
//!   - runtime_registry:  runtime/ts/, runtime/rs/ → include_str! registries
//!   - token_table:       grammar/tokens.toml → keyword map, precedence table

#[path = "build_helpers/stdlib_codegen.rs"]
mod stdlib_codegen;

#[path = "build_helpers/runtime_registry.rs"]
mod runtime_registry;

#[path = "build_helpers/token_table.rs"]
mod token_table;

fn main() {
    let out_dir = std::path::Path::new("src/generated");
    std::fs::create_dir_all(out_dir).unwrap();

    stdlib_codegen::generate_stdlib();
    runtime_registry::generate_runtime_registry(out_dir);
    token_table::generate_token_table(out_dir);
}
