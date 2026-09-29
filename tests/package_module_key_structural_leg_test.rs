//! #2865 gate: a package module keyed like a compiler namespace (`src/prim.almd`,
//! `src/list.almd`, …) is the package's own module on the structural wasm leg,
//! in-package exactly as it is as a dependency. The emitter recognizes its
//! intrinsic surfaces by module key — `prim.*` is the raw-memory floor, `list.*`
//! / `bytes.*` / `fs.*` … have hand-lowered arms — and the in-package module
//! used to reach them: `prim.build` walled as an unknown prim op, a package
//! `list.len` lowered as the stdlib's, and the E081 availability table barred
//! a package `process.exec` as the stdlib's.
//!
//! The cells are enumerated from the registries — every stdlib and bundled
//! module name and every compiler-known pseudo-module — never from a hand
//! list. Each cell's module declares `build` and one function named after a
//! function the stdlib module of that key declares (the collision the arms
//! match on), and runs in both shapes under FORCED structural routing (a wall
//! is a hard error, not a silent incumbent fallback). A cell passes when it
//! prints what the program means (`ok`, `42`); a cell the checker itself
//! rejects is outside this gate (the frontend's family) and is counted apart.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const EXPECTED: &str = "ok\n42\n";
const PER_RUN_LIMIT: Duration = Duration::from_secs(120);

/// `ALMIDE_BIN` runs the cells against another build; the default is this
/// workspace's binary.
fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, content).expect("write");
}

/// Every module key a program can name that the compiler also resolves on its
/// own, with a function name its stdlib source declares (`len` when the
/// module has no source).
fn cells() -> Vec<(String, String)> {
    use almide_lang::stdlib_info::{BUNDLED_MODULES, STDLIB_MODULES};
    let mut names: Vec<&str> = STDLIB_MODULES.iter().chain(BUNDLED_MODULES).copied().collect();
    names.extend(almide_lang::time_units::TIME_MODULES.iter().map(|(t, _)| *t));
    names.push("fan");
    names.sort_unstable();
    names.dedup();
    names
        .into_iter()
        .map(|m| {
            let collide = almide_lang::stdlib_info::bundled_source(m)
                .and_then(almide_lang::parse_cached)
                .and_then(|p| {
                    p.decls.iter().find_map(|d| match d {
                        almide_lang::ast::Decl::Fn { name, .. }
                            if !name.as_str().starts_with('_') && name.as_str() != "build" =>
                        {
                            Some(name.as_str().to_string())
                        }
                        _ => None,
                    })
                })
                .unwrap_or_else(|| "len".to_string());
            (m.to_string(), collide)
        })
        .collect()
}

fn module_source(collide: &str) -> String {
    format!("fn build(b: Bool) -> Bool = b\n\nfn {collide}(n: Int) -> Int = n + 1\n")
}

/// Lay the cell out in `shape`; returns the directory to run in.
fn lay_out(root: &Path, shape: &str, module: &str, collide: &str) -> std::path::PathBuf {
    let pkg_toml = "[package]\nname = \"cellpkg\"\nversion = \"0.1.0\"\n";
    let calls = format!("(if {module}.build(true) then \"ok\" else \"no\")");
    let number = format!("int.to_string({module}.{collide}(41))");
    if shape == "in-package" {
        write(&root.join("almide.toml"), pkg_toml);
        write(&root.join(format!("src/{module}.almd")), &module_source(collide));
        write(
            &root.join("src/main.almd"),
            &format!("import self.{module}\n\neffect fn main() -> Unit = {{\n  println({calls})\n  println({number})\n}}\n"),
        );
        return root.to_path_buf();
    }
    let dep = root.join("cellpkg");
    let app = root.join("app");
    write(&dep.join("almide.toml"), pkg_toml);
    write(&dep.join(format!("src/{module}.almd")), &module_source(collide));
    write(
        &dep.join("src/mod.almd"),
        &format!("import self.{module}\n\nfn run() -> String = {calls} + \"\\n\" + {number}\n"),
    );
    write(&app.join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ncellpkg = { path = \"../cellpkg\" }\n");
    write(&app.join("src/main.almd"), "import cellpkg\n\neffect fn main() -> Unit = {\n  println(cellpkg.run())\n}\n");
    app
}

/// Run with a per-run limit: a hang is a finding, not a stalled suite.
fn run(dir: &Path, args: &[&str], structural: bool) -> (bool, String, String) {
    let mut cmd = Command::new(almide());
    cmd.current_dir(dir).args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    if structural {
        cmd.env("ALMIDE_WASM_SKIP_STOCK_AUDIT", "1");
    }
    let mut child = cmd.spawn().expect("spawn almide");
    let (mut so, mut se) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let to = std::thread::spawn(move || {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut so, &mut s).ok();
        s
    });
    let te = std::thread::spawn(move || {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut se, &mut s).ok();
        s
    });
    let start = Instant::now();
    let ok = loop {
        if let Some(st) = child.try_wait().expect("wait") {
            break st.success();
        }
        if start.elapsed() > PER_RUN_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            return (false, String::new(), "HANG".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    (ok, to.join().unwrap(), te.join().unwrap())
}

#[test]
fn a_package_module_keyed_like_a_compiler_namespace_runs_on_the_structural_leg() {
    let mut bad = Vec::new();
    let mut passed = 0;
    let mut rejected = Vec::new();
    for (module, collide) in cells() {
        for shape in ["in-package", "dependency"] {
            let root = tempfile::tempdir().expect("tempdir");
            let dir = lay_out(root.path(), shape, &module, &collide);
            let (ok, out, err) = run(&dir, &["run", "src/main.almd", "--target", "wasm"], true);
            if ok && out == EXPECTED {
                passed += 1;
                continue;
            }
            // Outside this gate only when the checker itself refuses the program.
            let (checks, ..) = run(&dir, &["check", "src/main.almd"], false);
            if !checks {
                rejected.push(format!("{shape} `{module}`"));
                continue;
            }
            bad.push(format!("{shape} module `{module}` (fns build, {collide}):\n--- stdout\n{out}--- stderr\n{err}"));
        }
    }
    assert!(bad.is_empty(), "{} cell(s) diverged on the structural leg:\n{}", bad.len(), bad.join("\n"));
    // A floor, so a registry change or a checker regression cannot make the
    // gate pass by rejecting everything.
    assert!(passed >= 80, "only {passed} cells ran (checker-rejected: {rejected:?})");
}
