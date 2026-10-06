//! Build script for almide-codegen.
//!
//! Generates `rust_runtime.rs`: the runtime-module registry (source
//! text embedded into the compiler) from `runtime/rs/src/*.rs`. The
//! Stdlib Declarative Unification arc retired the TOML-derived
//! `arg_transforms` / `stdlib_ret_ty` tables, so only the runtime
//! registry remains.
//!
//! The imperative `FusionRule` emitter (Stage 1 skeleton) was retired
//! when `EggSaturationPass` became the sole fusion driver; the
//! `fusion_parse.rs` DSL parser lives on and is shared with
//! `almide-egg-lab/buildscript/egg_rules.rs` as a byte-identical copy.
//!
//! The outputs are the committed `src/generated/*.rs`; their inputs live
//! outside this package, so they are regenerated only inside an almide
//! checkout. A vendored or packaged almide-codegen compiles the committed
//! files and reads nothing past its own directory (#3361).

#[path = "buildscript/in_checkout.rs"]
mod in_checkout;

#[path = "buildscript/runtime_registry.rs"]
mod runtime_registry;

#[path = "buildscript/matrix_desugar.rs"]
mod matrix_desugar;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = in_checkout::manifest_dir();
    let Some(workspace_root) = in_checkout::checkout_root(&manifest_dir) else {
        // Standalone (vendored / packaged): the committed tables are the input.
        println!("cargo:rerun-if-changed=build.rs");
        return Ok(());
    };
    let out_dir = manifest_dir.join("src/generated");
    std::fs::create_dir_all(&out_dir)?;

    runtime_registry::generate(&workspace_root, &out_dir)?;
    // Reverse of the @rewrite fusion rules → desugar fallback (see matrix_desugar.rs).
    matrix_desugar::generate(&workspace_root, &out_dir);
    Ok(())
}
