//! Value semantics under the per-loop-entry copy-on-write judge (#2150,
//! src/cow_hoist.rs). A loop that reaches a list only through element reads
//! and stores judges it once per loop ENTRY instead of once per store; each
//! case below is a way that could let a store land in a block someone else
//! still holds. Every case runs on the wasm leg and on the interpreter (the
//! definition, ARCHITECTURE.md §6.6), and pins the expected text as well, so
//! a bug in both legs cannot agree silently.

mod harness;
use harness::run_wasm;

fn check(name: &str, src: &str, expected: &str) {
    let file = format!("{name}.almd");
    let ir = almide_spine::s5::lower_to_ir(&file, src).expect("lowers");
    let bytes = almide_wasm::emit_program(&ir).expect("emits");
    let run = run_wasm(&bytes).expect("runs");
    assert_eq!(run.exit, 0, "{name}: wasm exited {}: {}", run.exit, run.stderr);
    let interp = almide_spine::s5::run_file(&file, src).expect("interp runs");
    assert_eq!(interp.exit, 0, "{name}: oracle run failed: {}", interp.stderr);
    assert_eq!(run.stdout, interp.stdout, "{name}: wasm and the interpreter disagree");
    assert_eq!(run.stdout, expected, "{name}: both legs agree on the wrong text");
}

/// A list shared BEFORE the loop: the first store copies, the alias keeps
/// the old contents, and every later store goes to the copy.
#[test]
fn an_alias_taken_before_the_loop_keeps_its_contents() {
    check(
        "alias_before",
        r#"fn main() -> Unit = {
  var a: List[Int] = list.repeat(0, 5)
  let b = a
  var i = 0
  while i < 5 {
    a[i] = a[i] + i + 1
    i = i + 1
  }
  println("${b}")
  println("${a}")
}
"#,
        "[0, 0, 0, 0, 0]\n[1, 2, 3, 4, 5]\n",
    );
}

/// An alias taken INSIDE the loop, between stores: the scan must refuse the
/// list (it is read as a value there), so every store judges again.
#[test]
fn an_alias_taken_inside_the_loop_is_not_written_through() {
    check(
        "alias_inside",
        r#"fn main() -> Unit = {
  var a: List[Int] = list.repeat(0, 3)
  var snaps: List[List[Int]] = []
  for i in 0..<3 {
    a[i] = 10 + i
    list.push(snaps, a)
  }
  for s in snaps {
    println("${s}")
  }
}
"#,
        "[10, 0, 0]\n[10, 11, 0]\n[10, 11, 12]\n",
    );
}

/// The inner loop re-enters after the outer loop shared the list again: its
/// flag must be cleared on EVERY entry, or the second entry's first store
/// would write through the snapshot.
#[test]
fn a_nested_loop_re_judges_on_every_entry() {
    check(
        "nested_reentry",
        r#"fn main() -> Unit = {
  var a: List[Int] = [1, 2, 3]
  var r = 0
  while r < 3 {
    let snap = a
    for k in 0..<3 {
      a[k] = a[k] * 2
    }
    println("${snap}")
    r = r + 1
  }
  println("${a}")
}
"#,
        "[1, 2, 3]\n[2, 4, 6]\n[4, 8, 12]\n[8, 16, 24]\n",
    );
}

/// Handle elements: the replaced element's credit is still released per
/// store, and the swap through a temporary reads before it writes.
#[test]
fn string_elements_are_swapped_and_released_per_store() {
    check(
        "string_swap",
        r#"fn main() -> Unit = {
  var a: List[String] = ["a", "b", "c", "d"]
  let keep = a
  var i = 0
  while i < 2 {
    let t = a[i]
    a[i] = a[3 - i]
    a[3 - i] = t
    i = i + 1
  }
  println("${keep}")
  println("${a}")
}
"#,
        "[\"a\", \"b\", \"c\", \"d\"]\n[\"d\", \"c\", \"b\", \"a\"]\n",
    );
}

/// Two lists in one loop, one element-only and one handed to a call: the
/// first is judged once, the second per store, and neither leaks into the
/// other.
#[test]
fn two_lists_one_escaping() {
    check(
        "two_lists",
        r#"fn total(xs: List[Int]) -> Int = list.fold(xs, 0, (acc, x) => acc + x)

fn main() -> Unit = {
  var a: List[Int] = list.repeat(1, 4)
  var b: List[Int] = list.repeat(1, 4)
  let a0 = a
  let b0 = b
  var sums = 0
  for i in 0..<4 {
    a[i] = a[i] + i
    b[i] = b[i] + 10
    sums = sums + total(b)
  }
  println("${a0}")
  println("${b0}")
  println("${a}")
  println("${b}")
  println(int.to_string(sums))
}
"#,
        "[1, 1, 1, 1]\n[1, 1, 1, 1]\n[1, 2, 3, 4]\n[11, 11, 11, 11]\n116\n",
    );
}

/// The pre-judge (#2980): a counting loop whose first statement-level store
/// is certain judges once in its preheader. Reads before that store, in the
/// first iteration and after, see the list's own contents, and the alias
/// taken before the loop keeps the old ones.
#[test]
fn a_prejudged_loop_reads_before_its_first_store() {
    check(
        "prejudge_reads",
        r#"fn main() -> Unit = {
  var a: List[Int] = [1, 2, 3]
  let keep = a
  for i in 0..<3 {
    let x = a[2 - i]
    a[i] = x * 10
  }
  println("${keep}")
  println("${a}")
}
"#,
        "[1, 2, 3]\n[30, 20, 300]\n",
    );
}

/// An empty range runs no iteration, so the preheader judges nothing: the
/// shared list is neither copied nor written.
#[test]
fn an_empty_range_does_not_judge() {
    check(
        "prejudge_empty",
        r#"fn main() -> Unit = {
  var a: List[Int] = [1, 2, 3]
  let keep = a
  let n = list.len(keep) - 3
  for i in 0..<n {
    a[i] = 9
  }
  println("${keep}")
  println("${a}")
}
"#,
        "[1, 2, 3]\n[1, 2, 3]\n",
    );
}

/// A `break` before the store makes the store uncertain, so the loop keeps
/// the per-entry flag; the alias still keeps its contents either way.
#[test]
fn a_break_before_the_store_keeps_the_flag() {
    check(
        "prejudge_break",
        r#"fn main() -> Unit = {
  var a: List[Int] = [1, 2, 3, 4]
  let keep = a
  for i in 0..<4 {
    if i >= 2 then break
    a[i] = 7
  }
  println("${keep}")
  println("${a}")
}
"#,
        "[1, 2, 3, 4]\n[7, 7, 3, 4]\n",
    );
}

/// An enclosing loop's flag and an inner pre-judged loop on the same list:
/// the outer loop's own store after the inner loop sees the flag the
/// preheader set, and the snapshot taken before the outer loop is untouched.
#[test]
fn an_inner_prejudge_sets_the_outer_flag() {
    check(
        "prejudge_nested_flag",
        r#"fn main() -> Unit = {
  var a: List[Int] = [1, 2, 3]
  let keep = a
  var r = 0
  while r < 2 {
    for k in 0..<3 {
      a[k] = a[k] + 1
    }
    a[0] = a[0] * 10
    r = r + 1
  }
  println("${keep}")
  println("${a}")
}
"#,
        "[1, 2, 3]\n[210, 4, 5]\n",
    );
}
