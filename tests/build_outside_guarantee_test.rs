//! `almide build` names, on its `Built` line, every place where the program
//! leaves the memory-safety guarantee (#4008): an `@extern` fn, whose body is
//! foreign code, a `@region(len = …)` clause, the binding's unchecked word for
//! a foreign buffer's length, and a `Handle`, the opaque token only foreign
//! code reads through (ADR-0031). A `Region` access through `ffi.read` /
//! `ffi.write` is bounds-checked, so it is inside the guarantee. A build that
//! stays inside it prints the line it always printed. The same set is
//! what `--profile critical` refuses (E100); this makes it visible in every
//! profile, including in a module the entry file imports.
use std::path::Path;
use std::process::Command;

const NOTE: &str = "outside the memory-safety guarantee";

fn built_line(dir: &Path, file: &str, wasm: bool) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.current_dir(dir).args(["build", file, "-o", if wasm { "out.wasm" } else { "out" }]);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    let out = cmd.output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{stderr}");
    stderr.lines().find(|l| l.starts_with("Built ")).unwrap_or_else(|| panic!("no Built line:\n{stderr}")).to_string()
}

fn write(dir: &Path, name: &str, src: &str) {
    let p = dir.join(name);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, src).unwrap();
}

const FFI: &str = "import ffi\n\neffect fn main() -> Unit = {\n  let b = bytes.from_list([1, 2, 3])\n  let c = ffi.read(ffi.region(b), 0, 3)!\n  println(int.to_string(bytes.len(c)))\n}\n";

#[test]
fn a_program_inside_the_guarantee_prints_the_plain_line() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.almd", "effect fn main() -> Unit = println(\"hello\")\n");
    for wasm in [false, true] {
        let line = built_line(dir.path(), "main.almd", wasm);
        assert!(!line.contains(NOTE), "{line}");
    }
}

/// ADR-0031: a Region read is checked against the length its mint knew, so a
/// program that only reads and writes Regions stays inside the guarantee.
#[test]
fn a_bounds_checked_region_access_is_inside_the_guarantee_on_both_targets() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "main.almd", FFI);
    for wasm in [false, true] {
        let line = built_line(dir.path(), "main.almd", wasm);
        assert!(!line.contains(NOTE), "{line}");
    }
}

#[test]
fn a_region_length_clause_is_named() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "main.almd",
        "import ffi\n\n@extern(c, \"c\", \"malloc\")\n@region(len = n, release = \"free\")\nfn c_malloc(n: UInt64) -> Region\n\n@extern(c, \"c\", \"tmpfile\")\nfn c_tmpfile() -> Handle\n\nfn keep(h: Handle) -> Handle = h\n\neffect fn main() -> Unit = {\n  let r = c_malloc(8)\n  let _h = keep(c_tmpfile())\n  println(int.to_string(ffi.len(r)))\n}\n",
    );
    let line = built_line(dir.path(), "main.almd", false);
    assert!(line.contains("@extern fn `c_malloc` (main.almd:5)"), "{line}");
    assert!(line.contains("@region length of `c_malloc` (main.almd:5)"), "{line}");
}

#[test]
fn an_extern_fn_is_named() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "main.almd",
        "@extern(c, \"c\", \"labs\")\nfn c_labs(x: Int) -> Int\n\neffect fn main() -> Unit = println(int.to_string(c_labs(-3)))\n",
    );
    let line = built_line(dir.path(), "main.almd", false);
    assert!(line.contains("@extern fn `c_labs` (main.almd:2)"), "{line}");
}

#[test]
fn a_handle_in_an_imported_module_is_named_with_that_module() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "almide.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\n");
    write(
        dir.path(),
        "src/leak.almd",
        "@extern(c, \"c\", \"tmpfile\")\nfn tmp() -> Handle\nfn open() -> Handle = tmp()\n",
    );
    write(
        dir.path(),
        "src/main.almd",
        "import self.leak\neffect fn main() -> Unit = {\n  let _h = leak.open()\n  println(\"ok\")\n}\n",
    );
    let line = built_line(dir.path(), "src/main.almd", false);
    assert!(line.contains("a Handle in fn `open` (src/leak.almd:3)"), "{line}");
}
