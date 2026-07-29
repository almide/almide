fn main() {
    // All other code generation moved to crate-specific build scripts:
    // - almide-codegen: arg_transforms.rs, rust_runtime.rs
    // - almide-frontend: stdlib_sigs.rs
    embed_diagnostic_docs();
}

/// Generate `${OUT_DIR}/diagnostic_docs.rs`: a `(code, markdown)` table with one
/// `include_str!` per `docs/diagnostics/<CODE>.md`.
///
/// `almide explain <code>` used to probe the DISK for these files — a 6-level
/// parent walk from the executable — which worked only from a checkout: an
/// installed binary (`make install` ships the binary and the embedded stdlib,
/// not docs/) answered "Unknown error code" for 21 of the documented codes while
/// docs/diagnostics/README.md told users to run exactly that command (#923).
/// Embedding at build time makes the binary the delivery, the same move
/// `stdlib_info.rs` made for the stdlib. Scanning the directory (rather than
/// hand-listing the codes) means a new `E0xx.md` ships in the table by
/// existing; `tests/explain_docs_test.rs` pins that equivalence from outside.
fn embed_diagnostic_docs() {
    let docs_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/diagnostics");
    println!("cargo:rerun-if-changed={}", docs_dir.display());
    let mut codes: Vec<(String, std::path::PathBuf)> = std::fs::read_dir(&docs_dir)
        .expect("docs/diagnostics must exist — `almide explain` embeds it")
        .filter_map(|e| {
            let path = e.ok()?.path();
            let stem = path.file_stem()?.to_str()?.to_string();
            // Code files only (E001.md, E420.md …) — README.md is the index for
            // humans browsing the repo, not an explainable code.
            (path.extension()?.to_str()? == "md"
                && stem.starts_with('E')
                && stem[1..].chars().all(|c| c.is_ascii_digit()))
            .then_some((stem, path))
        })
        .collect();
    codes.sort();
    let mut out = String::from(
        "/// Every diagnostic doc under docs/diagnostics, embedded at build time (#923).\n\
         static DIAGNOSTIC_DOCS: &[(&str, &str)] = &[\n",
    );
    for (code, path) in &codes {
        out.push_str(&format!(
            "    ({:?}, include_str!({:?})),\n",
            code,
            path.display().to_string()
        ));
    }
    out.push_str("];\n");
    let dest = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("diagnostic_docs.rs");
    std::fs::write(dest, out).expect("write diagnostic_docs.rs");
}
