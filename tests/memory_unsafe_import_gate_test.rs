//! #4043: outside the stdlib, a call to a `@memory_unsafe` stdlib fn (the
//! `ffi` surface: Regions, ADR-0031) is refused unless the calling FILE
//! writes `import ffi`. `ffi` is never auto-imported, so without the import
//! the module name itself is unresolved (E003, whose hint is the import);
//! E100 is the refusal for a `@memory_unsafe` fn reached any other way. The import is the opt-in, so a program whose files have no
//! `import ffi` and no `@extern` reaches no memory outside its values, on any
//! target. These are the issue's two probes, now spelled with Regions, plus
//! every spelling of a call (direct, pipe stage, fn value, selective import)
//! and the per-file scope of the opt-in.

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

const OOB_READ: &str = "effect fn main() -> Unit = {\n  let small = bytes.from_list([1, 2, 3, 4])\n  let leaked = ffi.read(ffi.region(small), 0, 4096)!\n  println(int.to_string(bytes.len(leaked)))\n}\n";

const OOB_WRITE: &str = "effect fn main() -> Unit = {\n  let small = ffi.alloc(4)!\n  let big = bytes.from_list(list.repeat(65, 4096))\n  let _ = ffi.write(small, 0, big)\n  println(int.to_string(ffi.len(small)))\n}\n";

#[test]
fn the_issue_probes_are_refused_without_import_ffi() {
    for (what, src, callee) in [("oob_read", OOB_READ, "`ffi.read`"), ("oob_write", OOB_WRITE, "`ffi.write`")] {
        let (ok, err) = check(what, &[("main.almd", src)]);
        assert!(!ok, "{what}: accepted without `import ffi`:\n{err}");
        let _ = callee;
        assert!(err.contains("error[E003]") || err.contains("error[E100]"), "{what}: expected E003/E100:\n{err}");
        assert!(err.contains("import ffi"), "{what}: the hint names no repair:\n{err}");
        // The opt-in: the same file with `import ffi` checks clean.
        let (ok, err) = check(&format!("{what}_opted_in"), &[("main.almd", &format!("import ffi\n\n{src}"))]);
        assert!(ok, "{what}: refused with `import ffi`:\n{err}");
        assert!(!err.contains("E100") && !err.contains("E060"), "{what}:\n{err}");
    }
}

#[test]
fn every_spelling_of_the_call_is_gated() {
    for (what, body) in [
        ("pipe stage", "  let r = b |> ffi.region\n"),
        ("fn value", "  let f = ffi.region\n  let r = f(b)\n"),
        ("direct call", "  let r = ffi.region(b)\n"),
    ] {
        let src = format!("fn main() -> Unit = {{\n  let b = bytes.from_string(\"abc\")\n{body}  println(\"ok\")\n}}\n");
        let (ok, err) = check("spelling", &[("main.almd", &src)]);
        assert!(!ok && (err.contains("error[E003]") || err.contains("error[E100]")) && err.contains("import ffi"), "{what}:\n{err}");
    }
}

/// `ffi.read` with `import ffi` is the supported spelling and checks clean.
#[test]
fn ffi_with_its_import_checks() {
    let src = "import ffi\n\neffect fn main() -> Unit = {\n  let b = bytes.from_list([1, 2, 3, 4])\n  let c = ffi.read(ffi.region(b), 0, 4)!\n  println(int.to_string(bytes.len(c)))\n}\n";
    let (ok, err) = check("ffi_ok", &[("main.almd", src)]);
    assert!(ok && !err.contains("warning"), "{err}");
}

/// The opt-in is per FILE: `import ffi` in main does not cover a module that
/// main imports, so the module that touches memory says so itself.
#[test]
fn the_opt_in_does_not_cross_files() {
    let main = "import ffi\nimport helper\n\nfn main() -> Unit = {\n  let b = bytes.from_string(\"abc\")\n  println(int.to_string(helper.size(b)))\n}\n";
    let helper = "fn size(b: Bytes) -> Int = ffi.len(ffi.region(b))\n";
    let (ok, err) = check("per_file", &[("main.almd", main), ("helper.almd", helper)]);
    assert!(!ok && err.contains("helper.almd") && (err.contains("error[E003]") || err.contains("error[E100]")), "{err}");
}

/// A local binding named like the module is not the module.
#[test]
fn a_shadowing_binding_is_not_the_module() {
    let src = "fn main() -> Unit = {\n  let ffi = { region: (x: Int) => x + 1 }\n  println(int.to_string(ffi.region(1)))\n}\n";
    let (_, err) = check("shadow", &[("main.almd", src)]);
    assert!(!err.contains("E100"), "{err}");
}
