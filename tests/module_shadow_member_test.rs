//! #2345 / #2348: a local binding wins over a module of the same name in the
//! member path, as it already does for a bare identifier.
//!
//! These cells are the WHOLE of the evidence for the rule. No program in the
//! corpus binds a module's name and then calls through it — a scan of 1903
//! `.almd` files across stdlib, spec, tools, examples and research finds zero,
//! confirmed independently by two detectors each proven to fire on the shapes
//! below. So every gate stays green whether the rule is right or wrong, and a
//! green suite says nothing about it. The exposure is in USER code, where
//! `let list = [...]`, `let string = "..."` and `let map = {...}` are exactly
//! what gets written when a variable holds that type.
use std::process::Command;

fn bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

/// Returns (accepted, json). NOTE the accepted flag comes from a PLAIN check,
/// not from the `--json` run: `check --json` exits 0 even when it emitted
/// errors (#2350), so its status cannot be used as the verdict. Two runs is
/// the price of not asserting on a status that is always success.
fn check(program: &str) -> (bool, String) {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("shadow.almd");
    std::fs::write(&source, program).unwrap();
    let accepted = Command::new(bin())
        .arg("check")
        .arg(&source)
        .output()
        .unwrap()
        .status
        .success();
    let out = Command::new(bin())
        .args(["check", "--json"])
        .arg(&source)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (accepted, text)
}

/// The shape that reached codegen: both readings are well typed at one
/// argument, so nothing objected and rustc got invalid Rust. `almide 0.62.0`
/// answers "No errors found" for both of these.
#[test]
fn a_shadowed_module_call_is_rejected_at_check_time() {
    for program in [
        "fn main() -> Unit = {\n  let list = [3, 1, 2]\n  println(int.to_string(list.len(list)))\n}\n",
        "fn main() -> Unit = {\n  let string = \"hello\"\n  println(string.to_upper(string))\n}\n",
    ] {
        let (ok, text) = check(program);
        assert!(!ok, "must not check clean:\n{program}\n{text}");
        assert!(
            text.contains("E004"),
            "expected the arity error, got:\n{text}"
        );
    }
}

/// An explicitly imported module loses to a local of the same name, and the
/// import is then genuinely dead — E060 says so. E006 must NOT fire: the call
/// is the local's field, not the stdlib effect fn (#2345).
#[test]
fn an_explicit_import_loses_to_a_local_of_the_same_name() {
    let (_, text) = check(
        "import process\n\nfn main() -> Unit = {\n  let process = { exit: (c: Int) => c }\n  let _ = process.exit(200)\n  println(\"ok\")\n}\n",
    );
    assert!(
        !text.contains("E006"),
        "the module must not win over the local:\n{text}"
    );
    assert!(
        text.contains("E060"),
        "the now-dead import should be surfaced:\n{text}"
    );
}

/// An AUTO-imported module already loses to a local today — this is the cell
/// that makes the old behaviour self-inconsistent rather than a chosen rule,
/// and it must not change. Note there is no import statement here, so nothing
/// surfaces the shadow: for `list`/`string`/`map`/`set` the shadow is silent,
/// and that silent case is the common one.
#[test]
fn an_auto_imported_module_still_loses_to_a_local() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("auto.almd");
    std::fs::write(
        &source,
        "fn main() -> Unit = {\n  let string = { name: \"x\" }\n  println(string.name)\n}\n",
    )
    .unwrap();
    let out = Command::new(bin()).arg("run").arg(&source).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "x");
}

/// The escape cells: the local's type does not satisfy the module fn's first
/// parameter, so these never reached codegen even before the fix. They still
/// must be rejected, and they are now rejected as what they are — a method on
/// the local — rather than as a module call that no longer happens.
#[test]
fn a_shadowing_binding_of_the_wrong_type_is_still_rejected() {
    let (ok, text) = check(
        "fn main() -> Unit = {\n  let map = { a: 1 }\n  println(int.to_string(map.get(map, \"a\") ?? 0))\n}\n",
    );
    assert!(!ok, "must be rejected:\n{text}");
    assert!(text.contains("E002"), "expected undefined method:\n{text}");

    let (ok, text) = check(
        "fn main() -> Unit = {\n  let set = [1, 2]\n  println(int.to_string(set.len(set)))\n}\n",
    );
    assert!(!ok, "must be rejected:\n{text}");
    assert!(text.contains("E004"), "expected the arity error:\n{text}");
}

/// The control the whole change rests on: an UNSHADOWED module call is
/// untouched. If this ever fails, the guard is firing on module calls rather
/// than on shadowed ones.
#[test]
fn an_unshadowed_module_call_is_unaffected() {
    for program in [
        "import process\n\neffect fn main() -> Unit = process.exit(3)\n",
        "fn main() -> Unit = println(int.to_string(list.len([1, 2, 3])))\n",
        "fn main() -> Unit = println(string.to_upper(\"hello\"))\n",
    ] {
        let (ok, text) = check(program);
        assert!(ok, "an unshadowed module call must stay clean:\n{program}\n{text}");
    }
}
