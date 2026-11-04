//! The generated repr fns on the certified native render (#4006): `${v}` over
//! a custom variant calls `__repr_<V>`, over a named record outside the inline
//! expansion `__repr_rec_<R>`, and the render synthesizes each body from the
//! declarations (crates/almide-mir/src/lower/repr_synth.rs) as a certified fn.
//! Each pin compares the v1 render's output with the v0 pipeline's, the
//! oracle the generated bodies mirror (`render_repr_variant_arm`).

use std::path::PathBuf;
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almd_native_repr_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// stdout + exit code of the v0 native pipeline (the oracle).
fn run_v0(name: &str, src: &str) -> (String, i32) {
    let file = scratch(name).join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["run", "--no-verified", file.to_str().unwrap()])
        .env("ALMIDE_NO_VERIFIED_OK", "1")
        .output()
        .expect("almide run");
    (String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

/// The v1 render, and its stdout + exit code once rustc compiled it.
fn run_v1(name: &str, src: &str) -> (String, String, i32) {
    let rust = almide_mir::pipeline::try_render_rust_source(src)
        .unwrap_or_else(|e| panic!("v1 native walled on `{name}`: {e:?}"));
    let dir = scratch(name);
    let (rs, bin) = (dir.join("prog.rs"), dir.join("prog_bin"));
    std::fs::write(&rs, &rust).unwrap();
    let rc = Command::new("rustc").args(["-O", "-o", bin.to_str().unwrap(), rs.to_str().unwrap()]).output().expect("rustc");
    assert!(rc.status.success(), "rustc rejected `{name}`:\n{}", String::from_utf8_lossy(&rc.stderr));
    let out = Command::new(&bin).output().expect("run v1 binary");
    (rust, String::from_utf8_lossy(&out.stdout).into_owned(), out.status.code().unwrap_or(-1))
}

#[test]
fn a_recursive_variant_and_a_record_case_print_as_v0_does() {
    let src = "type Tree = | Leaf(Int) | Node(Tree, Tree)\n\
               type Shape = | Circle(Int) | Flag(Bool) | Rect { w: Int, h: Int } | Dot\n\n\
               effect fn main() -> Unit = {\n  \
                 println(\"${Node(Leaf(1), Node(Leaf(2), Leaf(-3)))}\")\n  \
                 println(\"${Rect { w: 3, h: 4 }}\")\n  \
                 println(\"${Circle(-2)} ${Flag(true)} ${Dot}\")\n\
               }\n";
    let (rust, out, code) = run_v1("variants", src);
    assert!(rust.contains("fn almd___repr_Tree(") || rust.contains("__repr_Tree("), "no generated repr fn:\n{rust}");
    assert_eq!((out, code), run_v0("variants", src), "v1 native differs from v0");
}
