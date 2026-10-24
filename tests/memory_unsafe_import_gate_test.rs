//! #4043: outside the stdlib, a call to a `@memory_unsafe` stdlib fn (`ffi.*`,
//! the deprecated `bytes` raw-pointer spellings) is E100 unless the calling
//! FILE writes `import ffi`. The import is the opt-in, so a program whose files
//! have no `import ffi` and no `@extern` cannot read or write memory outside
//! its values, on any target. These are the issue's two probes plus every
//! spelling of a call (direct, pipe stage, fn value, selective import) and the
//! per-file scope of the opt-in.

use std::path::Path;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// `almide check main.almd` in a fresh dir holding `files`; (success, stderr).
fn check(dir_name: &str, files: &[(&str, &str)]) -> (bool, String) {
    let dir = std::env::temp_dir().join("almide-memory-unsafe-gate").join(dir_name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    for (name, src) in files {
        std::fs::write(dir.join(name), src).expect("write");
    }
    let out = Command::new(almide())
        .arg("check")
        .arg(Path::new("main.almd"))
        .current_dir(&dir)
        .output()
        .expect("spawn almide check");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string())
}

const OOB_READ: &str = "fn main() -> Unit = {\n  let small = bytes.from_list([1, 2, 3, 4])\n  let leaked = bytes.from_raw_ptr(bytes.as_ptr(small), 4096)\n  println(int.to_string(bytes.len(leaked)))\n}\n";

const OOB_WRITE: &str = "fn main() -> Unit = {\n  let small = bytes.from_list([1, 2, 3, 4])\n  let big = bytes.from_list(list.repeat(65, 4096))\n  let n = bytes.copy_to_ptr(big, bytes.as_ptr(small), 4096)\n  println(int.to_string(n))\n  println(int.to_string(bytes.len(small)))\n}\n";

#[test]
fn the_issue_probes_are_refused_without_import_ffi() {
    for (what, src, callee, rewrite) in [
        ("oob_read", OOB_READ, "`bytes.from_raw_ptr`", "`ffi.read`"),
        ("oob_write", OOB_WRITE, "`bytes.copy_to_ptr`", "`ffi.write`"),
    ] {
        let (ok, err) = check(what, &[("main.almd", src)]);
        assert!(!ok, "{what}: accepted without `import ffi`:\n{err}");
        assert!(err.contains("error[E100]"), "{what}: expected E100:\n{err}");
        assert!(err.contains(&format!("{callee} is outside the memory-safety guarantee and needs `import ffi`")), "{what}:\n{err}");
        assert!(err.contains("`bytes.as_ptr` is outside the memory-safety guarantee"), "{what}: the address is refused too:\n{err}");
        assert!(err.contains(&format!("write {rewrite} in place of the deprecated {callee}")), "{what}: the hint names no rewrite:\n{err}");
        // The opt-in: the same file with `import ffi` checks, the deprecated
        // spellings keep only their E052 warning.
        let (ok, err) = check(&format!("{what}_opted_in"), &[("main.almd", &format!("import ffi\n\n{src}"))]);
        assert!(ok, "{what}: refused with `import ffi`:\n{err}");
        assert!(err.contains("E052") && !err.contains("E100") && !err.contains("E060"), "{what}:\n{err}");
    }
}

#[test]
fn every_spelling_of_the_call_is_gated() {
    for (what, body) in [
        ("pipe stage", "  let p = b |> bytes.as_ptr\n"),
        ("fn value", "  let f = bytes.data_ptr\n  let n = f(b)\n"),
        ("direct call", "  let n = bytes.data_ptr(b)\n"),
    ] {
        let src = format!("fn main() -> Unit = {{\n  let b = bytes.from_string(\"abc\")\n{body}  println(\"ok\")\n}}\n");
        let (ok, err) = check("spelling", &[("main.almd", &src)]);
        assert!(!ok && err.contains("error[E100]") && err.contains("needs `import ffi`"), "{what}:\n{err}");
    }
    let selective = "import bytes.{as_ptr}\n\nfn main() -> Unit = {\n  let b = bytes.from_string(\"abc\")\n  let p = as_ptr(b)\n  println(\"ok\")\n}\n";
    let (ok, err) = check("selective", &[("main.almd", selective)]);
    assert!(!ok && err.contains("`bytes.as_ptr` is outside the memory-safety guarantee"), "selective import:\n{err}");
}

/// `ffi.read` with `import ffi` is the supported spelling and checks clean.
#[test]
fn ffi_with_its_import_checks() {
    let src = "import ffi\n\neffect fn main() -> Unit = {\n  let b = bytes.from_list([1, 2, 3, 4])\n  let c = ffi.read(ffi.address(b), 4)!\n  println(int.to_string(bytes.len(c)))\n}\n";
    let (ok, err) = check("ffi_ok", &[("main.almd", src)]);
    assert!(ok && !err.contains("warning"), "{err}");
}

/// The opt-in is per FILE: `import ffi` in main does not cover a module that
/// main imports, so the module that touches raw memory says so itself.
#[test]
fn the_opt_in_does_not_cross_files() {
    let main = "import ffi\nimport helper\n\nfn main() -> Unit = {\n  let b = bytes.from_string(\"abc\")\n  println(int.to_string(helper.peek(b)))\n}\n";
    let helper = "fn peek(b: Bytes) -> Int = bytes.len(bytes.from_raw_ptr(bytes.as_ptr(b), 4096))\n";
    let (ok, err) = check("per_file", &[("main.almd", main), ("helper.almd", helper)]);
    assert!(!ok && err.contains("helper.almd") && err.contains("error[E100]"), "{err}");
}

/// A local binding named like the module is not the module.
#[test]
fn a_shadowing_binding_is_not_the_module() {
    let src = "fn main() -> Unit = {\n  let bytes = { as_ptr: (x: Int) => x + 1 }\n  println(int.to_string(bytes.as_ptr(1)))\n}\n";
    let (_, err) = check("shadow", &[("main.almd", src)]);
    assert!(!err.contains("E100"), "{err}");
}
