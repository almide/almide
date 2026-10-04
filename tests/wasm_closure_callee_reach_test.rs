//! #3296: a fn called only from a closure was dropped from the wasm module
//! (E082 `call:<fn>`) when a declared `@extern(wasm)` import sat earlier in
//! the program's fn order — typically in a dependency package (snaidhm's
//! `gpu` externs ahead of ceangal's editor closures).
//!
//! The emitter records, per program fn, the range of lambdas its body lifted
//! (`fn_lambdas[i]`), and a lambda is reachable only through its owner. The
//! import arm of the per-fn loop left without recording a range, so every fn
//! after the import read its neighbour's range: the closure-building fn got
//! none, its closures' callees were never reached, pass 2 dropped them, and
//! the closure walled. Small scalar callees hid it (they inline), so the
//! callees here recurse and build strings.
//!
//! Pinned: the extern in another dependency package, and in a sibling module
//! of the same package — wasm output identical to native.
use std::path::Path;
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

fn run(app: &Path, wasm: bool) -> (bool, String, String) {
    let mut cmd = Command::new(almide());
    cmd.current_dir(app).args(["run", "src/main.almd"]);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    let out = cmd.output().expect("spawn almide");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The closure-building module: `handlers` returns a record of closures
/// whose bodies call `label` / `shout`, which nothing else calls; `fire`
/// invokes them through a module `var`.
const HANDLERS: &str = r#"type Handlers = { on_a: (Int) -> String, on_b: (Int) -> String }

fn label(x: Int) -> String = if x <= 0 then "zero" else "n" + int.to_string(x) + label(x - 1)

fn shout(x: Int) -> String = string.to_upper(label(x)) + "!"

pub fn handlers(k: Int) -> Handlers = { on_a: (x) => label(x + k), on_b: (x) => shout(x - k) }

var current: Handlers = handlers(0)

pub fn install(h: Handlers) -> Unit = { current = h }

pub fn fire(x: Int) -> String = current.on_a(x) + " " + current.on_b(x)
"#;

const EXTERN: &str = "@extern(wasm, \"env\", \"host_beep\")\nfn host_beep(x: Int) -> Unit = _\n";

fn assert_same_on_both_legs(app: &Path) {
    let (ok, native, err) = run(app, false);
    assert!(ok, "native run failed:\n{err}");
    assert_eq!(native, "n4n3n2n1zero N2N1ZERO!\n");
    let (ok, wasm, err) = run(app, true);
    assert!(ok, "wasm run failed (was E082 call:label):\n{err}");
    assert_eq!(wasm, native, "wasm output must equal native");
}

#[test]
fn closure_callees_survive_an_extern_in_another_dependency_package() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(&root.join("hostpkg/almide.toml"), "[package]\nname = \"hostpkg\"\nversion = \"0.1.0\"\n");
    write(&root.join("hostpkg/src/mod.almd"), &format!("{EXTERN}\npub fn beep(x: Int) -> Unit = host_beep(x)\n"));
    write(
        &root.join("lib/almide.toml"),
        "[package]\nname = \"lib\"\nversion = \"0.1.0\"\n\n[dependencies]\nhostpkg = { path = \"../hostpkg\" }\n",
    );
    write(&root.join("lib/src/mod.almd"), &format!("import hostpkg\n\n{HANDLERS}"));
    let app = root.join("app");
    write(
        &app.join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nlib = { path = \"../lib\" }\n",
    );
    // `make` is a NAMED fn (the issue's `root`): the closures are built
    // inside the dependency, reached only through it.
    write(
        &app.join("src/main.almd"),
        "import lib\nfn make() -> lib.Handlers = lib.handlers(1)\neffect fn main() -> Unit = {\n  lib.install(make())\n  println(lib.fire(3))\n}\n",
    );
    assert_same_on_both_legs(&app);
}

#[test]
fn closure_callees_survive_an_extern_in_the_same_module() {
    let dir = tempfile::tempdir().expect("tempdir");
    let app = dir.path().join("app");
    write(&app.join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n");
    write(&app.join("src/lib.almd"), &format!("{EXTERN}\n{HANDLERS}"));
    write(
        &app.join("src/main.almd"),
        "import self.lib\nfn make() -> lib.Handlers = lib.handlers(1)\neffect fn main() -> Unit = {\n  lib.install(make())\n  println(lib.fire(3))\n}\n",
    );
    assert_same_on_both_legs(&app);
}
