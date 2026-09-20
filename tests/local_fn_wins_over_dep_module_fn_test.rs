//! A file's own top-level `fn` wins over a same-named `fn` in a DEPENDENCY
//! package's module (#2375).
//!
//! `infer_module` and `refresh_module_top_lets` register a module's decls
//! UNPREFIXED so the module's own bodies resolve bare names, then undo it.
//! The undo was a key-set restore: it could drop a key the registration
//! ADDED, and could not put back a binding it OVERWROTE. So a dependency
//! module's `parser.field(name, x)` landed on the bare key `field` that the
//! file under check owns, and stayed there — wearing two parameters — for the
//! rest of the run. `almide check` then rejected `field("fstring")` with E004
//! while `almide build` and `almide test` compiled and ran the same call,
//! because only `check` reaches every file as its own entry.
//!
//! The dependency module carries a top-level `let` on purpose: that is what
//! arms `refresh_module_top_lets`, the pre-pass that runs BEFORE the entry
//! program is inferred, so the corruption is already in place by the time the
//! entry's own call is resolved.

use std::path::Path;
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

fn tools_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

/// An app package that depends on `dep` by path, with the app's own `src/`.
fn scratch(name: &str, dep_modules: &[(&str, &str)], app_modules: &[(&str, &str)]) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("almide-issue2375-{}", name));
    let _ = std::fs::remove_dir_all(&root);
    let dep = root.join("dep");
    let app = root.join("app");
    std::fs::create_dir_all(dep.join("src")).expect("mkdir dep");
    std::fs::create_dir_all(app.join("src")).expect("mkdir app");
    std::fs::write(
        dep.join("almide.toml"),
        "[package]\nname = \"deplib\"\nversion = \"0.1.0\"\n",
    )
    .expect("write dep toml");
    for (file, body) in dep_modules {
        std::fs::write(dep.join("src").join(file), body).expect("write dep module");
    }
    std::fs::write(
        app.join("almide.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ndeplib = { path = \"../dep\" }\n",
    )
    .expect("write app toml");
    for (file, body) in app_modules {
        std::fs::write(app.join("src").join(file), body).expect("write app module");
    }
    app
}

/// The dependency's `parser` module: a two-parameter `field`, plus a
/// top-level `let` so the module reaches the top-let refresh pre-pass.
const DEP_PARSER: &str = concat!(
    "let PREFIX = \"r:\"\n",
    "fn field(name: String, x: String) -> String = PREFIX + name + x\n",
);

const DEP_MOD: &str = "import self.parser\nfn run() -> String = parser.field(\"a\", \"b\")\n";

fn check_in(dir: &Path, args: &[&str]) -> (bool, String) {
    let mut cmd = Command::new(almide_bin());
    cmd.arg("check");
    cmd.args(args);
    let output = cmd.current_dir(dir).output().expect("failed to spawn almide");
    let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), combined)
}

/// The shape from the report: the call is in a NON-ENTRY file of the package,
/// so `almide check` with no file argument reaches it as its own entry.
#[test]
fn a_dep_modules_fn_does_not_capture_a_files_own_bare_call() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "bare-call",
        &[("parser.almd", DEP_PARSER), ("mod.almd", DEP_MOD)],
        &[
            (
                "rules.almd",
                concat!(
                    "import deplib.parser\n",
                    "fn field(family: String) -> String = family\n",
                    "fn rules() -> List[String] = [field(\"fstring\")]\n",
                ),
            ),
            ("mod.almd", "import self.rules\nfn all() -> List[String] = rules.rules()\n"),
        ],
    );
    let (ok, out) = check_in(&dir, &[]);
    assert!(
        ok,
        "the file's own one-parameter `field` lost to the dependency's two-parameter one:\n{out}"
    );
    assert!(
        !out.contains("E004"),
        "arity error against a local fn that takes exactly the arguments given:\n{out}"
    );
}

/// Same package, pointed at the one file — the form that fails identically on
/// 0.62.0, which is why the report reads as a 0.63 regression only through
/// `check`'s widened entry discovery (#2165).
#[test]
fn the_same_holds_when_the_file_is_named_directly() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "named-file",
        &[("parser.almd", DEP_PARSER), ("mod.almd", DEP_MOD)],
        &[
            (
                "rules.almd",
                concat!(
                    "import deplib.parser\n",
                    "fn field(family: String) -> String = family\n",
                    "fn rules() -> List[String] = [field(\"fstring\")]\n",
                ),
            ),
            ("mod.almd", "import self.rules\nfn all() -> List[String] = rules.rules()\n"),
        ],
    );
    let (ok, out) = check_in(&dir, &["src/rules.almd"]);
    assert!(ok, "checking the file directly still resolved to the dependency's fn:\n{out}");
}

/// The other half of the same shared namespace, found by putting the restore
/// in and watching a real program break: with the entry's bare binding no
/// longer destroyed, it was ANSWERING a dependency module's selectively
/// imported call. The entry declares `opt(args, name)`; the dependency module
/// writes `import deplib.parser.{ opt }` and calls `opt(rule)`. A file's
/// selective import must beat a bare binding that belongs to another file.
#[test]
fn a_selective_import_beats_the_entrys_same_named_fn() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "selective-vs-entry",
        &[
            (
                "parser.almd",
                concat!(
                    "let PREFIX = \"r:\"\n",
                    "fn opt(rule: String) -> String = PREFIX + rule\n",
                ),
            ),
            (
                "mod.almd",
                concat!(
                    "import deplib.parser.{ opt }\n",
                    "fn run() -> String = opt(\"a\")\n",
                ),
            ),
        ],
        &[(
            "mod.almd",
            concat!(
                "import deplib\n",
                "fn opt(args: List[String], name: String) -> String = name + list.join(args, \"\")\n",
                "fn all() -> String = deplib.run() + opt([\"x\"], \"y\")\n",
            ),
        )],
    );
    let (ok, out) = check_in(&dir, &[]);
    assert!(
        ok,
        "the entry's two-parameter `opt` answered the dependency's `import ...{{ opt }}` call:\n{out}"
    );
}

/// The dependency's own call must keep resolving to its own `field` — the
/// restore must put the entry's binding back WITHOUT taking the module's
/// intra-module resolution away from it.
#[test]
fn the_dependency_still_resolves_its_own_bare_call() {
    if !tools_available() {
        eprintln!("skip: almide binary unavailable");
        return;
    }
    let dir = scratch(
        "dep-intramodule",
        &[
            (
                "parser.almd",
                concat!(
                    "let PREFIX = \"r:\"\n",
                    "fn field(name: String, x: String) -> String = PREFIX + name + x\n",
                    "fn pair() -> String = field(\"a\", \"b\")\n",
                ),
            ),
            ("mod.almd", "import self.parser\nfn run() -> String = parser.pair()\n"),
        ],
        &[
            (
                "rules.almd",
                concat!(
                    "import deplib.parser\n",
                    "fn field(family: String) -> String = family\n",
                    "fn rules() -> List[String] = [field(\"fstring\")]\n",
                ),
            ),
            ("mod.almd", "import self.rules\nfn all() -> List[String] = rules.rules()\n"),
        ],
    );
    let (ok, out) = check_in(&dir, &[]);
    assert!(ok, "the dependency's own bare call to its own fn stopped resolving:\n{out}");
}
