//! NATIVE-leg regression pins for the #1549/#1550/#1551 batch (23a679a03).
//!
//! All three were NATIVE Rust codegen bugs (check green, rustc E0308/E0596):
//! the spec/ fixtures carrying the same shapes run on the WASM leg — which
//! never executes the broken code path — so a corpus file alone asserts
//! nothing for them (A/B-verified: the pre-fix compiler passes the wasm-leg
//! fixtures). Same doctrine as tests/effect_tail_generic_bound_test.rs:
//! a RUST-target property pins at the compiler level, spec/ pins the
//! cross-target behavior.
//!
//! Each test compiles + runs the exact repro shape on the NATIVE target and
//! asserts the output; on the pre-fix compiler each fails at the generated-
//! Rust build (A/B-verified against 23a679a03^ via ALMIDE_BIN).

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

/// Write `files` under a temp package dir, `almide run` (NATIVE target) the
/// first one, assert it prints `expected`.
fn run_prints(name: &str, files: &[(&str, &str)], expected: &str) {
    let dir = std::env::temp_dir().join(format!("almd_ddd_pins_{name}_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    // `import self.m` resolves through the package manifest.
    std::fs::write(
        dir.join("almide.toml"),
        "[package]\nname = \"pins\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    for (rel, src) in files {
        let file = dir.join(rel);
        let mut f = std::fs::File::create(&file).unwrap();
        f.write_all(src.as_bytes()).unwrap();
    }
    let entry = dir.join(files[0].0);
    let out = Command::new(almide_bin())
        .args(["run", entry.to_str().unwrap()])
        .output()
        .expect("failed to spawn almide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        out.status.success(),
        "[{name}] almide run (native) failed — the pinned native codegen bug is back?\n{stderr}"
    );
    assert_eq!(stdout.trim_end(), expected, "[{name}] wrong output");
}

/// #1549: a convention method's receiver BORROW MODE crosses the module
/// boundary — `fn Box.twice(self)` in a submodule is emitted by-reference,
/// and a method-form call from the main module used to pass by value
/// (rustc E0308 pre-fix).
#[test]
fn cross_module_receiver_borrow_mode_native() {
    run_prints(
        "recv_mode",
        &[
            (
                "src/main.almd",
                "import self.m\nimport io\n\n\
                 effect fn main() -> Unit = {\n  \
                   io.print(int.to_string(m.Box { n: 21 }.twice()))\n  \
                   let b = m.Box { n: 5 }\n  \
                   io.print(int.to_string(b.twice()))\n}\n",
            ),
            ("src/m.almd", "type Box = { n: Int }\n\nfn Box.twice(self) -> Int = self.n * 2\n"),
        ],
        "4210",
    );
}

/// #1549 (mut leg): the MUT-receiver convention's signature must mirror under
/// both the dotted and bare emit keys, or the cross-module caller's borrow
/// classification misses it (rustc E0308 pre-fix).
#[test]
fn cross_module_mut_receiver_native() {
    run_prints(
        "mut_recv",
        &[
            (
                "src/main.almd",
                "import self.m\nimport io\n\n\
                 effect fn main() -> Unit = {\n  \
                   var b = m.Box { n: 1 }\n  \
                   b.bump(4)\n  \
                   io.print(int.to_string(b.n))\n  \
                   b.bump(2)\n  \
                   io.print(int.to_string(b.n))\n}\n",
            ),
            (
                "src/m.almd",
                "type Box = { n: Int }\n\n\
                 fn Box.bump(mut self: Box, by: Int) -> Unit = { self.n = self.n + by }\n",
            ),
        ],
        "57",
    );
}

/// #1550: a WHOLE-parameter assignment to a `mut` param must emit the
/// write-back through the &mut binding (`*p = …`), not a plain rebind
/// (rustc E0308 pre-fix; the caller would also never see the new value).
#[test]
fn mut_param_whole_assign_native() {
    run_prints(
        "whole_assign",
        &[(
            "src/main.almd",
            "import io\n\n\
             type Box = { n: Int }\n\n\
             fn swap_in(mut b: Box, by: Int) -> Unit = { b = Box { n: b.n + by } }\n\n\
             effect fn main() -> Unit = {\n  \
               var b = Box { n: 1 }\n  \
               swap_in(b, 4)\n  \
               io.print(int.to_string(b.n))\n  \
               swap_in(b, 15)\n  \
               io.print(int.to_string(b.n))\n}\n",
        )],
        "520",
    );
}

/// #1551: monomorphizing a `mut` param with a protocol bound must keep its
/// by-reference calling convention (rustc E0596 pre-fix), and the mutation
/// must be visible at the caller after the generic call.
#[test]
fn mut_param_protocol_bound_native() {
    run_prints(
        "proto_bound",
        &[(
            "src/main.almd",
            "import io\n\n\
             protocol Counter {\n  \
               fn bump(mut self: Self, by: Int) -> Unit\n  \
               fn read(self) -> Int\n}\n\n\
             type Tally: Counter = { n: Int }\n\n\
             fn Tally.bump(mut self: Tally, by: Int) -> Unit = { self.n = self.n + by }\n\n\
             fn Tally.read(self) -> Int = self.n\n\n\
             fn bump_twice[C: Counter](mut c: C) -> Int = {\n  \
               c.bump(1)\n  \
               c.bump(2)\n  \
               c.read()\n}\n\n\
             effect fn main() -> Unit = {\n  \
               var t = Tally { n: 10 }\n  \
               let r = bump_twice(t)\n  \
               io.print(int.to_string(r))\n  \
               io.print(int.to_string(t.n))\n}\n",
        )],
        "1313",
    );
}
