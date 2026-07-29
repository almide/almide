//! CI tool-arming tripwire (#983).
//!
//! Every tool-gated suite in tests/ returns early AS A PASS when its tool is
//! missing — right for local dev, but it let CI legs lose their tools
//! silently: build-cross never installed wasm-opt on macOS, Windows never had
//! wasmtime, and a future install regression on the armed Linux leg would
//! read as green while 18 suites quietly stopped testing anything.
//!
//! Jobs that CLAIM full tool coverage set `ALMIDE_EXPECT_TOOLS=1`; under that
//! flag a missing tool is a FAILURE here, converting the silence to red on
//! exactly the legs that advertise the coverage. Unarmed environments (local
//! dev, feature CI, Windows — the recorded #983 decision) leave the variable
//! unset and this test is a no-op.

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

#[test]
fn armed_jobs_actually_have_their_tools() {
    if std::env::var("ALMIDE_EXPECT_TOOLS").ok().as_deref() != Some("1") {
        return; // nothing claimed, nothing to enforce
    }
    for tool in ["wasmtime", "wasm-opt"] {
        assert!(
            Command::new(tool).arg("--version").output().is_ok(),
            "ALMIDE_EXPECT_TOOLS=1 but `{tool}` is not runnable — this job claims full \
             tool coverage, so the tool-gated suites would silently skip-as-pass. \
             Install it or drop the claim (#983)."
        );
    }
    assert!(
        Command::new(almide_bin()).arg("--version").output().is_ok(),
        "ALMIDE_EXPECT_TOOLS=1 but the almide binary is not runnable — every \
         tool-gated suite would silently skip-as-pass (#983)."
    );
}
