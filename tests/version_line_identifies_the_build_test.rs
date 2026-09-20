//! `almide --version` identifies the BUILD, not just Cargo.toml (#2384).
//!
//! `CARGO_PKG_VERSION` answers "what does Cargo.toml say". Between a release
//! tag and the next version bump, every build from `develop` wears the
//! PREVIOUS release's number — `v0.62.0` tagged 2026-09-08, `string.byte_slice`
//! landed 2026-09-10, Cargo.toml went to `0.63.0` on 2026-09-18 — so for ten
//! days `almide --version` said `0.62.0` for a compiler that had a stdlib
//! function 0.62.0 does not have. A downstream session's every "0.62.0" A/B
//! column for this release came from such a binary: not stale, MISLABELLED,
//! which re-running never catches because the answer is consistent each time.
//!
//! Two things are pinned here, and the second is the one that makes the first
//! safe:
//!
//! 1. the version line always names which kind of build produced it, and
//! 2. the COMPARABLE version is untouched — `almide.toml`'s `almide_min` is
//!    still matched against the bare `CARGO_PKG_VERSION`.
//!
//! (2) is why the provenance is not spelled into the version itself:
//! `0.63.0-dev` sorts BELOW `0.63.0` in semver, so decorating it would make a
//! development build fail the pin on packages it can compile — trading a
//! silent wrong answer for a loud one.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn version_line() -> Option<String> {
    let out = Command::new(almide_bin()).arg("--version").output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `almide <version> (<kind>[, <sha>])`. The kind is asserted to be one of the
/// two the build script can emit rather than to be `dev` specifically, so that
/// running this against a release artifact is a pass and not a false alarm —
/// the invariant is that the line SAYS which, not which it says.
#[test]
fn the_version_line_names_the_kind_of_build() {
    let Some(line) = version_line() else {
        eprintln!("skip: almide binary unavailable");
        return;
    };
    let mut fields = line.split_whitespace();
    assert_eq!(fields.next(), Some("almide"), "unexpected version line: {line}");

    let version = fields.next().unwrap_or_default();
    assert!(
        version.split('.').count() == 3 && version.split('.').all(|p| !p.is_empty()),
        "the second field must be the bare version — Makefile's install assertion reads it \
         with `awk '{{print $2}}'`: {line}"
    );

    let rest = line.split_once('(').map(|(_, r)| r).unwrap_or("");
    let kind = rest.split([',', ')']).next().unwrap_or("").trim();
    assert!(
        kind == "dev" || kind == "release",
        "the version line must name the kind of build, so a binary can be identified without \
         building the tag to find out (#2384). Got: {line}"
    );
}

/// The half that would have caught the real defect: a binary is identified by
/// something that a develop build CANNOT accidentally answer the way a release
/// does. A default `cargo build` — which is what CI runs this under — must say
/// `dev`, because only the release workflow sets the environment that says
/// otherwise.
#[test]
fn a_build_that_did_not_come_from_the_release_workflow_says_dev() {
    if std::env::var("ALMIDE_BUILD_PROVENANCE").as_deref() == Ok("release") {
        eprintln!("skip: this tree was built as a release artifact on purpose");
        return;
    }
    let Some(line) = version_line() else {
        eprintln!("skip: almide binary unavailable");
        return;
    };
    // Guard against ALMIDE_BIN pointing at an installed release binary, which
    // would make this assert something about the wrong build.
    if !line.contains('(') {
        eprintln!("skip: {line} predates #2384 — not the tree under test");
        return;
    }
    assert!(
        line.contains("(dev"),
        "a build outside the release workflow claimed to be a release: {line}"
    );
}

/// The comparable version is NOT decorated: a package pinning exactly this
/// compiler's version still checks clean on a development build of it. This is
/// the assertion that forbids the `0.63.0-dev` shape, which would sort below
/// `0.63.0` and reject packages this binary compiles.
#[test]
fn the_package_version_pin_still_matches_a_dev_build() {
    let bin = almide_bin();
    if Command::new(&bin).arg("--version").output().is_err() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let Some(line) = version_line() else { return };
    let Some(version) = line.split_whitespace().nth(1) else { return };

    let dir = std::env::temp_dir().join("almide-issue2384-pin");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).expect("mkdir");
    std::fs::write(
        dir.join("almide.toml"),
        format!("[package]\nname = \"pinned\"\nversion = \"0.1.0\"\nalmide = \"{version}\"\n"),
    )
    .expect("write almide.toml");
    std::fs::write(dir.join("src/main.almd"), "fn main() -> Unit = println(\"ok\")\n")
        .expect("write main.almd");

    let out = Command::new(&bin)
        .arg("check")
        .current_dir(&dir)
        .output()
        .expect("spawn almide check");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success(),
        "a package pinning almide = \"{version}\" was refused by the binary reporting that \
         version — the comparable version has been decorated with provenance:\n{log}"
    );
}
