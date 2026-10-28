//! #4107 row 21: `Secret[A]` is opaque outside the `secret` module whatever
//! the code calls it. An alias (any depth, any instantiation, in the entry
//! program or a dependency module) is the opaque type itself, so building,
//! updating, reading or destructuring one through the alias is E033 exactly
//! as through `Secret`. And the converse: a program or module that declares
//! its OWN type named `Secret` checks and runs as it would under a fresh name.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("secret_opaque_alias").join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `(success, stdout + stderr)` of `almide <verb> <file>` run from `dir`.
fn run(dir: &Path, verb: &str, file: &str) -> (bool, String) {
    let o = Command::new(almide()).current_dir(dir).arg(verb).arg(file).output().expect("almide runs");
    (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}

fn check_single(name: &str, src: &str) -> (bool, String) {
    let d = scratch(name);
    std::fs::write(d.join("main.almd"), src).unwrap();
    run(&d, "check", "main.almd")
}

fn assert_e033(name: &str, src: &str, doing: &str) {
    let (ok, out) = check_single(name, src);
    assert!(!ok, "{name}: an opaque Secret was reached through an alias and check passed:\n{out}");
    assert!(out.contains("E033"), "{name}: expected E033, got:\n{out}");
    let msg = format!("cannot {doing} opaque type 'Secret' outside its module 'secret'");
    assert!(out.contains(&msg), "{name}: expected `{msg}`, got:\n{out}");
}

#[test]
fn literal_through_alias_is_e033() {
    assert_e033("lit_direct", "import secret\nfn main() -> Unit = {\n  let s = Secret { hidden: 1 }\n  println(\"${s}\")\n}\n", "build a literal of");
    assert_e033("lit_alias", "import secret\ntype Mine = Secret[Int]\nfn main() -> Unit = {\n  let s = Mine { hidden: 1 }\n  println(\"${s}\")\n}\n", "build a literal of");
    assert_e033("lit_alias_string", "import secret\ntype Key = Secret[String]\nfn main() -> Unit = {\n  let s = Key { hidden: \"k\" }\n  println(\"${s}\")\n}\n", "build a literal of");
    assert_e033("lit_double_alias", "import secret\ntype Mine = Secret[Int]\ntype Mine2 = Mine\nfn main() -> Unit = {\n  let s = Mine2 { hidden: 1 }\n  println(\"${s}\")\n}\n", "build a literal of");
    assert_e033("lit_generic_alias", "import secret\ntype Wrap[T] = Secret[T]\ntype Mine = Wrap[Int]\nfn main() -> Unit = {\n  let s = Mine { hidden: 1 }\n  println(\"${s}\")\n}\n", "build a literal of");
    assert_e033("lit_alias_in_slot", "import secret\ntype Mine = Secret[Int]\nfn main() -> Unit = {\n  let s: Mine = { hidden: 1 }\n  println(\"${s}\")\n}\n", "build a literal of");
}

#[test]
fn field_read_through_alias_is_e033() {
    assert_e033("read_alias", "import secret\ntype Mine = Secret[Int]\nfn main() -> Unit = {\n  let s: Mine = secret.new(1)\n  println(int.to_string(s.hidden))\n}\n", "read field 'hidden' of");
    assert_e033("read_double_alias_param", "import secret\ntype Mine = Secret[Int]\ntype Mine2 = Mine\nfn peek(s: Mine2) -> Int = s.hidden\nfn main() -> Unit = println(int.to_string(peek(secret.new(1))))\n", "read field 'hidden' of");
}

#[test]
fn record_update_is_e033() {
    assert_e033("update_direct", "import secret\nfn main() -> Unit = {\n  let s = secret.new(1)\n  let t = { ...s, hidden: 2 }\n  println(\"${t}\")\n}\n", "build a record update of");
    assert_e033("update_alias", "import secret\ntype Mine = Secret[Int]\nfn main() -> Unit = {\n  let s: Mine = secret.new(1)\n  let t = { ...s, hidden: 2 }\n  println(\"${t}\")\n}\n", "build a record update of");
}

#[test]
fn destructuring_is_e033() {
    assert_e033("match_direct", "import secret\nfn main() -> Unit = {\n  let s = secret.new(1)\n  match s {\n    Secret { hidden } => println(int.to_string(hidden)),\n  }\n}\n", "destructure");
    assert_e033("match_alias", "import secret\ntype Mine = Secret[Int]\nfn main() -> Unit = {\n  let s: Mine = secret.new(1)\n  match s {\n    Mine { hidden } => println(int.to_string(hidden)),\n  }\n}\n", "destructure");
    assert_e033("let_destructure_alias", "import secret\ntype Mine = Secret[Int]\nfn main() -> Unit = {\n  let s: Mine = secret.new(1)\n  let { hidden } = s\n  println(int.to_string(hidden))\n}\n", "destructure");
}

/// A project whose dependency module `vault` declares the alias.
fn project_with_alias_module(name: &str, vault: &str, main: &str) -> (bool, String) {
    let d = scratch(name);
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("almide.toml"), "[package]\nname = \"aliasfx\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(d.join("src/vault.almd"), vault).unwrap();
    std::fs::write(d.join("src/main.almd"), main).unwrap();
    run(&d, "check", "src/main.almd")
}

#[test]
fn alias_in_a_dependency_module_is_e033() {
    // The module itself builds through its alias: foreign to `secret`.
    let (ok, out) = project_with_alias_module(
        "dep_builds",
        "import secret\ntype Mine = Secret[Int]\npub fn make() -> Mine = Mine { hidden: 7 }\n",
        "import self.vault\nfn main() -> Unit = println(\"${vault.make()}\")\n",
    );
    assert!(!ok && out.contains("E033"), "a dependency module built a Secret through its alias:\n{out}");
    // The entry builds through the module's alias.
    let (ok, out) = project_with_alias_module(
        "entry_builds",
        "import secret\ntype Mine = Secret[Int]\npub fn make() -> Mine = secret.new(7)\n",
        "import self.vault\nfn main() -> Unit = {\n  let s = vault.Mine { hidden: 1 }\n  println(\"${s}\")\n}\n",
    );
    assert!(!ok && out.contains("E033"), "the entry built a Secret through a module's alias:\n{out}");
    // The entry reads through the module's alias.
    let (ok, out) = project_with_alias_module(
        "entry_reads",
        "import secret\ntype Mine = Secret[Int]\npub fn make() -> Mine = secret.new(7)\n",
        "import self.vault\nfn main() -> Unit = println(int.to_string(vault.make().hidden))\n",
    );
    assert!(!ok && out.contains("E033"), "the entry read a Secret through a module's alias:\n{out}");
    // Control: the same project through the module's fns checks.
    let (ok, out) = project_with_alias_module(
        "control",
        "import secret\ntype Mine = Secret[Int]\npub fn make() -> Mine = secret.new(7)\npub fn get(s: Mine) -> Int = secret.reveal(s)\n",
        "import self.vault\nfn main() -> Unit = println(int.to_string(vault.get(vault.make())))\n",
    );
    assert!(ok, "the control project must check:\n{out}");
}

#[test]
fn the_alias_used_through_the_module_fns_checks() {
    let (ok, out) = check_single(
        "alias_ok",
        "import secret\ntype Mine = Secret[Int]\ntype Mine2 = Mine\nfn bump(s: Mine2) -> Mine = secret.new(secret.reveal(s) + 1)\nfn main() -> Unit = {\n  let s: Mine = secret.new(1)\n  println(\"${bump(s)}\")\n}\n",
    );
    assert!(ok, "an alias used only through `secret.new` / `secret.reveal` must check:\n{out}");
}

/// The program's own `type Secret` checks AND runs as under a fresh name.
#[test]
fn an_own_type_named_secret_is_its_own_type() {
    for (name, prelude) in [("own_no_import", ""), ("own_with_import", "import secret\n")] {
        let d = scratch(name);
        let src = format!(
            "{prelude}type Secret = {{ flag: Bool }}\nfn main() -> Unit = {{\n  let s = Secret {{ flag: true }}\n  let t = {{ ...s, flag: false }}\n  match t {{\n    Secret {{ flag }} => println(\"${{s.flag}} ${{flag}}\"),\n  }}\n}}\n"
        );
        std::fs::write(d.join("main.almd"), &src).unwrap();
        let (ok, out) = run(&d, "check", "main.almd");
        assert!(ok, "{name}: the program's own `type Secret` must check:\n{out}");
        let (ok, out) = run(&d, "run", "main.almd");
        assert!(ok && out.contains("true false"), "{name}: the program's own `type Secret` must run:\n{out}");
    }
    // A generic own type of the same name.
    let d = scratch("own_generic");
    std::fs::write(d.join("main.almd"), "type Secret[T] = { v: T, n: Int }\nfn main() -> Unit = {\n  let s = Secret { v: \"x\", n: 2 }\n  println(\"${s.v}${s.n}\")\n}\n").unwrap();
    let (ok, out) = run(&d, "run", "main.almd");
    assert!(ok && out.contains("x2"), "own generic `type Secret[T]` must run:\n{out}");
    // A user module declaring its own `Secret`, built and read from the entry.
    let (ok, out) = project_with_alias_module(
        "module_own_secret",
        "type Secret = { ok: Bool }\npub fn make() -> Secret = Secret { ok: true }\n",
        "import self.vault\nfn main() -> Unit = {\n  let s = vault.Secret { ok: false }\n  println(\"${s.ok} ${vault.make().ok}\")\n}\n",
    );
    assert!(ok, "a module's own `type Secret` must check:\n{out}");
}
