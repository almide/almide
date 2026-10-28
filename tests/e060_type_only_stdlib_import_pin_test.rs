//! #1853: a stdlib import used ONLY through the type it owns — `e: IpAddr`
//! in a signature, a `List[IpAddr]` annotation, the bare `V4(..)`
//! constructor, a `V4(..) =>` arm — is a USE of that import. The E060
//! pre-pass judged usage by qualified `net.*` heads alone and warned on
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
    almide::stdlib_info::stdlib_owned_types()
        .iter()
        .copied()
        .filter(|(module, _)| !almide::stdlib_info::AUTO_IMPORT_BUNDLED.contains(module))
        .collect()
}

/// How many type parameters `module`'s `ty` takes, read off its `type`
/// declaration in the bundled source (`type Secret[A] = …` → 1). A
/// runtime-backed row has no declaration and takes none.
fn owned_type_arity(module: &str, ty: &str) -> usize {
    let Some(source) = almide::stdlib_info::bundled_source(module) else { return 0 };
    let program = almide_lang::parse_cached(source).unwrap_or_else(|| panic!("bundled {module} parses"));
    program
        .decls
        .iter()
        .find_map(|decl| match decl {
            almide::ast::Decl::Type { name, generics, .. } if name.as_str() == ty => {
                Some(generics.as_ref().map_or(0, |g| g.len()))
            }
            _ => None,
        })
        .unwrap_or(0)
}

/// `ty` applied to `Int` once per declared type parameter (`Secret[Int]`),
/// bare when it takes none — the spelling a signature must write.
fn applied(module: &str, ty: &str) -> String {
    match owned_type_arity(module, ty) {
        0 => ty.to_string(),
        n => format!("{ty}[{}]", vec!["Int"; n].join(", ")),
    }
}

#[test]
fn signature_only_use_of_every_owned_type_is_a_use() {
    let cells = explicit_import_owned_types();
    assert!(cells.len() >= 11, "the owned-type table shrank: {cells:?}");
    for (module, ty) in cells {
        let t = applied(module, ty);
        assert_no_e060(
            &format!("{module}'s {t} in a signature"),
            &format!(
                "import {module}\n\nfn keep(x: {t}) -> {t} = x\n\nfn main() -> Unit = println(\"ok\")\n"
            ),
        );
    }
}

#[test]
fn a_generic_owned_type_is_spelled_with_its_arguments() {
    // The arity comes from the declaration, so the matrix above spells a
    // generic owned type applied, never bare (E093).
    assert_eq!(applied("secret", "Secret"), "Secret[Int]");
    assert_eq!(applied("http", "HttpRequest"), "HttpRequest");
}

#[test]
fn annotation_and_generic_argument_use_is_a_use() {
    assert_no_e060(
        "List[IpAddr] annotation",
        r#"import net

fn main() -> Unit = {
  let xs: List[IpAddr] = [V4(127, 0, 0, 1), V6([0, 0, 0, 0, 0, 0, 0, 1])]
  println(int.to_string(list.len(xs)))
}
"#,
    );
}

#[test]
fn bare_constructor_use_is_a_use() {
    assert_no_e060(
        "bare V4 value",
        r#"import net

fn main() -> Unit = println("${V4(127, 0, 0, 1)}")
"#,
    );
    assert_no_e060(
        "V4 in a match arm",
        r#"import net

fn main() -> Unit = println(match V4(127, 0, 0, 1) { V4(_, _, _, _) => "v4", V6(_) => "v6" })
"#,
    );
}

#[test]
fn owning_a_type_is_not_a_use_of_the_import() {
    assert_e060_on(
        "import net with neither a call nor a type",
        r#"import net

fn main() -> Unit = println("no net used")
"#,
        "net",
    );
}

#[test]
fn a_user_shadow_of_the_owned_type_is_not_a_use() {
    // #1837: the program's own `type IpAddr` is `self.IpAddr`; its bare
    // spellings (and its cases) are the user's, so `import net` stays unused.
    assert_e060_on(
        "user type IpAddr beside import net",
        r#"import net

type IpAddr = | Little | Big

fn f(e: IpAddr) -> IpAddr = e

fn main() -> Unit = println(match f(Little) { Little => "little", Big => "big" })
"#,
        "net",
    );
}

#[test]
fn the_stdlib_constructor_beside_a_user_shadow_is_still_a_use() {
    // The shadow reuses the TYPE name only; `V4` is still the `net`
    // module's case, judged by the case's owner, not the type name.
    assert_no_e060(
        "V4 beside a user type IpAddr",
        r#"import net

type IpAddr = | Little | Big

fn f(e: IpAddr) -> IpAddr = e

fn main() -> Unit = println("${V4(127, 0, 0, 1)} ${match f(Little) { Little => "little", Big => "big" }}")
"#,
    );
}
