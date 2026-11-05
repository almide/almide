//! The native leg's capability report agrees with the wasm leg's (#4006).
//!
//! A host op joins the native floor only as a `host_ops::HOST_OPS` row whose
//! capabilities the certificate counts at its call site (the owner's ruling on
//! the native IO family). This holds that accounting to the wasm leg's for the
//! same program: the capabilities `main` reaches on the native render
//! (`almide_mir::pipeline::native_capability_reach`, over the MIR the native
//! renderer rendered from) equal the reach of `main`'s `caps` witness in the
//! wasm certificate bundle (`almide verify --emit`, `cert_project::caps`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_cross").join(name)
}

/// `main`'s reach in the wasm bundle: its `caps` witness is `declared|reach`.
fn wasm_main_reach(path: &Path) -> BTreeSet<u32> {
    let out = std::env::temp_dir().join(format!(
        "almide-caps-parity-{}-{}.bundle",
        std::process::id(),
        path.file_stem().unwrap().to_string_lossy()
    ));
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("verify")
        .arg(path)
        .arg("--emit")
        .arg(&out)
        .output()
        .expect("spawn almide verify");
    let bytes = std::fs::read(&out).unwrap_or_else(|e| {
        panic!("{}: no bundle written ({e}): {}", path.display(), String::from_utf8_lossy(&o.stderr))
    });
    let _ = std::fs::remove_file(&out);
    let bundle = almide_verify::bundle::parse(&bytes).expect("a bundle the verifier's parser reads");
    let w = bundle
        .witnesses
        .iter()
        .find(|w| w.function == "main" && w.property.name() == "caps")
        .unwrap_or_else(|| panic!("{}: no caps witness for main", path.display()));
    let text = String::from_utf8_lossy(&w.bytes).trim().to_string();
    let reach = text.split('|').nth(1).unwrap_or_else(|| panic!("caps witness `{text}` has no reach side"));
    reach.split_whitespace().map(|c| c.parse().unwrap_or_else(|_| panic!("capability id `{c}`"))).collect()
}

fn native_main_reach(path: &Path) -> BTreeSet<u32> {
    let src = std::fs::read_to_string(path).expect("read fixture");
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(move || almide_mir::pipeline::native_capability_reach(&src))
        .expect("spawn")
        .join()
        .expect("render thread")
        .unwrap_or_else(|e| panic!("{}: walls on the native render: {e:?}", path.display()))
}

/// Programs on the native floor that reach a host op through it, and two
/// that reach only the console.
const PROGRAMS: &[&str] = &[
    "env_args.almd",
    "datetime_monotonic_ns.almd",
    "env_sleep_pause.almd",
    "env_platform_reporting.almd",
    "effect_unit_yields_result.almd",
    // (grain_gc_shapes left the floor: its tail call passes a heap arg past a
    // typed release, which the render walls, #3550.)
    "while_loops.almd",
];

#[test]
fn native_and_wasm_capability_reports_agree() {
    let mut problems = Vec::new();
    for name in PROGRAMS {
        let path = fixture(name);
        let (native, wasm) = (native_main_reach(&path), wasm_main_reach(&path));
        if native != wasm {
            problems.push(format!("{name}: native reaches {native:?}, wasm reaches {wasm:?}"));
        }
    }
    assert!(problems.is_empty(), "capability reports disagree:\n{}", problems.join("\n"));
}

#[test]
fn a_host_op_program_reaches_its_capability_natively() {
    // `env.args` reads argv: CliArgs (id 2), counted at the native call site.
    let native = native_main_reach(&fixture("env_args.almd"));
    assert!(native.contains(&almide_mir::Capability::CliArgs.id()), "{native:?}");
}
