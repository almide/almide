//! A wasm wall renders as E082 plus the ONE machine-readable `wall:` line the
//! nightly fuzzer's honest-wall classifier keys on. (The #931 headline /
//! rewrite-hint / caret rendering was the retired incumbent renderer's
//! `WallShape` machinery; the structural leg's wall names its reason and the
//! function it came from, #2807.)
//!
//! Skips cleanly when the `almide` binary is unavailable (CI builds it in the
//! build step; locally run `cargo build --release` first).

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

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

/// Every wall's stderr must carry the ONE machine-readable line the nightly
/// fuzzer's honest-wall classifier keys on: `wall: <reason>` (the shared
/// `almide::WASM_WALL_MARKER`). The human diagnostic may be reworked freely;
/// dropping this line turns every honest wall into a phantom
/// WasmBuildFailure finding and fails the night on subset debt — which is
/// exactly what happened when #931 reworked the old `wall: {e:?}` form.
fn assert_wall_marker_line(stderr: &str) {
    let marker_lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with(almide::WASM_WALL_MARKER))
        .collect();
    assert!(
        marker_lines.len() == 1,
        "expected exactly one `{}` marker line, got {}:\n{stderr}",
        almide::WASM_WALL_MARKER,
        marker_lines.len()
    );
    let reason = &marker_lines[0][almide::WASM_WALL_MARKER.len()..];
    assert!(!reason.trim().is_empty(), "empty wall reason in marker line:\n{stderr}");
}

/// A shape the wasm leg refuses on purpose (a mut op through a deeper field
/// path; tests/wasm_wall_e082_test.rs pins the refusal's spelling).
const WALLED_SHAPE: &str = r#"type Inner = { xs: List[Int] }
type Outer = { inner: Inner }

effect fn main() -> Unit = {
  var o = Outer { inner: Inner { xs: [1] } }
  list.push(o.inner.xs, 2)
  println("${list.len(o.inner.xs)}")
}
"#;

#[test]
fn a_wasm_wall_is_e082_with_the_marker_line() {
    if !tool_available() {
        eprintln!("skipping: almide binary not available");
        return;
    }
    let dir = std::env::temp_dir().join(format!("almide-wall-marker-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("wall.almd");
    std::fs::write(&src, WALLED_SHAPE).unwrap();
    let output = Command::new(almide_bin())
        .args(["build", src.to_str().unwrap(), "--target", "wasm", "-o", dir.join("wall.wasm").to_str().unwrap()])
        .output()
        .expect("failed to spawn almide");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let wrote = dir.join("wall.wasm").exists();
    std::fs::remove_dir_all(&dir).ok();
    assert!(!output.status.success(), "a walled build must fail, got success.\nstderr: {stderr}");
    assert!(!wrote, "a walled build must leave no artifact behind");
    assert!(stderr.contains("error[E082]"), "the wall is E082:\n{stderr}");
    // No second leg to name: the message is about the one wasm leg.
    assert!(!stderr.contains("incumbent") && !stderr.contains("both wasm legs"), "{stderr}");
    assert_wall_marker_line(&stderr);
}
