//! A monomorphized instance's name cannot be spelled by user source (#3492).
//!
//! Mono named an instance `<fn>__<A>_<B>`, and both halves of that were
//! user-spellable:
//!
//! - a user type whose name holds `_` met the mangle of a structural type:
//!   `tag[List[Int]]` and `tag[List_Int]` were one fn `tag__List_Int`, so
//!   native printed `List_Int { n: 1 }` for `tag([1, 2])` and wasm walled; the
//!   same for `Pair[Int, String]` vs `Pair_Int_String`, `Box[List[Int]]` vs
//!   `Box_List_Int`, and a two-slot binding `(Int, String_Xx)` vs
//!   `(Int_String, Xx)`;
//! - a user fn named `wrap__Int` was the instance `wrap[Int]`: wasm called the
//!   instance for the user's call (`[2, 2]` for `[1002]`), native failed E0428.
//!
//! Instances are now named `__almd_mono<len>_<fn>__<types>` in the `__` space
//! #3483 keeps free of user entry fns, and `<types>` is a prefix-free code in
//! which a user `_` is `_u` and structure is `_` + another letter. Each
//! program runs on both legs and against a twin whose user names hold no `_`,
//! so a collision shows as a divergence.

use std::process::Command;

const COLLIDING: &str = r#"
type List_Int = { n: Int }

type Pair[A, B] = { a: A, b: B }

type Pair_Int_String = { k: Int }

type Box[T] = { v: T }

type Box_List_Int = { w: Int }

type List_Box_Int = { z: Int }

type Int_String = { q: Int }

type String_Xx = { r: Int }

type Xx = { s: Int }

fn tag[T](x: T) -> String = "${x}"

fn two[A, B](a: A, b: B) -> String = "${a}/${b}"

fn wrap[T](x: T) -> List[T] = [x, x]

local fn wrap__Int(x: Int) -> List[Int] = [x + 1000]

fn wrap__List_Int(x: List[Int]) -> List[List[Int]] = [x, x, x]

fn __almd_mono4_wrap__String(x: String) -> List[String] = [x + "!"]

effect fn main() -> Unit = {
  println(tag([1, 2]))
  println(tag(List_Int { n: 5 }))
  println(tag(Pair { a: 1, b: "x" }))
  println(tag(Pair_Int_String { k: 7 }))
  println(tag(Box { v: [1] }))
  println(tag(Box_List_Int { w: 2 }))
  println(tag([Box { v: 3 }]))
  println(tag(List_Box_Int { z: 4 }))
  println(two(1, String_Xx { r: 2 }))
  println(two(Int_String { q: 9 }, Xx { s: 3 }))
  println("${wrap(1)} ${wrap__Int(2)}")
  println("${wrap([1])} ${wrap__List_Int([2])}")
  println("${wrap("s")} ${__almd_mono4_wrap__String("t")}")
}
"#;

/// The user spellings above and their neutral twins. Applied longest first,
/// so no replacement rewrites part of another.
const RENAMES: &[(&str, &str)] = &[
    ("__almd_mono4_wrap__String", "userwrapstring"),
    ("Pair_Int_String", "PairIntString"),
    ("wrap__List_Int", "userwraplist"),
    ("Box_List_Int", "BoxListInt"),
    ("List_Box_Int", "ListBoxInt"),
    ("Int_String", "IntString"),
    ("String_Xx", "StringXx"),
    ("wrap__Int", "userwrapint"),
    ("List_Int", "ListInt"),
];

const EXPECTED: &str = "[1, 2]\nList_Int { n: 5 }\nPair { a: 1, b: \"x\" }\nPair_Int_String { k: 7 }\nBox { v: [1] }\nBox_List_Int { w: 2 }\n[Box { v: 3 }]\nList_Box_Int { z: 4 }\n1/String_Xx { r: 2 }\nInt_String { q: 9 }/Xx { s: 3 }\n[1, 1] [1002]\n[[1], [1]] [[2], [2], [2]]\n[\"s\", \"s\"] [\"t!\"]\n";

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn write(dir: &std::path::Path, program: &str) -> std::path::PathBuf {
    let source = dir.join("main.almd");
    std::fs::write(&source, program).expect("source");
    source
}

fn log(out: &std::process::Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

/// Build and run `program` on one leg; the stdout.
fn run(program: &str, target: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = write(dir.path(), program);
    let artifact = dir.path().join(if target == "rust" { "native" } else { "m.wasm" });
    let built = Command::new(almide_bin())
        .args(["build", source.to_str().expect("path"), "--target", target, "-o"])
        .arg(&artifact)
        .env_remove("ALMIDE_COMPONENT_P3")
        .output()
        .expect("build");
    assert!(built.status.success(), "{target} build:\n{}", log(&built));
    let mut command = if target == "rust" {
        Command::new(&artifact)
    } else {
        let mut c = Command::new("wasmtime");
        c.arg("run").arg(&artifact);
        c
    };
    let out = command.output().expect("run");
    assert!(out.status.success(), "{target} exited {:?}", out.status.code());
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// `text` with every user spelling replaced by its neutral twin.
fn neutral(text: &str) -> String {
    let mut renames = RENAMES.to_vec();
    renames.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    renames.iter().fold(text.to_string(), |src, (from, to)| src.replace(from, to))
}

#[test]
fn user_types_and_fns_spelling_instance_names_answer_like_their_neutral_twin_on_native() {
    assert_eq!(run(COLLIDING, "rust"), EXPECTED);
    assert_eq!(run(&neutral(COLLIDING), "rust"), neutral(EXPECTED));
}

#[test]
fn user_types_and_fns_spelling_instance_names_answer_like_their_neutral_twin_on_wasm() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(run(COLLIDING, "wasm"), EXPECTED);
    assert_eq!(run(&neutral(COLLIDING), "wasm"), neutral(EXPECTED));
}

/// The emitted Rust: one definition per instance, every instance in the
/// `__almd_mono` space, and each user fn under its own name.
#[test]
fn an_instance_has_its_own_definition_apart_from_every_user_fn() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = write(dir.path(), COLLIDING);
    let out = Command::new(almide_bin())
        .args([source.to_str().expect("path"), "--target", "rust"])
        .output()
        .expect("emit");
    assert!(out.status.success(), "{}", log(&out));
    let rust = String::from_utf8_lossy(&out.stdout);
    let defs: Vec<&str> = rust
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("pub fn "))
        .filter_map(|l| l.split('(').next())
        .collect();
    let instances: Vec<&&str> = defs.iter().filter(|d| d.starts_with("__almd_mono")).collect();
    // tag × 8 type bindings, two × 2, wrap × 3.
    assert_eq!(instances.len(), 13, "{defs:?}");
    let unique: std::collections::HashSet<&&&str> = instances.iter().collect();
    assert_eq!(unique.len(), instances.len(), "two instances share a name: {defs:?}");
    for user in ["wrap__Int", "wrap__List_Int", "almide_fn___almd_mono4_wrap__String"] {
        assert_eq!(defs.iter().filter(|d| **d == user).count(), 1, "user fn `{user}` is defined once: {defs:?}");
    }
}
