//! A package's `native/*.rs` that calls back into the package by the item
//! names released compilers spelled keeps building (#3425).
//!
//! #3338 made `module_ident` injective and respelled every item a native
//! module calls back into: `almide_rt_tf_v0_entry` → `almide_rt_tf_0v0_entry`,
//! `almide_rt_tf_v0_calc_double` → `almide_rt_tf_0v0_1calc_double`. The old
//! spellings stay callable as deprecated aliases, emitted only into a crate
//! that carries a package `native/` tree, only where the old spelling is
//! unambiguous, with one warning per old name the native code uses.
//!
//! Each build gets its own `ALMIDE_RUN_PROJECT_DIR`, so it is a cold build
//! whose generated crate root (`src/main.rs`) can be read back.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

fn write(path: PathBuf, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
    std::fs::write(path, body).expect("write");
}

fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue3425-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("mkdir");
    root
}

/// The released spelling: the root fn twice (the warning is still once), a
/// sub-module fn once.
const OLD_HOST: &str = "\
pub fn call_both(x: i64) -> i64 {
    crate::almide_rt_tf_v0_entry(x) * 100 + crate::almide_rt_tf_v0_calc_double(x) + 0 * crate::almide_rt_tf_v0_entry(0)
}
";
const NEW_HOST: &str = "\
pub fn call_both(x: i64) -> i64 {
    crate::almide_rt_tf_0v0_entry(x) * 100 + crate::almide_rt_tf_0v0_1calc_double(x)
}
";

/// Package `tf`: a root module whose `via_native` goes through
/// `native/host.rs`, which calls the root's `entry` and `calc.double` back.
/// `via_native(3)` is `entry(3) * 100 + double(3)` = 706.
fn tf_package(dir: &Path, host: &str) {
    write(dir.join("almide.toml"), "[package]\nname = \"tf\"\nversion = \"0.1.0\"\n");
    write(
        dir.join("src/mod.almd"),
        "import self.calc\n\n@extern(rs, \"host\", \"call_both\")\nfn call_both(x: Int) -> Int\n\n\
         pub fn entry(x: Int) -> Int = calc.double(x) + 1\n\npub fn via_native(x: Int) -> Int = call_both(x)\n",
    );
    write(dir.join("src/calc.almd"), "pub fn double(x: Int) -> Int = x * 2\n");
    write(dir.join("src/main.almd"), "import self as tf\n\nfn main() -> Unit = println(int.to_string(tf.via_native(3)))\n");
    write(dir.join("native/host.rs"), host);
}

/// An app depending on `dep_dir`'s package `dep` by path, printing `expr`.
fn app_package(dir: &Path, dep: &str, dep_dir: &Path, expr: &str) {
    write(
        dir.join("almide.toml"),
        &format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n{dep} = {{ path = \"{}\" }}\n", dep_dir.display()),
    );
    write(dir.join("src/main.almd"), &format!("import {dep}\n\nfn main() -> Unit = println(int.to_string({expr}))\n"));
}

struct Run {
    ok: bool,
    stdout: String,
    stderr: String,
    /// The generated crate root, when the build wrote one.
    crate_root: Option<String>,
}

/// `almide run src/main.almd` in `dir`, in a fresh build directory.
fn run(dir: &Path, tag: &str) -> Run {
    let project = scratch(&format!("{tag}-build"));
    let out = Command::new(almide_bin())
        .current_dir(dir)
        .env("ALMIDE_RUN_PROJECT_DIR", &project)
        .args(["run", "src/main.almd"])
        .output()
        .expect("spawn almide run");
    let crate_root = std::fs::read_to_string(project.join("src/main.rs")).ok();
    let _ = std::fs::remove_dir_all(&project);
    Run {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        crate_root,
    }
}

fn warnings_for(stderr: &str, name: &str) -> usize {
    stderr.matches(&format!("calls `{name}`, the spelling before #3338")).count()
}

fn assert_old_names_run_and_warn(r: &Run, lane: &str) {
    assert!(r.ok && r.stdout.trim() == "706", "{lane}: the old spelling did not build and run:\n{}\n{}", r.stdout, r.stderr);
    for (old, new) in [
        ("almide_rt_tf_v0_entry", "almide_rt_tf_0v0_entry"),
        ("almide_rt_tf_v0_calc_double", "almide_rt_tf_0v0_1calc_double"),
    ] {
        assert_eq!(warnings_for(&r.stderr, old), 1, "{lane}: not one warning for {old}:\n{}", r.stderr);
        assert!(r.stderr.contains(&format!("write `crate::{new}`")), "{lane}: the warning does not name {new}:\n{}", r.stderr);
        assert!(r.stderr.contains("dialect epoch 13"), "{lane}: the warning does not name the removal epoch:\n{}", r.stderr);
    }
}

#[test]
fn old_names_build_as_the_entry_package_and_warn_once_each() {
    if !available() {
        return;
    }
    let tf = scratch("old-entry");
    tf_package(&tf, OLD_HOST);
    let r = run(&tf, "old-entry");
    assert_old_names_run_and_warn(&r, "entry package");
    let root = r.crate_root.expect("the build wrote no crate root");
    assert!(root.contains("use crate::almide_rt_tf_0v0_entry as almide_rt_tf_v0_entry;"), "no root alias in the crate");
    assert!(root.contains("use crate::almide_rt_tf_0v0_1calc_double as almide_rt_tf_v0_calc_double;"), "no sub-module alias in the crate");
    let _ = std::fs::remove_dir_all(&tf);
}

#[test]
fn old_names_build_as_a_dependency_and_warn_once_each() {
    if !available() {
        return;
    }
    let base = scratch("old-dep");
    tf_package(&base.join("tf"), OLD_HOST);
    app_package(&base.join("app"), "tf", &base.join("tf"), "tf.via_native(3)");
    let r = run(&base.join("app"), "old-dep");
    assert_old_names_run_and_warn(&r, "dependency");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn new_names_build_without_a_warning() {
    if !available() {
        return;
    }
    let base = scratch("new");
    tf_package(&base.join("tf"), NEW_HOST);
    app_package(&base.join("app"), "tf", &base.join("tf"), "tf.via_native(3)");
    for (dir, lane) in [(base.join("tf"), "entry package"), (base.join("app"), "dependency")] {
        let r = run(&dir, &format!("new-{}", lane.replace(' ', "-")));
        assert!(r.ok && r.stdout.trim() == "706", "{lane}: the new spelling did not build and run:\n{}\n{}", r.stdout, r.stderr);
        assert!(!r.stderr.contains("#3338"), "{lane}: a warning for a name the native code does not use:\n{}", r.stderr);
    }
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_package_without_native_code_gets_no_alias() {
    if !available() {
        return;
    }
    let base = scratch("plain");
    let lib = base.join("plain");
    write(lib.join("almide.toml"), "[package]\nname = \"plain\"\nversion = \"0.1.0\"\n");
    write(lib.join("src/mod.almd"), "import self.calc\n\npub fn entry(x: Int) -> Int = calc.double(x) + 1\n");
    write(lib.join("src/calc.almd"), "pub fn double(x: Int) -> Int = x * 2\n");
    app_package(&base.join("app"), "plain", &lib, "plain.entry(3)");
    let r = run(&base.join("app"), "plain");
    assert!(r.ok && r.stdout.trim() == "7", "the plain package did not run:\n{}\n{}", r.stdout, r.stderr);
    if let Some(root) = &r.crate_root {
        assert!(!root.contains("#3425") && !root.contains(" as almide_rt_"), "a crate without native code got aliases");
    }
    // The emitted Rust is the generated crate before injection: no alias,
    // and the same text a second time.
    let emit = |dir: &Path| {
        let out = Command::new(almide_bin()).current_dir(dir).args(["src/main.almd", "--target", "rust"]).output().expect("spawn");
        assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    let first = emit(&base.join("app"));
    assert!(first.contains("almide_rt_plain_0v0_entry") && !first.contains(" as almide_rt_"), "unexpected emission");
    assert_eq!(first, emit(&base.join("app")));
    let _ = std::fs::remove_dir_all(&base);
}

/// #3338's own collision: modules `a.b` and `a_b` both define `c`, so the old
/// spelling `almide_rt_cz_v0_a_b_c` named two items. Neither gets it as an
/// alias, and native code calling both by their new names builds.
#[test]
fn a_colliding_old_spelling_gets_no_alias() {
    if !available() {
        return;
    }
    let cz = scratch("collide");
    write(cz.join("almide.toml"), "[package]\nname = \"cz\"\nversion = \"0.1.0\"\n");
    write(
        cz.join("src/mod.almd"),
        "import self.a.b\nimport self.a_b\n\n@extern(rs, \"host\", \"call_both\")\nfn call_both() -> Int\n\n\
         pub fn direct() -> Int = b.c() * 10 + a_b.c()\n\npub fn via_native() -> Int = call_both()\n",
    );
    write(cz.join("src/a/b.almd"), "pub fn c() -> Int = 1\n");
    write(cz.join("src/a_b.almd"), "pub fn c() -> Int = 2\n");
    write(cz.join("src/main.almd"), "import self as cz\n\nfn main() -> Unit = println(int.to_string(cz.via_native() + cz.direct()))\n");
    write(
        cz.join("native/host.rs"),
        "pub fn call_both() -> i64 { crate::almide_rt_cz_0v0_1a_1b_c() * 100 + crate::almide_rt_cz_0v0_1a_0b_c() * 1000 }\n",
    );
    let r = run(&cz, "collide");
    assert!(r.ok && r.stdout.trim() == "2112", "the collision package did not build and run:\n{}\n{}", r.stdout, r.stderr);
    assert!(!r.stderr.contains("#3338"), "a warning for names the native code does not use:\n{}", r.stderr);
    let root = r.crate_root.expect("the build wrote no crate root");
    assert!(!root.contains("as almide_rt_cz_v0_a_b_c;"), "the ambiguous old spelling got an alias");
    let _ = std::fs::remove_dir_all(&cz);
}
