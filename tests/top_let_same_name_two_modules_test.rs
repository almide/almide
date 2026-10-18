//! Two modules may each declare a top-level `let` of the same name, with
//! different types. A use of one must get its own module's type.
//!
//! #3688 (nightly fuzz, NativeBuildFailure): `m0.cell` is a function value and
//! `m1.cell` a Matrix. The call `m0.cell(…)` is lowered with an Unknown
//! callee type, and ConcretizeTypes filled Unknown use-site types from a
//! table of every module's top-let types keyed by the bare name. `m0.cell`
//! got `m1.cell`'s Matrix type, the native leg wrapped the closure in
//! `AlmideRcCow::<AlmideMatrix>::from(…)`, and rustc refused it (E0277,
//! E0618). The wasm leg printed the right answer.
use std::process::Command;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("scratch");
    let p = dir.path();
    std::fs::create_dir_all(p.join("src")).expect("src");
    std::fs::write(p.join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n").expect("manifest");
    std::fs::write(p.join("src/m0.almd"), "fn apply1(f: () -> Int, x: Int) -> Int = f() + x\nlet cell = apply1\n").expect("m0");
    std::fs::write(p.join("src/m1.almd"), "let cell = matrix.zeros(2, 3)\n").expect("m1");
    std::fs::write(
        p.join("src/main.almd"),
        "import self.m0\nimport self.m1\n\neffect fn main() -> Unit = {\n  println(int.to_string(m0.cell(() => 1, 2) + matrix.rows(m1.cell)))\n}\n",
    )
    .expect("main");
    dir
}

#[test]
fn each_module_top_let_keeps_its_own_type_on_both_legs() {
    let dir = project();
    for target in [None, Some("wasm")] {
        let mut c = Command::new(env!("CARGO_BIN_EXE_almide"));
        c.current_dir(dir.path()).arg("run").arg("src/main.almd");
        if let Some(t) = target {
            c.args(["--target", t]);
        }
        let out = c.output().expect("run");
        assert!(out.status.success(), "{target:?}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout), "5\n", "{target:?}");
    }
}
