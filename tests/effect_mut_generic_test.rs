//! #1622: the never-err half of `effect fn` protocol methods with `mut self`
//! reached through a generic bound lowers on BOTH wasm legs (the fixpoint
//! never-err scan admits an effect callee with no err arm, and `!` over an
//! admitted callee is a pass-through); the CAN-ERR half keeps its honest
//! wall, and that wall names #1576's design question instead of C-132's
//! brick. The cross-target value parity of the green half is pinned by
//! spec/wasm_cross/effect_mut_generic_port.almd; this gate pins the ROUTING
//! boundary from both sides.

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

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

const NEVER_ERR: &str = r#"protocol Store { effect fn put(mut self: Self, k: String) -> Unit }

type Mem: Store = { ks: List[String] }

effect fn Mem.put(mut self: Mem, k: String) -> Unit = { self.ks = self.ks + [k] }

effect fn uc[S: Store](mut s: S, k: String) -> Int = {
  s.put(k)!
  1
}

effect fn main() -> Unit = {
  var m = Mem { ks: [] }
  let n = uc(m, "a")!
  println("${n} ${m.ks}")
}
"#;

/// The same shape with a direct err arm in the GENERIC fn — the can-err
/// half, whose mut-param semantics await #1576's ruling.
const CAN_ERR: &str = r#"protocol Store { effect fn put(mut self: Self, k: String) -> Unit }

type Mem: Store = { ks: List[String] }

effect fn Mem.put(mut self: Mem, k: String) -> Unit = { self.ks = self.ks + [k] }

effect fn uc[S: Store](mut s: S, k: String) -> Int = {
  guard k != "" else err("empty key")
  s.put(k)!
  1
}

effect fn main() -> Unit = {
  var m = Mem { ks: [] }
  let n = uc(m, "a")!
  println("${n} ${m.ks}")
}
"#;

fn write_fixture(name: &str, src: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("almide-effect-mut-generic");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join(name);
    std::fs::write(&path, src).expect("write");
    path
}

#[test]
fn never_err_effect_mut_method_via_bound_lowers_structurally() {
    if !tools_available() {
        return;
    }
    let path = write_fixture("never_err.almd", NEVER_ERR);
    let native = Command::new(almide_bin())
        .args(["run", path.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert!(
        native.status.success(),
        "native run failed:\n{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let expected = String::from_utf8_lossy(&native.stdout).to_string();
    assert_eq!(expected, "1 [\"a\"]\n");

    // Forced-structural: a wall is a hard error, so success proves the
    // structural leg itself lowered the shape (#1622's decline is gone).
    let forced = Command::new(almide_bin())
        .args(["run", path.to_str().unwrap(), "--target", "wasm"])
        .env("ALMIDE_WASM_STRUCTURAL", "1")
        .output()
        .expect("spawn");
    assert!(
        forced.status.success(),
        "structural leg walled on the never-err shape (#1622 regressed):\n{}",
        String::from_utf8_lossy(&forced.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&forced.stdout),
        expected,
        "wasm/native divergence on the never-err shape"
    );

    // The incumbent leg lowers it too (the same shared C-132 rewrite).
    let incumbent = Command::new(almide_bin())
        .args(["run", path.to_str().unwrap(), "--target", "wasm"])
        .env("ALMIDE_WASM_INCUMBENT", "1")
        .output()
        .expect("spawn");
    assert!(
        incumbent.status.success(),
        "incumbent leg walled on the never-err shape:\n{}",
        String::from_utf8_lossy(&incumbent.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&incumbent.stdout), expected);
}

#[test]
fn can_err_half_keeps_an_honest_wall_naming_1576() {
    if !tools_available() {
        return;
    }
    let path = write_fixture("can_err.almd", CAN_ERR);
    // Native runs it — the wall is a wasm-leg coverage boundary, not a
    // language error.
    let native = Command::new(almide_bin())
        .args(["run", path.to_str().unwrap()])
        .output()
        .expect("spawn");
    assert!(native.status.success());

    let wasm = Command::new(almide_bin())
        .args(["run", path.to_str().unwrap(), "--target", "wasm"])
        .output()
        .expect("spawn");
    let stderr = String::from_utf8_lossy(&wasm.stderr);
    assert!(
        !wasm.status.success(),
        "the can-err half unexpectedly lowered — #1576 is unratified, this \
         must stay an honest wall until it is"
    );
    assert!(
        stderr.contains("#1576"),
        "the can-err wall must name #1576's design question, not a missing \
         brick:\n{stderr}"
    );
}
