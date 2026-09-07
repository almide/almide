//! #1853: a stdlib import used ONLY through the type it owns — `e: Endian`
//! in a signature, a `List[Endian]` annotation, the bare `LittleEndian`
//! constructor, a `LittleEndian =>` arm — is a USE of that import. The E060
//! pre-pass judged usage by qualified `bytes.*` heads alone and warned on
//! every such program. The diagnostics harness pins a warning's PRESENCE,
//! never its absence, so the no-warning half lives here; the signature
//! matrix walks `STDLIB_OWNED_TYPES` so a type added to the table is pinned
//! the day it lands.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(source: &str) -> (bool, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("e060.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .arg("check")
        .arg(&file)
        .output()
        .expect("run almide check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

fn assert_no_e060(label: &str, source: &str) {
    let (ok, text) = check(source);
    assert!(ok, "{label}: must check clean, got:\n{text}");
    assert!(!text.contains("E060"), "{label}: a type-only use of an import is a use, got:\n{text}");
}

fn assert_e060_on(label: &str, source: &str, import: &str) {
    let (ok, text) = check(source);
    assert!(ok, "{label}: E060 is a warning, the program must still check, got:\n{text}");
    assert!(
        text.contains(&format!("unused import '{import}'")),
        "{label}: `import {import}` is unused here and must warn E060, got:\n{text}"
    );
}

/// Every stdlib-owned type whose owner needs an explicit `import` — the
/// auto-imported owners (`value`'s `Value`) have no import line to judge.
fn explicit_import_owned_types() -> Vec<(&'static str, &'static str)> {
    almide::stdlib_info::STDLIB_OWNED_TYPES
        .iter()
        .copied()
        .filter(|(module, _)| !almide::stdlib_info::AUTO_IMPORT_BUNDLED.contains(module))
        .collect()
}

#[test]
fn signature_only_use_of_every_owned_type_is_a_use() {
    let cells = explicit_import_owned_types();
    assert!(cells.len() >= 11, "the owned-type table shrank: {cells:?}");
    for (module, ty) in cells {
        assert_no_e060(
            &format!("{module}'s {ty} in a signature"),
            &format!(
                "import {module}\n\nfn keep(x: {ty}) -> {ty} = x\n\nfn main() -> Unit = println(\"ok\")\n"
            ),
        );
    }
}

#[test]
fn annotation_and_generic_argument_use_is_a_use() {
    assert_no_e060(
        "List[Endian] annotation",
        r#"import bytes

fn main() -> Unit = {
  let xs: List[Endian] = [LittleEndian, BigEndian]
  println(int.to_string(list.len(xs)))
}
"#,
    );
}

#[test]
fn bare_constructor_use_is_a_use() {
    assert_no_e060(
        "bare LittleEndian value",
        r#"import bytes

fn main() -> Unit = println("${LittleEndian}")
"#,
    );
    assert_no_e060(
        "LittleEndian in a match arm",
        r#"import bytes

fn main() -> Unit = println(match LittleEndian { LittleEndian => "le", BigEndian => "be" })
"#,
    );
}

#[test]
fn owning_a_type_is_not_a_use_of_the_import() {
    assert_e060_on(
        "import bytes with neither a call nor a type",
        r#"import bytes

fn main() -> Unit = println("no bytes used")
"#,
        "bytes",
    );
}

#[test]
fn a_user_shadow_of_the_owned_type_is_not_a_use() {
    // #1837: the program's own `type Endian` is `self.Endian`; its bare
    // spellings (and its cases) are the user's, so `import bytes` stays unused.
    assert_e060_on(
        "user type Endian beside import bytes",
        r#"import bytes

type Endian = | Little | Big

fn f(e: Endian) -> Endian = e

fn main() -> Unit = println(match f(Little) { Little => "little", Big => "big" })
"#,
        "bytes",
    );
}

#[test]
fn the_stdlib_constructor_beside_a_user_shadow_is_still_a_use() {
    // The shadow reuses the TYPE name only; `LittleEndian` is still the
    // `bytes` module's case, judged by the case's owner, not the type name.
    assert_no_e060(
        "LittleEndian beside a user type Endian",
        r#"import bytes

type Endian = | Little | Big

fn f(e: Endian) -> Endian = e

fn main() -> Unit = println("${LittleEndian} ${match f(Little) { Little => "little", Big => "big" }}")
"#,
    );
}
