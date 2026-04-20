//! Build script for almide-egg-lab.
//!
//! Generates `matrix_rules_gen.rs` at `$OUT_DIR` from the `@rewrite`
//! attributes in `stdlib/matrix.almd`. Consumed via `include!` from
//! `src/lib.rs` so `matrix_fusion_rules()` stays in sync with stdlib.

#[path = "buildscript/egg_rules.rs"]
mod egg_rules;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = std::path::Path::new(&manifest_dir).join("../..");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    let out_path = std::path::Path::new(&out_dir);
    egg_rules::generate(&workspace_root, out_path);
}
