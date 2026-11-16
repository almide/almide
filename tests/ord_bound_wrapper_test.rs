//! #4758: an ordering behind a generic stdlib wrapper is refused at check
//! time, as the ordering it wraps is.
//!
//! `list.max` refuses an element with no order (E030): its runtime fn's
//! `T: Ord` bound is read into the checker's capability table. A wrapper
//! written in Almide — `deque.max[A](d) = list.max(to_list(d))`, the heap's
//! natural order, `set.sort`, every `*_by` key — carried no such bound, so
//! `almide check` passed it at `Set[Int]` and native then failed rustc
//! (`AlmideSet<i64>: Ord`) while wasm walled. Each now declares `[A: Ord]` on
//! the variable it orders, and the built-in `Ord` bound is judged by the same
//! predicate and diagnostic as `list.max` (it used to be read off the
//! conformance table, which lists no primitive: `[A: Ord]` refused Int and
//! took a Set).

use std::process::Command;

fn check(program: &str) -> (bool, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("check")
        .arg(&source)
        .current_dir(dir.path())
        .output()
        .expect("almide check");
    let report =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    (out.status.success(), report)
}

fn refused(label: &str, body: &str, fn_name: &str) {
    let program = format!("import secret\neffect fn main() -> Unit = {{\n{body}\n}}\n");
    let (ok, report) = check(&program);
    assert!(
        !ok,
        "{label}: check accepted an unordered element:\n{program}"
    );
    assert!(
        report.contains("E030") && report.contains("has no ordering") && report.contains(fn_name),
        "{label}: expected E030 naming {fn_name}, got:\n{report}"
    );
}

fn accepted(label: &str, body: &str) {
    let program = format!("effect fn main() -> Unit = {{\n{body}\n}}\n");
    let (ok, report) = check(&program);
    assert!(
        ok,
        "{label}: check refused an ordered element:\n{program}\n{report}"
    );
}

#[test]
fn an_unordered_element_is_e030_through_every_ordering_wrapper() {
    refused(
        "deque.max over Set",
        "  let m = deque.max(deque.from_list([set.from_list([1]), set.from_list([2])]))\n  println(\"${list.len([m])}\")",
        "deque.max",
    );
    refused(
        "deque.min over Secret",
        "  let m = deque.min(deque.from_list([secret.new(1), secret.new(2)]))\n  println(\"${list.len([m])}\")",
        "deque.min",
    );
    refused(
        "deque.sort over Set",
        "  println(\"${list.len(deque.sort(deque.from_list([set.from_list([1])])))}\")",
        "deque.sort",
    );
    refused(
        "heap.new at a heap of heaps",
        "  let h: Heap[Heap[Int]] = heap.new()\n  println(\"${heap.len(h)}\")",
        "heap.new",
    );
    refused(
        "heap.from_list over Secret",
        "  let h = heap.from_list([secret.new(1), secret.new(2)])\n  println(\"${heap.len(h)}\")",
        "heap.from_list",
    );
    refused(
        "heap.new_by with a Set key",
        "  let h: Heap[Int] = heap.new_by((x) => set.from_list([x]))\n  println(\"${heap.len(h)}\")",
        "heap.new_by",
    );
    refused(
        "list.max_by with a Set key",
        "  println(\"${list.len([list.max_by([1, 2], (x) => set.from_list([x]))])}\")",
        "list.max_by",
    );
    refused(
        "set.sort_by with a Map key",
        "  println(\"${list.len(set.sort_by(set.from_list([1, 2]), (x) => [\"k\": x]))}\")",
        "set.sort_by",
    );
}

#[test]
fn ordered_elements_still_pass_through_the_same_wrappers() {
    accepted(
        "Int and String",
        "  println(\"${deque.max(deque.from_list([3, 1, 2]))} ${deque.min(deque.from_list([\"b\", \"a\"]))}\")\n  \
         var h = heap.from_list([\"b\", \"a\"])\n  println(\"${heap.pop(h)}\")\n  \
         let e: Heap[Int] = heap.new()\n  println(\"${heap.len(e)}\")\n  \
         println(\"${list.max_by([1, 2], (x) => int.to_string(x))} ${set.sort(set.from_list([2, 1]))}\")",
    );
    accepted(
        "Float, Bool, a list and an option",
        "  println(\"${deque.max(deque.from_list([1.5, 0.5]))} ${deque.min(deque.from_list([true, false]))}\")\n  \
         println(\"${deque.max(deque.from_list([[1, 2], [3]]))} ${deque.min(deque.from_list([some(1), none]))}\")",
    );
}

/// A user's own `[A: Ord]` takes what `list.sort` orders, and only that.
#[test]
fn a_declared_ord_bound_is_the_order_list_sort_uses() {
    let (ok, report) = check(
        "fn top[A: Ord](x: A) -> A = x\neffect fn main() -> Unit = {\n  println(\"${top(1)} ${top(\"s\")} ${top(1.5)} ${top(true)} ${top([1, 2])}\")\n}\n",
    );
    assert!(ok, "an [A: Ord] bound refused an ordered type:\n{report}");
    let (ok, report) = check(
        "fn top[A: Ord](x: A) -> A = x\neffect fn main() -> Unit = {\n  println(\"${set.len(top(set.from_list([1])))}\")\n}\n",
    );
    assert!(
        !ok && report.contains("E030"),
        "an [A: Ord] bound took a Set:\n{report}"
    );
}

/// The ordered cells still build and answer alike on both targets: the bound
/// changes what check accepts, never what an accepted program does.
#[test]
fn an_ordered_wrapper_call_still_runs_on_native() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(
        &source,
        "effect fn main() -> Unit = {\n  var h = heap.from_list([3, 1, 2])\n  println(\"${deque.max(deque.from_list([3, 1, 2]))} ${heap.pop(h)} ${list.max_by([\"a\", \"bb\"], (s) => string.len(s))}\")\n}\n",
    )
    .expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&source)
        .current_dir(dir.path())
        .output()
        .expect("almide run");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "some(3) some(1) some(\"bb\")\n"
    );
}
