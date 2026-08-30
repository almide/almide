//! `for x in xs` clone elision (#1673).
//!
//! The idiomatic loop lowered to `xs.clone().iter().cloned()` and cloned the
//! loop variable again at every consuming use: a whole-list copy on loop
//! entry, one element copy per iteration, and a third copy at the call. The
//! first is a borrow (`.iter()` never consumes `xs`), the third is a move
//! (the loop rebinds `x` every iteration). These tests pin the emitted
//! shapes: the two redundant clones are gone, and the cases where a copy IS
//! load-bearing — the body writes the list, an outer loop variable is used
//! inside an inner loop, a closure captures it — keep theirs.
//!
//! Emit-shape tests, in the mold of `tco_accumulator_move_test.rs`: they
//! assert on `--target rust` output, so a regression shows up as the exact
//! `.clone()` that came back. Skips cleanly when the `almide` binary is
//! unavailable (CI builds it first; locally `cargo build --release`).

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

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

/// Emit Rust for `source` and return the body of `pub fn main`.
/// (Shared by the clone-elision and the borrowed-binder tests below.)
fn emitted_main(source: &str, tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("almide-forin-clone-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    let output = Command::new(almide_bin())
        .args([src.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("failed to spawn almide");
    let rust = String::from_utf8_lossy(&output.stdout).to_string();
    std::fs::remove_dir_all(&dir).ok();
    assert!(output.status.success(), "--target rust emit failed:\n{}", String::from_utf8_lossy(&output.stderr));
    let start = rust.find("pub fn main(").unwrap_or_else(|| panic!("emitted Rust has no `pub fn main(`:\n{rust}"));
    let rest = &rust[start..];
    let end = rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len());
    rest[..end].to_string()
}

const PRELUDE: &str = "type U = { a: Int }\n\
    fn hit(v: Value) -> Int = match value.as_int(v) { ok(n) => n, err(_) => 0 }\n\
    fn hitu(u: U) -> Int = u.a\n";

#[test]
fn list_loop_borrows_the_list_and_moves_the_element() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let body = emitted_main(&format!("{PRELUDE}\
        fn main() -> Unit = {{\n\
          let data: List[Value] = [value.int(1), value.int(2)]\n\
          var t = 0\n\
          for v in data {{ t = t + hit(v) }}\n\
          for v in data {{ t = t + hit(v) }}\n\
          println(\"${{t}}\")\n\
        }}\n"), "borrow-move");
    assert!(body.contains("for v in data.iter().cloned()"), "the list is borrowed by `.iter()`, a clone under it is a throwaway copy:\n{body}");
    assert!(!body.contains("data.clone()"), "whole-list clone is back on the loop head:\n{body}");
    assert!(body.contains("hit(v)"), "the loop variable is rebound every iteration; its last use is a move:\n{body}");
    assert!(!body.contains("v.clone()"), "loop-variable clone is back:\n{body}");
}

#[test]
fn body_bound_let_moves_at_its_last_use() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let body = emitted_main(&format!("{PRELUDE}\
        fn main() -> Unit = {{\n\
          let data: List[Value] = [value.int(1), value.int(2)]\n\
          var t = 0\n\
          for v in data {{\n\
            let w = v\n\
            t = t + hit(w)\n\
          }}\n\
          println(\"${{t}}\")\n\
        }}\n"), "let-move");
    assert!(body.contains("let w: Value = v;"), "`let w = v` is the loop variable's last use — a move:\n{body}");
    assert!(body.contains("hit(w)") && !body.contains("w.clone()"), "a body-level `let` is rebound every iteration too:\n{body}");
}

#[test]
fn body_that_writes_the_list_keeps_the_head_copy() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let body = emitted_main(&format!("{PRELUDE}\
        fn main() -> Unit = {{\n\
          var xs: List[String] = [\"u\", \"v\"]\n\
          for s in xs {{ list.push(xs, s) }}\n\
          println(\"${{list.len(xs)}}\")\n\
        }}\n"), "writes");
    assert!(body.contains("for s in xs.clone().iter().cloned()"), "`list.push(xs, …)` holds `&mut xs` under the loop's borrow — the head copy is load-bearing:\n{body}");
    assert!(body.contains("almide_rt_list_push(&mut xs, s)"), "the pushed element is still the loop variable's last use (a move):\n{body}");
}

#[test]
fn outer_loop_variable_used_in_an_inner_loop_still_clones() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let body = emitted_main(&format!("{PRELUDE}\
        fn main() -> Unit = {{\n\
          let data: List[Value] = [value.int(1), value.int(2)]\n\
          var t = 0\n\
          for v in data {{\n\
            for i in 0..<2 {{ t = t + hit(v) + i }}\n\
          }}\n\
          println(\"${{t}}\")\n\
        }}\n"), "nested");
    assert!(body.contains("hit(v.clone())"), "an outer loop variable is NOT fresh inside the inner loop — it must clone:\n{body}");
}

#[test]
fn loop_variable_captured_by_a_closure_still_clones() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let body = emitted_main(&format!("{PRELUDE}\
        fn main() -> Unit = {{\n\
          let data: List[Value] = [value.int(1), value.int(2)]\n\
          var t = 0\n\
          for v in data {{\n\
            let f = () => hit(v)\n\
            t = t + f() + f()\n\
          }}\n\
          println(\"${{t}}\")\n\
        }}\n"), "closure");
    // The capture pass hands the closure its own copy (`__cap_N`), and the
    // closure body clones THAT on every call — the loop variable itself is
    // consumed once, by the capture bind. Before #1673 that bind read
    // `v.clone().clone()`: the capture pass's copy, cloned again by the
    // in-loop always-clone rule.
    assert!(body.contains("= v.clone();"), "the capture bind copies the loop variable exactly once:\n{body}");
    assert!(!body.contains("v.clone().clone()"), "the double clone on the capture bind is back:\n{body}");
    assert!(body.contains("hit(__cap_"), "the closure body must read its own capture, never the loop variable:\n{body}");
}

#[test]
fn map_loop_still_consumes_a_copy_when_the_map_is_used_after() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let body = emitted_main("fn eat(s: String) -> Int = string.len(s)\n\
        fn main() -> Unit = {\n\
          let m: Map[String, String] = [\"a\": \"x\"]\n\
          var t = 0\n\
          for (k, v) in m { t = t + eat(k) + eat(v) }\n\
          println(\"${t} ${list.len(map.keys(m))}\")\n\
        }\n", "map");
    assert!(body.contains("for (k, v) in m.clone()"), "a Map loop iterates BY VALUE — the copy is what keeps `m` alive for the later use:\n{body}");
}

#[test]
fn binder_the_body_only_borrows_iterates_by_reference() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let body = emitted_main(&format!("{PRELUDE}\
        fn main() -> Unit = {{\
          let us: List[U] = [U {{ a: 1 }}, U {{ a: 2 }}]\
          let names: List[String] = [\"ab\", \"c\"]\
          var t = 0\
          for u in us {{ t = t + hitu(u) }}\
          for u in us {{ t = t + u.a }}\
          for s in names {{ t = t + string.len(s) }}\
          println(\"${{t}}\")\
        }}\n"), "borrowed-binder");
    assert!(body.contains("for u in us.iter() {"), "a binder only ever passed by `&` never needs an owned element:\n{body}");
    assert!(body.contains("hitu(&u)"), "the borrowed call site is unchanged (`&&T` coerces):\n{body}");
    assert!(body.contains("for s in names.iter() {"), "a String binder read through `&*s` iterates by reference too:\n{body}");
    assert!(!body.contains("iter().cloned()"), "no loop in this program consumes its element — every `.cloned()` here is a copy nobody reads:\n{body}");
}

#[test]
fn binder_that_is_consumed_or_matched_keeps_the_element_copy() {
    if !tool_available() { eprintln!("skipping: almide binary not available"); return; }
    let body = emitted_main(&format!("{PRELUDE}\
        fn main() -> Unit = {{\
          let us: List[U] = [U {{ a: 1 }}]\
          let data: List[Value] = [value.int(1)]\
          var t = 0\
          for v in data {{ t = t + hit(v) }}\
          for u in us {{ t = t + (match u {{ U {{ a }} => a }}) }}\
          println(\"${{t}}\")\
        }}\n"), "consumed-binder");
    assert!(body.contains("for v in data.iter().cloned() {") && body.contains("hit(v)"), "an owned-arg call consumes the element — it must be copied out of the list, then moved:\n{body}");
    assert!(body.contains("for u in us.iter().cloned() {"), "a match subject binds payloads by value — the element stays owned:\n{body}");
}
