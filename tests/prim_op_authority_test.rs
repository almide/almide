//! `@prim_op` is stdlib-only (#4006): it declares what a prim fn IS to the MIR
//! lowering, so a user file writing it is E085, the same authority gate as
//! `@intrinsic`. The control is the same file without the attribute.

use std::process::Command;

fn check(name: &str, src: &str) -> (Option<i32>, String) {
    let dir = std::env::temp_dir().join(format!("almide-prim-op-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let file = dir.join("app.almd");
    std::fs::write(&file, src).expect("write");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("check")
        .arg(&file)
        .output()
        .expect("spawn almide");
    let _ = std::fs::remove_dir_all(&dir);
    (out.status.code(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

const BODY: &str = "fn twice(a: Int) -> Int = a + a\n\nfn main() -> Unit = println(int.to_string(twice(2)))\n";

#[test]
fn a_user_prim_op_is_e085() {
    let (code, out) = check("user", &format!("@prim_op(load, width = 4)\n{BODY}"));
    assert_ne!(code, Some(0), "{out}");
    assert!(out.contains("E085"), "{out}");
    assert!(out.contains("@prim_op on fn 'twice' outside the stdlib"), "{out}");
}

#[test]
fn the_same_file_without_it_checks() {
    let (code, out) = check("control", BODY);
    assert_eq!(code, Some(0), "{out}");
}
