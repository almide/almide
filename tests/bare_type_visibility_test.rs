//! #2715: a bare type or protocol name resolves only among the modules the
//! file can see: its own module, the modules it imports, and the stdlib.
//!
//! The bare-name fallback scanned the whole program. A type declared only by a
//! module the file never imports resolved anyway, and which module's type a
//! bare name meant depended on which other modules were in the program and on
//! the order they registered (the last writer of the program-wide bare key).
//! A file that never mentioned a module could change meaning or stop
//! compiling when that module gained a same-named type. Every cell runs
//! under several import orders so no single order can hide the dependence.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// A package with the given `src/<name>.almd` files.
fn package(files: &[(&str, String)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    std::fs::write(dir.path().join("almide.toml"), "[package]\nname = \"vis\"\nversion = \"0.1.0\"\n").expect("manifest");
    for (name, body) in files {
        std::fs::write(src.join(format!("{name}.almd")), body).expect("module");
    }
    dir
}

fn run(dir: &std::path::Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(almide()).current_dir(dir).args(args).output().expect("run almide");
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

fn imports(order: &[&str]) -> String {
    order.iter().map(|m| format!("import self.{m}\n")).collect()
}

const A: &str = "type X = { a_only: Int }\nfn mk() -> X = X { a_only: 1 }\n";
const B: &str = "type X = { b_only: String }\nfn mk() -> X = X { b_only: \"b\" }\n";
const Z: &str = "type Y = { z_only: Bool }\nfn mk() -> Y = Y { z_only: true }\n";

/// `c` imports `a` (and `z`) and spells `X` bare; `b`, which `c` never
/// imports, declares an `X` of its own and is pulled in by `main`.
fn visible_and_foreign(main_order: &[&str], c_order: &[&str]) -> tempfile::TempDir {
    package(&[
        ("a", A.to_string()),
        ("b", B.to_string()),
        ("z", Z.to_string()),
        ("c", format!(
            "{}\nfn get(x: X) -> Int = x.a_only\nfn make() -> X = X {{ a_only: 5 }}\n\
             fn go() -> Int = get(make()) + (if z.mk().z_only then 1 else 0)\n",
            imports(c_order)
        )),
        ("main", format!(
            "{}\neffect fn main() -> Unit = println(int.to_string(c.go()) + b.mk().b_only)\n",
            imports(main_order)
        )),
    ])
}

#[test]
fn a_bare_name_means_the_imported_modules_type_in_every_order() {
    for main_order in [["b", "c"], ["c", "b"]] {
        for c_order in [["a", "z"], ["z", "a"]] {
            let dir = visible_and_foreign(&main_order, &c_order);
            let (ok, text) = run(dir.path(), &["check", "src/main.almd"]);
            assert!(ok, "main {main_order:?}, c {c_order:?}: must check, got:\n{text}");
        }
    }
}

#[test]
fn a_bare_name_means_the_imported_modules_type_on_both_legs() {
    let dir = visible_and_foreign(&["c", "b"], &["z", "a"]);
    let (ok, native) = run(dir.path(), &["run", "src/main.almd"]);
    assert!(ok && native.ends_with("6b\n"), "native:\n{native}");
    if Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success()) {
        let (ok, wasm) = run(dir.path(), &["run", "src/main.almd", "--target", "wasm"]);
        assert!(ok && wasm.ends_with("6b\n"), "wasm:\n{wasm}");
    }
}

#[test]
fn a_type_only_an_unimported_module_declares_is_e029_naming_the_import() {
    // `main` imports `c` and `z`; only `c` imports `b`, the one module that
    // declares `X`.
    for order in [["c", "z"], ["z", "c"]] {
        let dir = package(&[
            ("b", B.to_string()),
            ("z", Z.to_string()),
            ("c", "import self.b\nfn hello() -> String = b.mk().b_only\n".to_string()),
            ("main", format!(
                "{}\nfn name_of(x: X) -> String = x.b_only\n\n\
                 effect fn main() -> Unit = println(c.hello() + (if z.mk().z_only then \"z\" else \"\"))\n",
                imports(&order)
            )),
        ]);
        let (ok, text) = run(dir.path(), &["check", "src/main.almd"]);
        assert!(!ok, "order {order:?}: a type of an unimported module must not resolve, got success:\n{text}");
        assert!(text.contains("error[E029]") && text.contains("type 'X' is not in scope here"), "expected E029, got:\n{text}");
        assert!(text.contains("`b.X`"), "the hint must give the qualified spelling, got:\n{text}");
    }
}

#[test]
fn a_protocol_only_an_unimported_module_declares_is_e029() {
    let dir = package(&[
        ("p", "protocol Named {\n  fn label(a: Self) -> String\n}\n".to_string()),
        ("c", "import self.p\nfn hello() -> String = \"c\"\n".to_string()),
        ("main", "import self.c\n\ntype Dog: Named = { n: String }\nfn Dog.label(a: Dog) -> String = a.n\n\n\
                  effect fn main() -> Unit = println(c.hello())\n".to_string()),
    ]);
    let (ok, text) = run(dir.path(), &["check", "src/main.almd"]);
    assert!(!ok, "a protocol of an unimported module must not resolve, got success:\n{text}");
    assert!(text.contains("protocol 'Named' is not in scope here") && text.contains("`p.Named`"), "got:\n{text}");
}
