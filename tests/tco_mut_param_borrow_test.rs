//! A `mut` parameter keeps its `&mut` through the TCO loop rewrite (#2293).
//!
//! `TailCallOptPass` used to reset every parameter's borrow to `Own` except
//! `Ty::Bytes | Ty::Fn`, so the loop form of a `mut List` / `mut Map` / `mut Int`
//! parameter took the caller's value BY VALUE: the call site cloned, the loop
//! mutated the clone, and the caller saw none of it. `almide check` passed and
//! the program printed a wrong number — `count_down(seen, 3)` reported the
//! caller seeing 0 of 3 pushes, where the same function written without a tail
//! call reported 3. The wasm legs were right all along (they reach the shape
//! through the C-132 write-back rewrite, not through this pass), so it was a
//! native-only miscompile AND a cross-target divergence.
//!
//! The fix asks the question the loop actually poses — can the loop still hold
//! this reference at the top of the next iteration? — instead of asking what
//! type the parameter has. These tests pin BOTH halves of the answer:
//!
//! 1. the loop form keeps `&mut` (the borrow survived), and
//! 2. it is still a LOOP (`while true` / `__tco_result`) — the fix is not
//!    "stop optimising functions with a `mut` parameter".
//!
//! Plus the one shape the loop cannot carry — a `mut` parameter a self-call
//! rebinds to another variable, which would make the loop store a reference to
//! a binding it re-creates every iteration. That function keeps its recursive
//! form, which is correct on every target; it must never come back as a loop
//! over an owned copy.
//!
//! The behavioural twins are spec/lang/tco_test.almd (both legs) and
//! spec/wasm_cross/mut_param_tail_recursion.almd (C-226).
//!
//! Skips cleanly when the `almide` binary is unavailable (CI builds it in the
//! build step; locally run `cargo build --release` first).

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

/// Emit Rust for `source` and return the body of `fn <name>` (from its
/// `pub fn <name>` line to the first column-zero `}`).
fn emitted_fn(source: &str, name: &str, tag: &str) -> String {
    let dir = std::env::temp_dir().join(format!("almide-tco-mut-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();

    let output = Command::new(almide_bin())
        .args([src.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("failed to spawn almide");
    let rust = String::from_utf8_lossy(&output.stdout).to_string();
    std::fs::remove_dir_all(&dir).ok();
    assert!(
        output.status.success(),
        "--target rust emit failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let needle = format!("fn {name}(");
    let start = rust
        .find(&needle)
        .unwrap_or_else(|| panic!("emitted Rust has no `{needle}`:\n{rust}"));
    let rest = &rust[start..];
    let end = rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len());
    rest[..end].to_string()
}

/// `fn <name>(..)`'s signature line only.
fn signature(body: &str) -> &str {
    body.lines().next().unwrap()
}

fn assert_borrowed_loop(body: &str, param_ty: &str) {
    let sig = signature(body);
    assert!(
        sig.contains(&format!("&mut {param_ty}")),
        "the loop form dropped the `mut` parameter's borrow — the caller is \
         handed a copy and every mutation is lost (#2293):\n{body}"
    );
    assert!(
        body.contains("__tco_result") && body.contains("while true"),
        "expected the tail call to still be a LOOP — the fix keeps the borrow, \
         it does not give up on optimising `mut`-parameter functions:\n{body}"
    );
}

const LIST_SRC: &str = "\
fn count_down(mut seen: List[Int], n: Int) -> Int =
  if n == 0 then list.len(seen)
  else { list.push(seen, n); count_down(seen, n - 1) }

effect fn main() -> Unit = {
  var seen: List[Int] = []
  println(int.to_string(count_down(seen, 3)) + \" \" + int.to_string(list.len(seen)))
}
";

const MAP_SRC: &str = "\
fn fill(mut m: Map[Int, Int], n: Int) -> Int =
  if n == 0 then map.len(m)
  else { map.insert(m, n, n * 10); fill(m, n - 1) }

effect fn main() -> Unit = {
  var m: Map[Int, Int] = [:]
  println(int.to_string(fill(m, 3)) + \" \" + int.to_string(map.len(m)))
}
";

const SCALAR_SRC: &str = "\
fn bump(mut d: Int, n: Int) -> Int =
  if n == 0 then d
  else { d = d + 1; bump(d, n - 1) }

effect fn main() -> Unit = {
  var d = 0
  println(int.to_string(bump(d, 3)) + \" \" + int.to_string(d))
}
";

/// A `mut` parameter the self-call REBINDS to another variable. The loop cannot
/// carry that reference, so the function must keep its recursive form.
const REBIND_SRC: &str = "\
fn walk(mut a: List[Int], n: Int) -> Int = {
  var fresh: List[Int] = [7]
  if n == 0 then list.len(a)
  else { list.push(a, n); walk(fresh, n - 1) }
}

effect fn main() -> Unit = {
  var xs: List[Int] = []
  println(int.to_string(walk(xs, 3)) + \" \" + int.to_string(list.len(xs)))
}
";

#[test]
fn mut_list_param_stays_borrowed_in_the_loop() {
    if !tool_available() {
        eprintln!("skipping: almide binary not available");
        return;
    }
    assert_borrowed_loop(&emitted_fn(LIST_SRC, "count_down", "list"), "Vec<i64>");
}

#[test]
fn mut_map_param_stays_borrowed_in_the_loop() {
    if !tool_available() {
        eprintln!("skipping: almide binary not available");
        return;
    }
    assert_borrowed_loop(&emitted_fn(MAP_SRC, "fill", "map"), "AlmideMap<i64, i64>");
}

#[test]
fn mut_scalar_param_stays_borrowed_in_the_loop() {
    if !tool_available() {
        eprintln!("skipping: almide binary not available");
        return;
    }
    assert_borrowed_loop(&emitted_fn(SCALAR_SRC, "bump", "scalar"), "i64");
}

/// The call site must hand over the caller's storage, not a clone of it. The
/// clone at the call site was the visible half of the bug: `count_down(seen.clone(), 3)`.
#[test]
fn the_call_site_hands_over_the_callers_storage() {
    if !tool_available() {
        eprintln!("skipping: almide binary not available");
        return;
    }
    let main = emitted_fn(LIST_SRC, "__almide_main", "callsite");
    assert!(
        main.contains("count_down(&mut seen,"),
        "the call site must pass the caller's list by mutable reference:\n{main}"
    );
    assert!(
        !main.contains("count_down(seen.clone(),"),
        "the call site clones the caller's list again (#2293):\n{main}"
    );
}

#[test]
fn a_rebound_mut_param_declines_the_loop() {
    if !tool_available() {
        eprintln!("skipping: almide binary not available");
        return;
    }
    let body = emitted_fn(REBIND_SRC, "walk", "rebind");
    assert!(
        signature(&body).contains("&mut Vec<i64>"),
        "a `mut` parameter must stay borrowed whether or not the function is \
         loop-converted:\n{body}"
    );
    assert!(
        !body.contains("__tco_result"),
        "a `mut` parameter the self-call rebinds cannot become loop state — the \
         loop would hold a reference to a binding it re-creates every \
         iteration:\n{body}"
    );
}
