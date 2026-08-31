//! NATIVE-leg regression pins for #1674: derive suppression for closure-
//! carrying types is TRANSITIVE.
//!
//! A type whose payload is a function value already dropped
//! `Debug`/`PartialEq`/`AlmideRepr` — but the analysis was one level deep, so
//! any type CONTAINING that type (variant payload, `List[...]`, record field,
//! or a two-hop chain) still derived them and the generated Rust failed rustc
//! (E0277 `Check: Debug`, E0369 `==` on `&Check`, E0599 `almide_repr`). This
//! is the representation every schema/validation library reaches for: nodes
//! carrying user predicates, trees built from those nodes.
//!
//! Native pins per the tests/native_mut_param_pins_test.rs doctrine (the wasm
//! leg does not share this derive machinery; `almide run` here is the NATIVE
//! target). A/B-verified: each shape fails the generated-Rust build on the
//! pre-fix compiler.

use std::io::Write;
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

/// Write `src` as a single-file program, `almide run` (NATIVE) it, assert it
/// prints `expected`.
fn run_prints(name: &str, src: &str, expected: &str) {
    let dir = std::env::temp_dir().join(format!("almd_fn_derive_pin_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.almd");
    let mut f = std::fs::File::create(&file).unwrap();
    f.write_all(src.as_bytes()).unwrap();
    let out = Command::new(almide_bin())
        .args(["run", file.to_str().unwrap()])
        .output()
        .expect("failed to spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        out.status.success(),
        "[{name}] almide run (native) failed — transitive fn-derive suppression regressed?\n{stderr}"
    );
    assert_eq!(stdout.trim_end(), expected, "[{name}] wrong output");
}

#[test]
fn variant_containing_fn_carrying_type_builds_native() {
    run_prints(
        "variant",
        "type Check = | Email | Refine((Int) -> Bool)\n\
         type Schema = | Str(Check)\n\
         fn main() -> Unit = println(match Str(Refine((n) => n > 0)) { Str(_) => \"ok\" })\n",
        "ok",
    );
}

#[test]
fn list_of_fn_carrying_type_in_payload_builds_native() {
    run_prints(
        "list",
        "type Check = | Email | Refine((Int) -> Bool)\n\
         type Schema = | Str(List[Check])\n\
         fn main() -> Unit = println(match Str([Refine((n) => n > 0)]) { Str(_) => \"ok\" })\n",
        "ok",
    );
}

#[test]
fn record_field_of_fn_carrying_type_builds_native() {
    run_prints(
        "record",
        "type Check = | Email | Refine((Int) -> Bool)\n\
         type Schema = { checks: List[Check] }\n\
         fn main() -> Unit = {\n\
           let s = Schema { checks: [Refine((n) => n > 0)] }\n\
           println(int.to_string(list.len(s.checks)))\n\
         }\n",
        "1",
    );
}

#[test]
fn two_hop_chain_through_named_types_builds_native() {
    run_prints(
        "two_hop",
        "type Check = | Email | Refine((Int) -> Bool)\n\
         type Node = | Leaf(Check) | Empty\n\
         type Tree = { root: Node, tag: String }\n\
         fn main() -> Unit = {\n\
           let t = Tree { root: Leaf(Refine((n) => n > 0)), tag: \"t\" }\n\
           println(t.tag)\n\
         }\n",
        "t",
    );
}
