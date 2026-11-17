//! E016 reaches a call through a generic wrapper (#4796, the program half).
//!
//! `fn uniq[A](xs: List[A]) -> List[A] = list.unique(xs)`: the equality
//! `list.unique` needs is checked where it is called, where `A` is still a
//! type variable, so `uniq` over a list of functions passed check and then
//! failed to build. The compare is now a requirement the wrapper carries to
//! its concrete calls, as the Secret rule (#4239) already carried it. The
//! stdlib's own wrappers (`deque.unique`) are checked in another pass, so
//! their requirement needs the stdlib-wide table #4796 tracks.

use std::io::Write;
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

fn check(tag: &str, src: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!("almd_4796_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("main.almd");
    std::fs::File::create(&file).unwrap().write_all(src.as_bytes()).unwrap();
    let out = Command::new(almide_bin()).args(["check", file.to_str().unwrap()]).output().expect("spawn almide");
    std::fs::remove_dir_all(&dir).ok();
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn a_wrapper_over_a_function_list_is_e016() {
    let (ok, text) = check(
        "fn",
        r#"fn uniq[A](xs: List[A]) -> List[A] = list.unique(xs)
fn h(x: Int) -> Int = x
effect fn main() -> Unit = {
  let d = uniq([h, h])
  println("${list.len(d)}")
}
"#,
    );
    assert!(!ok, "check accepted it:\n{text}");
    assert!(text.contains("E016"), "expected E016:\n{text}");
}

#[test]
fn a_wrapper_over_an_eq_type_still_checks() {
    let (ok, text) = check(
        "int",
        r#"fn uniq[A](xs: List[A]) -> List[A] = list.unique(xs)
effect fn main() -> Unit = {
  let d = uniq([1, 2, 2, 3])
  println("${list.len(d)}")
}
"#,
    );
    assert!(ok, "check refused it:\n{text}");
}

// The stdlib half (#4796, owner ruling 2026-11-17, option B): the
// self-hosted wrappers declare `[A: Eq]` and the built-in Eq is admitted by
// structure, as `[A: Ord]` is (#4758).

#[test]
fn deque_unique_over_a_function_holding_record_is_e016() {
    let (ok, text) = check(
        "dq_fn",
        r#"import http
effect fn main() -> Unit = {
  let rs = deque.unique(deque.from_list([http.route("GET /p", (req) => http.response(200, "x"))]))
  println("${deque.len(rs)}")
}
"#,
    );
    assert!(!ok, "check accepted it:\n{text}");
    assert!(text.contains("E016"), "expected E016:\n{text}");
}

#[test]
fn deque_unique_over_a_secret_is_refused_as_list_unique_is() {
    let (ok, text) = check(
        "dq_secret",
        r#"import secret
effect fn main() -> Unit = {
  let d = deque.unique(deque.from_list([secret.new(1), secret.new(2)]))
  println("${deque.len(d)}")
}
"#,
    );
    let (lok, ltext) = check(
        "ls_secret",
        r#"import secret
effect fn main() -> Unit = {
  let d = list.unique([secret.new(1), secret.new(2)])
  println("${list.len(d)}")
}
"#,
    );
    assert!(!ok, "check accepted deque.unique over a Secret:\n{text}");
    assert!(!lok, "list.unique over a Secret was accepted:\n{ltext}");
    assert!(text.contains("E103") && ltext.contains("E103"), "expected E103 on both:\n{text}\n{ltext}");
}

#[test]
fn the_eq_bound_wrappers_still_check_at_eq_types() {
    let (ok, text) = check(
        "dq_int",
        r#"effect fn main() -> Unit = {
  let d = deque.unique(deque.from_list([1, 2, 2, 3]))
  let e = deque.unique_by(deque.from_list(["a", "bb"]), (s) => string.len(s))
  let f = deque.dedup(deque.from_list([[1], [1], [2]]))
  let g = bytes.unique_by(bytes.from_list([1, 2, 3]), (b) => b % 2)
  let h = string.group_by("abc", (c) => c == "a")
  let i = set.group_by(set.from_list([1, 2, 3]), (x) => x % 2)
  println("${deque.len(d)} ${deque.len(e)} ${deque.len(f)} ${bytes.len(g)} ${map.len(h)} ${map.len(i)} ${deque.contains(d, 2)}")
}
"#,
    );
    assert!(ok, "check refused it:\n{text}");
}
