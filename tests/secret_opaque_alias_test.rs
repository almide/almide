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

fn assert_refused(name: &str, src: &str, code: &str, needle: &str) {
    let (ok, out) = check_single(name, src);
    assert!(!ok, "{name}: check passed, but the Secret crossed its boundary:\n{out}");
    assert!(out.contains(code), "{name}: expected {code}, got:\n{out}");
    assert!(out.contains(needle), "{name}: expected `{needle}`, got:\n{out}");
}

fn assert_checks(name: &str, src: &str) {
    let (ok, out) = check_single(name, src);
    assert!(ok, "{name}: must check:\n{out}");
}

const READS: &str = "cannot use opaque type 'Secret' as another record type outside its module 'secret'";
const FORGES: &str = "cannot use another record type as opaque type 'Secret' outside its module 'secret'";

/// A structural record type is no way into an opaque one (row 21): a Secret
/// never fills `{ hidden: .. }`, an open row, a structural bound, a user
/// record of the same shape — at the top of a slot or inside it — and no
/// such record fills a Secret slot.
#[test]
fn a_structural_record_type_is_no_way_into_a_secret() {
    assert_refused("param_closed", "import secret\nfn peek(r: { hidden: String }) -> String = r.hidden\nfn main() -> Unit = println(peek(secret.new(\"pw\")))\n", "E033", READS);
    assert_refused("param_open_row", "import secret\nfn peek(r: { hidden: String, .. }) -> String = r.hidden\nfn main() -> Unit = println(peek(secret.new(\"pw\")))\n", "E033", READS);
    assert_refused("param_bound", "import secret\nfn peek[T: { hidden: String, .. }](r: T) -> String = r.hidden\nfn main() -> Unit = println(peek(secret.new(\"pw\")))\n", "E033", READS);
    assert_refused("param_user_record", "import secret\ntype Fake = { hidden: String }\nfn peek(r: Fake) -> String = r.hidden\nfn main() -> Unit = println(peek(secret.new(\"pw\")))\n", "E033", READS);
    assert_refused("param_in_list", "import secret\nfn peek(rs: List[{ hidden: String }]) -> String = rs |> list.map((r) => r.hidden) |> list.join(\",\")\nfn main() -> Unit = println(peek([secret.new(\"pw\")]))\n", "E033", READS);
    assert_refused("let_annotation", "import secret\nfn main() -> Unit = {\n  let r: { hidden: String } = secret.new(\"pw\")\n  println(r.hidden)\n}\n", "E033", READS);
    assert_refused("forge_user_record", "import secret\ntype Fake = { hidden: String }\nfn main() -> Unit = {\n  let f = Fake { hidden: \"x\" }\n  println(secret.reveal(f))\n}\n", "E033", FORGES);
    // Control: the same shapes on a plain record check.
    assert_checks("param_plain", "type Plain = { hidden: String }\nfn peek(r: { hidden: String }) -> String = r.hidden\nfn main() -> Unit = println(peek(Plain { hidden: \"x\" }))\n");
}

/// Every spread copies the base's fields: E033 with or without an override,
/// and when the base's type is only known after the solve.
#[test]
fn every_spread_of_a_secret_is_e033() {
    let msg = "cannot build a record update of opaque type 'Secret' outside its module 'secret'";
    assert_refused("spread_bare", "import secret\nfn main() -> Unit = {\n  let s = secret.new(\"pw\")\n  let t = { ...s }\n  println(\"${t}\")\n}\n", "E033", msg);
    assert_refused("spread_override", "import secret\nfn main() -> Unit = {\n  let s = secret.new(\"pw\")\n  let t = { ...s, hidden: \"x\" }\n  println(\"${t}\")\n}\n", "E033", msg);
    assert_refused("spread_in_lambda", "import secret\nfn main() -> Unit = {\n  let xs = [secret.new(\"pw\")] |> list.map((x) => { ...x })\n  println(\"${xs}\")\n}\n", "E033", msg);
}

/// #4239: a generic `==` is structural, so no instantiation may hold a
/// Secret — whatever it holds, directly or through a generic caller, and
/// through a stdlib fn's equality bound.
#[test]
fn a_secret_never_reaches_a_generic_compare() {
    let msg = "a generic `==` is structural";
    assert_refused("eq_int", "import secret\nfn same[A](a: A, b: A) -> Bool = a == b\nfn main() -> Unit = println(\"${same(secret.new(1), secret.new(2))}\")\n", "E103", msg);
    assert_refused("eq_string", "import secret\nfn same[A](a: A, b: A) -> Bool = a == b\nfn main() -> Unit = println(\"${same(secret.new(\"a\"), secret.new(\"b\"))}\")\n", "E103", msg);
    assert_refused("eq_nested", "import secret\nfn same[A](a: A, b: A) -> Bool = a == b\nfn main() -> Unit = println(\"${same([secret.new(\"a\")], [secret.new(\"b\")])}\")\n", "E103", msg);
    assert_refused("eq_through_caller", "import secret\nfn same[A](a: A, b: A) -> Bool = a == b\nfn also[B](a: B, b: B) -> Bool = same(a, b)\nfn main() -> Unit = println(\"${also(secret.new(\"a\"), secret.new(\"b\"))}\")\n", "E103", msg);
    assert_refused("eq_stdlib", "import secret\nfn main() -> Unit = {\n  let k = secret.new(\"a\")\n  println(\"${list.contains([k], k)}\")\n}\n", "E103", "its compare is generic and structural");
    // Controls: the generic over plain values, and the concrete constant-time `==`.
    assert_checks("eq_plain", "fn same[A](a: A, b: A) -> Bool = a == b\nfn main() -> Unit = println(\"${same(1, 2)} ${same(\"a\", \"a\")}\")\n");
    assert_checks("eq_concrete", "import secret\nfn main() -> Unit = println(\"${secret.new(\"a\") == secret.new(\"b\")}\")\n");
}

/// A dependency package `vaultlib` declaring `mod type Vault`, and an app
/// whose entry is `main`. `(check ok, output)` of `almide check` in the app.
fn dep_project(name: &str, main: &str, app_mod: Option<&str>) -> (bool, String) {
    let root = scratch(name);
    let lib = root.join("vaultlib");
    std::fs::create_dir_all(lib.join("src")).unwrap();
    std::fs::write(lib.join("almide.toml"), "[package]\nname = \"vaultlib\"\nversion = \"0.1.0\"\n").unwrap();
    std::fs::write(
        lib.join("src/mod.almd"),
        "mod type Vault = { key: String }\n\npub fn make(s: String) -> Vault = { key: s }\n\npub fn key_len(v: Vault) -> Int = string.len(v.key)\n",
    )
    .unwrap();
    let app = root.join("app");
    std::fs::create_dir_all(app.join("src")).unwrap();
    std::fs::write(app.join("almide.toml"), "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\nvaultlib = { path = \"../vaultlib\" }\n").unwrap();
    std::fs::write(app.join("src/main.almd"), main).unwrap();
    if let Some(m) = app_mod {
        std::fs::write(app.join("src/notes.almd"), m).unwrap();
    }
    run(&app, "check", "src/main.almd")
}

/// #4241: a USER `mod type` record is opaque outside its project by the
/// same rule as the bundled Secret — read off the declaration, not a name.
#[test]
fn a_user_mod_type_is_opaque_outside_its_project() {
    let cases = [
        ("user_update_forge", "import vaultlib\neffect fn main() -> Unit = {\n  let v = { ...vaultlib.make(\"a\"), key: \"forged\" }\n  println(\"${vaultlib.key_len(v)}\")\n}\n", "cannot build a record update of opaque type 'Vault' outside its module 'vaultlib'"),
        ("user_spread_copy", "import vaultlib\neffect fn main() -> Unit = {\n  let v = { ...vaultlib.make(\"a\") }\n  println(\"${vaultlib.key_len(v)}\")\n}\n", "cannot build a record update of opaque type 'Vault' outside its module 'vaultlib'"),
        ("user_field_read", "import vaultlib\neffect fn main() -> Unit = println(vaultlib.make(\"a\").key)\n", "cannot read field 'key' of opaque type 'Vault' outside its module 'vaultlib'"),
        ("user_structural_param", "import vaultlib\nfn peek(r: { key: String }) -> String = r.key\neffect fn main() -> Unit = println(peek(vaultlib.make(\"a\")))\n", "cannot use opaque type 'Vault' as another record type outside its module 'vaultlib'"),
        ("user_structural_forge", "import vaultlib\neffect fn main() -> Unit = {\n  let r = { key: \"forged\" }\n  println(\"${vaultlib.key_len(r)}\")\n}\n", "outside its module 'vaultlib'"),
    ];
    for (name, main, needle) in cases {
        let (ok, out) = dep_project(name, main, None);
        assert!(!ok && out.contains("E033") && out.contains(needle), "{name}: expected E033 `{needle}`, got (ok={ok}):\n{out}");
    }
    // Control: through the module's API it checks.
    let (ok, out) = dep_project("user_api", "import vaultlib\neffect fn main() -> Unit = println(\"${vaultlib.key_len(vaultlib.make(\"a\"))}\")\n", None);
    assert!(ok, "the API route must check:\n{out}");
}

/// Inside its own project a `mod type` stays open: a sibling module of the
/// declaring one reads, destructures and updates it.
#[test]
fn a_user_mod_type_is_open_inside_its_project() {
    let notes = "mod type Note = { body: String }\n\nmod fn note(s: String) -> Note = { body: s }\n";
    let main = "import self.notes\neffect fn main() -> Unit = {\n  let n = notes.note(\"a\")\n  let m = { ...n, body: \"b\" }\n  let { body } = m\n  println(n.body)\n  println(body)\n}\n";
    let (ok, out) = dep_project("same_project", main, Some(notes));
    assert!(ok, "a `mod type` is open to its own project:\n{out}");
}
