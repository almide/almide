fn main() {
    // All other code generation moved to crate-specific build scripts:
    // - almide-codegen: arg_transforms.rs, rust_runtime.rs
    // - almide-frontend: stdlib_sigs.rs
    embed_diagnostic_docs();
    emit_version_line();
}

/// Generate `ALMIDE_VERSION_LINE`: the version number PLUS which kind of build
/// produced it (#2384).
///
/// `CARGO_PKG_VERSION` answers "what does Cargo.toml say", which is not the
/// same question as "which compiler is this". Between a release tag and the
/// next version bump, every `make install` from `develop` produces a binary
/// wearing the PREVIOUS release's number: `v0.62.0` was tagged 2026-09-08,
/// `string.byte_slice` landed 2026-09-10, and Cargo.toml went to `0.63.0` on
/// 2026-09-18 — ten days in which `almide --version` said `0.62.0` for a
/// compiler that was not it. The bump is one commit, so the window opens every
/// cycle by construction. It cost a downstream session every "0.62.0" column
/// of every A/B sweep for this release: not stale, MISLABELLED, which no
/// re-run catches because the binary answers consistently every time.
///
/// The two kinds of thing are separated by structure rather than by a spelling,
/// and deliberately NOT by decorating the version itself: `0.63.0-dev` sorts
/// BELOW `0.63.0` in semver, so a development build would start failing
/// `almide_min = "0.63.0"` on packages it can compile — a loud wrong answer in
/// place of a silent one. So `CARGO_PKG_VERSION` stays the COMPARABLE version
/// (`src/project.rs`'s `check_compiler_version_with` reads exactly it, and is
/// untouched by this), and the provenance rides alongside it, for humans and
/// for whoever is identifying a binary in a measurement.
///
/// The discriminator is a mechanism, not a convention: `release` is stated by
/// the environment, and only `.github/workflows/release.yml` — the one thing
/// that builds from a tag — states it. Anything else is `dev`, including a
/// developer's `cargo build`, so the default is the honest answer rather than
/// the flattering one.
fn emit_version_line() {
    println!("cargo:rerun-if-env-changed=ALMIDE_BUILD_PROVENANCE");
    println!("cargo:rerun-if-env-changed=ALMIDE_BUILD_SHA");
    let kind = match std::env::var("ALMIDE_BUILD_PROVENANCE").as_deref() {
        Ok("release") => "release",
        _ => "dev",
    };
    // The sha is passed IN (by `make install` and by the release workflow)
    // rather than read from git here: running `git rev-parse` in the build
    // script would mean a `cargo:rerun-if-changed` on `.git/HEAD`, and so a
    // rebuild of the largest crate in the workspace after every commit — a
    // cost paid by everyone, every day, to identify a binary that is usually
    // not being identified. Absent, the line still says `dev`, which is the
    // part that cannot be wrong.
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION");
    let line = match std::env::var("ALMIDE_BUILD_SHA") {
        Ok(sha) if !sha.trim().is_empty() => {
            // Truncated here rather than at each caller, so the workflow's full
            // 40-char `github.sha` and the Makefile's short one read the same.
            let short: String = sha.trim().chars().take(9).collect();
            format!("{} ({}, {})", version, kind, short)
        }
        _ => format!("{} ({})", version, kind),
    };
    println!("cargo:rustc-env=ALMIDE_VERSION_LINE={}", line);
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
