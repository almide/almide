//! #2554: the wasm routing decision has ONE implementation
//! (`almide::wasm_route::route_wasm`), which the CLI's `--target wasm` and
//! the library entry `render_wasm_routed` both call. This net measures that
//! claim on the whole `spec/wasm_cross` corpus rather than trusting the
//! call graph: every fixture goes through `almide build --target wasm` (the
//! CLI, a subprocess, its leg named by `ALMIDE_VERIFIED_DEBUG`) and through
//! the library entry in the BUILD form, and the two must agree on the leg
//! and on the bytes — the CLI's file is the library's `stock_wasi()` form
//! (the structural leg's module through `to_wasi`, the incumbent's as is).
//! A fixture the CLI refuses must be refused by the library too.
//!
//! The two sides read the routing facts differently — the CLI off the v0 IR
//! its own gates built, the library off the structural front's IR — so a
//! drift between `RouteInputs::of_ir`'s two feeds would show here as a leg
//! mismatch.

use almide::wasm_route::{render_wasm_routed, Leg, ModuleSource, RouteOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    env!("CARGO_BIN_EXE_almide").to_string()
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum CliVerdict {
    Structural,
    Incumbent,
    Refused,
}

/// `almide build <fixture> --target wasm -o <out>` with the probe switches
/// cleared, the leg read off the debug narration.
fn cli_route(fixture: &Path, out: &Path) -> (CliVerdict, String) {
    let output = Command::new(almide_bin())
        .args(["build", fixture.to_str().expect("utf8"), "--target", "wasm", "-o", out.to_str().expect("utf8")])
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .env_remove("ALMIDE_WASM_STRUCTURAL")
        .env_remove("ALMIDE_WASM_INCUMBENT")
        .env_remove("ALMIDE_FUEL_PROBE")
        .env_remove("ALMIDE_COMPONENT_P3")
        .current_dir(fixture.parent().expect("fixture dir"))
        .output()
        .expect("almide runs");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return (CliVerdict::Refused, stderr);
    }
    let structural = stderr.contains("[almide] structural leg emitted the module")
        || stderr.contains("[almide] incumbent walled — structural leg took the build");
    let incumbent = stderr.contains("[almide] v1 trust-spine emitted the module");
    match (structural, incumbent) {
        (true, false) => (CliVerdict::Structural, stderr),
        (false, true) => (CliVerdict::Incumbent, stderr),
        _ => panic!("{}: the CLI named no leg (or both) under ALMIDE_VERIFIED_DEBUG:\n{stderr}", fixture.display()),
    }
}

#[cfg_attr(debug_assertions, ignore = "corpus sweep through the CLI is release-only (CI: commissioned wasm gates, release-only root tests)")]
#[test]
fn library_route_matches_the_cli_route_on_every_wasm_cross_fixture() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("spec/wasm_cross");
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("spec/wasm_cross exists")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("almd"))
        .collect();
    fixtures.sort();
    let scratch = tempfile::tempdir().expect("tempdir");
    let out = scratch.path().join("m.wasm");

    let mut counts = [0usize; 3];
    let mut mismatches: Vec<String> = Vec::new();
    for fixture in &fixtures {
        let name = fixture.file_name().expect("name").to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&out);
        let (cli, stderr) = cli_route(fixture, &out);
        counts[cli as usize] += 1;
        let text = std::fs::read_to_string(fixture).expect("fixture readable");
        let lib = render_wasm_routed(
            fixture.to_str().expect("utf8"),
            &text,
            ModuleSource::Disk { dep_paths: &[] },
            RouteOptions { library: true, ..RouteOptions::default() },
        );
        match (cli, lib) {
            (CliVerdict::Refused, Err(_)) => {}
            (CliVerdict::Refused, Ok(m)) => {
                mismatches.push(format!("{name}: the CLI refused, the library routed to the {:?} leg\n  cli stderr: {}", m.leg, stderr.trim()));
            }
            (_, Err(e)) => mismatches.push(format!("{name}: the CLI built ({cli:?}), the library refused: {e:?}")),
            (verdict, Ok(m)) => {
                let want = match verdict {
                    CliVerdict::Structural => Leg::Structural,
                    CliVerdict::Incumbent => Leg::Incumbent,
                    CliVerdict::Refused => unreachable!("handled above"),
                };
                if m.leg != want {
                    mismatches.push(format!("{name}: leg differs — CLI {verdict:?}, library {:?}", m.leg));
                    continue;
                }
                let shipped = std::fs::read(&out).expect("the CLI wrote its module");
                let lib_bytes = m.stock_wasi().expect("to_wasi");
                if shipped != lib_bytes {
                    mismatches.push(format!(
                        "{name}: bytes differ on the {:?} leg — CLI wrote {} bytes, library produced {}",
                        m.leg,
                        shipped.len(),
                        lib_bytes.len()
                    ));
                }
            }
        }
    }
    eprintln!(
        "wasm route parity: {} fixtures — CLI structural {}, incumbent {}, refused {}",
        fixtures.len(),
        counts[CliVerdict::Structural as usize],
        counts[CliVerdict::Incumbent as usize],
        counts[CliVerdict::Refused as usize]
    );
    assert!(
        mismatches.is_empty(),
        "{} of {} fixtures route differently through the library entry than through the CLI:\n{}",
        mismatches.len(),
        fixtures.len(),
        mismatches.join("\n")
    );
    assert!(fixtures.len() >= 500, "sweep looks broken: only {} fixtures", fixtures.len());
    assert!(
        counts[CliVerdict::Structural as usize] >= 400,
        "the default leg looks broken: only {} of {} fixtures built structurally",
        counts[CliVerdict::Structural as usize],
        fixtures.len()
    );
}

/// The forced routes agree too: `ALMIDE_WASM_INCUMBENT=1` on the CLI is
/// `force_incumbent` on the library, on a sample of the corpus.
#[cfg_attr(debug_assertions, ignore = "corpus sweep through the CLI is release-only (CI: commissioned wasm gates, release-only root tests)")]
#[test]
fn forced_incumbent_route_matches_on_a_corpus_sample() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("spec/wasm_cross");
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("spec/wasm_cross exists")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("almd"))
        .collect();
    fixtures.sort();
    let scratch = tempfile::tempdir().expect("tempdir");
    let out = scratch.path().join("m.wasm");
    let mut swept = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for fixture in fixtures.iter().step_by(25) {
        let name = fixture.file_name().expect("name").to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&out);
        let output = Command::new(almide_bin())
            .args(["build", fixture.to_str().expect("utf8"), "--target", "wasm", "-o", out.to_str().expect("utf8")])
            .env("ALMIDE_WASM_INCUMBENT", "1")
            .env_remove("ALMIDE_WASM_STRUCTURAL")
            .env_remove("ALMIDE_FUEL_PROBE")
            .env_remove("ALMIDE_COMPONENT_P3")
            .current_dir(fixture.parent().expect("fixture dir"))
            .output()
            .expect("almide runs");
        let text = std::fs::read_to_string(fixture).expect("fixture readable");
        let lib = render_wasm_routed(
            fixture.to_str().expect("utf8"),
            &text,
            ModuleSource::Disk { dep_paths: &[] },
            RouteOptions { library: true, force_incumbent: true, ..RouteOptions::default() },
        );
        match (output.status.success(), lib) {
            (false, Err(_)) => {}
            (true, Ok(m)) => {
                assert_eq!(m.leg, Leg::Incumbent, "{name}: a forced incumbent route is final");
                let shipped = std::fs::read(&out).expect("the CLI wrote its module");
                if shipped != m.bytes {
                    mismatches.push(format!("{name}: incumbent bytes differ ({} vs {})", shipped.len(), m.bytes.len()));
                }
            }
            (cli_ok, lib) => mismatches.push(format!("{name}: CLI success={cli_ok}, library {:?}", lib.map(|m| m.leg))),
        }
        swept += 1;
    }
    assert!(swept >= 20, "sample looks broken: {swept} fixtures");
    assert!(mismatches.is_empty(), "forced-incumbent mismatches:\n{}", mismatches.join("\n"));
}
