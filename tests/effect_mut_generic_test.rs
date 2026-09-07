//! #1622: `effect fn` protocol methods with `mut self` reached through a
//! generic bound lower on the structural wasm leg in BOTH halves — the
//! never-err half (#1622, both legs) and the can-err half (#1576's ruling:
//! `(T, Buf)` rides the ok payload only, the err propagates before any
//! write-back). The cross-target value parity is pinned by
//! spec/wasm_cross/effect_mut_generic_port.almd (never-err) and
//! spec/wasm_cross/mut_param_effect_can_err.almd (can-err); this gate pins
//! the ROUTING from both sides: the never-err half on both legs, the can-err
//! half on the structural leg (the incumbent walls the synthesized
//! `let (r, b) = call!` destructure honestly — a walled-real baseline row).

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
/// half, admitted by #1576's ruling (the tuple rides the ok payload only).
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
fn can_err_half_lowers_structurally_under_the_1576_ruling() {
    if !tools_available() {
        return;
    }
    let path = write_fixture("can_err.almd", CAN_ERR);
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
    // structural leg lowered the can-err shape itself.
    let forced = Command::new(almide_bin())
        .args(["run", path.to_str().unwrap(), "--target", "wasm"])
        .env("ALMIDE_WASM_STRUCTURAL", "1")
        .output()
        .expect("spawn");
    assert!(
        forced.status.success(),
        "structural leg walled on the can-err shape (#1576 regressed):\n{}",
        String::from_utf8_lossy(&forced.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&forced.stdout),
        expected,
        "wasm/native divergence on the can-err shape"
    );

    // The incumbent may still wall the synthesized destructure-unwrap, but
    // it must never emit different bytes: either the same stdout, or an
    // honest refusal.
    let incumbent = Command::new(almide_bin())
        .args(["run", path.to_str().unwrap(), "--target", "wasm"])
        .env("ALMIDE_WASM_INCUMBENT", "1")
        .output()
        .expect("spawn");
    if incumbent.status.success() {
        assert_eq!(String::from_utf8_lossy(&incumbent.stdout), expected);
    } else {
        assert!(
            String::from_utf8_lossy(&incumbent.stderr).contains("not yet supported"),
            "the incumbent neither lowered nor walled honestly:\n{}",
            String::from_utf8_lossy(&incumbent.stderr)
        );
    }
}
