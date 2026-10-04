//! Build script for almide-egg-lab.
//!
//! Generates the committed `src/generated/matrix_rules_gen.rs` from the
//! `@rewrite` attributes in `stdlib/matrix.almd`. Consumed via `include!`
//! from `src/lib.rs` so `matrix_fusion_rules()` stays in sync with stdlib.
//! `stdlib/` is outside this package, so the table is regenerated only inside
//! an almide checkout; a vendored or packaged almide-egg-lab compiles the
//! committed file (#3361).

#[path = "buildscript/in_checkout.rs"]
mod in_checkout;

#[path = "buildscript/egg_rules.rs"]
mod egg_rules;

fn main() {
    let manifest_dir = in_checkout::manifest_dir();
    let Some(workspace_root) = in_checkout::checkout_root(&manifest_dir) else {
        // Standalone (vendored / packaged): the committed table is the input.
        println!("cargo:rerun-if-changed=build.rs");
        return;
    };
    egg_rules::generate(&workspace_root, &manifest_dir.join("src/generated"));
}
