//! An index-assign into a list a closure captures and the fn reassigns, on
//! the certified native render (#4640).
//!
//! Such a list lives in a shared cell. A heap element's write read the list
//! as a plain local for its bounds guard, found none, and the fn was refused;
//! the guard now reads the cell, as every other reference to the list does,
//! and the rebind writes it back. A scalar element's write had no cell form
//! at all; it now takes the list out of the cell, makes it unique, stores the
//! element and stores the list back, as a mutable global's write does.
use std::process::Command;

/// The program on the native leg under the ownership certificate with no
/// fallback to the standard codegen, and on wasm: both print `expected`.
fn renders_and_agrees(name: &str, source: &str, expected: &str) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, source).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "{name} {extra:?}");
    }
}

/// A swap in a call-bearing `if` arm, then a sort through closures that read the list.
const REORDER: &str = r##"type Info = { cp: Int, cat: Int, pos: Int }

fn merge(b: List[Info], s: Int, e: Int) -> List[Info] = b |> list.enumerate |> list.map(((i, x)) => if i >= s and i < e then { ...x, pos: 9 } else x)

fn reorder(buf: List[Info], start: Int, end: Int, script: Int) -> List[Info] = {
  var b = buf
  if script == 2 and start + 3 <= end and b[start].cp == 7 then {
    b = merge(b, start + 1, start + 3)
    let h = b[start + 1]
    b[start + 1] = b[start + 2]
    b[start + 2] = h
  } else ()
  let idx = list.range(0, end - start)
  let order = list.sort_by(idx, (k) => b[start + k].pos * 4096 + k)
  let sorted = list.map(order, (k) => b[start + k])
  for k in 0..<list.len(sorted) { b[start + k] = sorted[k] }
  b
}

effect fn main() -> Unit = {
  let xs = [Info { cp: 7, cat: 1, pos: 5 }, Info { cp: 8, cat: 4, pos: 0 }, Info { cp: 9, cat: 5, pos: 3 }, Info { cp: 1, cat: 0, pos: 0 }]
  for sc in 1..<4 {
    let r = reorder(xs, 0, 4, sc)
    println(r |> list.map((x) => "${x.cp}/${x.cat}/${x.pos}") |> list.join(" "))
  }
}
"##;

const REORDER_STDOUT: &str = "8/4/0 1/0/0 9/5/3 7/1/5\n1/0/0 7/1/5 9/5/9 8/4/9\n8/4/0 1/0/0 9/5/3 7/1/5\n";

/// Int and Bool lists in cells: stores in both arms, a for body, a while body; an alias stays as it was.
const SCALAR_CELLS: &str = r##"fn rot(xs: List[Int]) -> List[Int] = list.drop(xs, 1) + list.take(xs, 1)

fn show(xs: List[Int]) -> String = xs |> list.map((x) => int.to_string(x)) |> list.join(" ")

fn run(buf: List[Int], i: Int) -> String = {
  var b = buf
  var seen = list.repeat(false, list.len(b))
  if i % 2 == 0 and list.len(b) > 2 then {
    b = rot(b)
    let h = b[1]
    b[1] = b[2]
    b[2] = h * 10
    seen[i % 4] = true
  } else {
    b[0] = b[3] + i
  }
  let before = show(b)
  let weight = (k) => b[k] * 16 + (if seen[k] then 1 else 0)
  for k in 0..<list.len(b) {
    if weight(k) % 3 == i % 3 then { b[k] = weight(k) } else ()
  }
  var j = 0
  while j < 2 {
    b[j] = b[j] + b[j + 1]
    j = j + 1
  }
  if i > 5 then { seen = [] } else ()
  before + " | " + show(b) + " | " + int.to_string(list.len(seen))
}

effect fn main() -> Unit = {
  for i in 0..<4 { println(run([1, 2, 3, 4], i)) }
  let alias = [5, 6, 7, 8]
  println(run(alias, 2))
  println(show(alias))
}
"##;

const SCALAR_CELLS_STDOUT: &str = "2 4 30 1 | 37 484 480 1 | 4\n5 2 3 4 | 7 5 3 64 | 4\n2 4 30 1 | 36 34 30 1 | 4\n7 2 3 4 | 9 50 48 4 | 4\n6 8 70 5 | 134 1249 1121 80 | 4\n5 6 7 8\n";

#[test]
fn an_index_assign_into_a_shared_cell_list_is_guarded_and_written() {
    renders_and_agrees("reorder", REORDER, REORDER_STDOUT);
}

#[test]
fn a_scalar_element_store_into_a_shared_cell_list_goes_through_the_cell() {
    renders_and_agrees("scalar_cells", SCALAR_CELLS, SCALAR_CELLS_STDOUT);
}
