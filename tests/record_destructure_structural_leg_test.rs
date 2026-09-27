//! An anonymous record destructure (`let { a, b } = r`) binds each name at the
//! type of the field it reads, on the structural wasm leg, whatever the
//! binder's IR type says.
//!
//! The destructure names no type, so its binders reach the IR typed from the
//! pattern's empty name, and a later pass can guess them by name. When the
//! entry program's record `Stop` shares its spelling with a module's variant
//! case `| Stop` (legal since #2636), the guess took the case: the binders
//! were `Unit` and the structural leg walled with
//! `ty-mismatch:Unit-vs-Scalar(Str)` while native ran the program. The leg now
//! sizes each binder's local from the subject's layout, the same layout its
//! load reads.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[test]
fn a_destructure_of_a_record_named_like_a_modules_case_runs_on_both_legs() {
    if !Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success()) {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"destr\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(src.join("finish.almd"), "type Finish = | Stop | Length\nfn name(f: Finish) -> String = match f { Stop => \"stop\", Length => \"length\" }\n").unwrap();
    std::fs::write(src.join("middle.almd"), "import self.finish as finish\nfn describe() -> String = finish.name(finish.Stop)\n").unwrap();
    std::fs::write(
        src.join("main.almd"),
        "import self.middle\n\ntype Stop = { message: String, code: Int }\n\n\
         effect fn main() -> Unit = {\n  println(middle.describe())\n  let s = Stop { message: \"halt\", code: 7 }\n  \
         let { message, code } = s\n  println(message + \" \" + int.to_string(code))\n}\n",
    )
    .unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(almide()).current_dir(dir.path()).args(args).output().expect("run");
        (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
    };
    let (ok, native) = run(&["run", "src/main.almd"]);
    assert!(ok && native.ends_with("stop\nhalt 7\n"), "native:\n{native}");
    let (ok, wasm) = run(&["run", "src/main.almd", "--target", "wasm"]);
    assert!(ok && wasm.ends_with("stop\nhalt 7\n"), "wasm:\n{wasm}");
}
