//! #2117: a string accumulator past the largest size class keeps growing.
//! Above `16 << 15` the allocator stops rounding requests up to a class, so
//! `$str_append`'s in-place window could never fire again: every append
//! reallocated the whole string and `$free` abandons blocks that big, which
//! walked an 8 MB accumulator into C-197 on a machine with gigabytes free.
//! Native has no such bound, so the ceiling was also a divergence.
use std::path::Path;
use std::process::Command;

fn build(dir: &Path, source: &Path, name: &str, flags: &[&str]) -> std::path::PathBuf {
    let artifact = dir.join(name);
    let output = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("build")
        .arg(source)
        .args(flags)
        .arg("-o")
        .arg(&artifact)
        .output()
        .expect("build");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    artifact
}

fn run(program: &Path, wasm: bool) -> String {
    let mut command = if wasm { Command::new("wasmtime") } else { Command::new(program) };
    if wasm {
        command.arg("run").arg(program);
    }
    let output = command.output().expect("run");
    assert!(
        output.status.success(),
        "{} exited {:?}: {}",
        program.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8")
}

#[test]
fn an_accumulator_past_the_class_ceiling_completes_on_both_legs() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("acc.almd");
    // Doubling reaches the 16 << 15 ceiling in nineteen appends, then two
    // thousand small appends run past it — the shape that used to abandon
    // a gigabyte of outgrown blocks and abort. Reaching the size by
    // doubling keeps the native leg (whose `+` copies) fast enough to be
    // the oracle.
    std::fs::write(
        &source,
        r#"effect fn main() -> Unit = {
  var acc = "a"
  var d = 0
  while d < 19 {
    acc = acc + acc
    d = d + 1
  }
  var n = 2000
  while n > 0 {
    acc = acc + "aaaaaaaaaaaaaaaa"
    n = n - 1
  }
  println("len=${string.len(acc)}")
  println("tail=${string.slice(acc, string.len(acc) - 3, string.len(acc))}")
}
"#,
    )
    .expect("source");
    // A 32 MiB ceiling is the assertion: the live accumulator never passes
    // 2 MiB, so only the abandoned copies of the old shape can reach it
    // (they summed to about a gigabyte). --heap-cap is honoured by both
    // legs, so one number states the promise for both.
    let cap = ["--heap-cap", "33554432"];
    let native = build(dir.path(), &source, "native", &cap);
    let core = build(dir.path(), &source, "core.wasm", &[&cap[..], &["--target", "wasm"]].concat());
    let expected = "len=556288\ntail=aaa\n";
    assert_eq!(run(&native, false), expected, "native");
    assert_eq!(run(&core, true), expected, "core wasm");
}

/// #2117's other half: the spelling `CLAUDE.md` tells writers to prefer.
/// `build(acc + s, n - 1)` never reached `$str_append` — the tail call emitted
/// a concat and a release of the old block — so the accumulator reallocated
/// per iteration and abandoned every outgrown copy, exactly as the `var` form
/// did before the allocator change. Under the same ceiling both legs must
/// finish, and the answer must be the imperative spelling's.
#[test]
fn a_tail_recursive_accumulator_grows_like_the_assign_form() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("rec.almd");
    std::fs::write(
        &source,
        r#"fn dbl(acc: String, n: Int) -> String =
  if n == 0 then acc else dbl(acc + acc, n - 1)

fn build(acc: String, n: Int) -> String =
  if n == 0 then acc else build(acc + "aaaaaaaaaaaaaaaa", n - 1)

effect fn main() -> Unit = {
  let acc = build(dbl("a", 19), 2000)
  println("len=${string.len(acc)}")
  println("tail=${string.slice(acc, string.len(acc) - 3, string.len(acc))}")
}
"#,
    )
    .expect("source");
    let cap = ["--heap-cap", "33554432"];
    let native = build(dir.path(), &source, "native", &cap);
    let core = build(dir.path(), &source, "core.wasm", &[&cap[..], &["--target", "wasm"]].concat());
    let expected = "len=556288\ntail=aaa\n";
    assert_eq!(run(&native, false), expected, "native");
    assert_eq!(run(&core, true), expected, "core wasm");
}

/// The list twin of the same shape: `build(acc + [e], n - 1)` took `$concat`'s
/// full copy per iteration and abandoned every outgrown block, so 200,000
/// elements — 1.6 MB — aborted where the `var` + `while` spelling finished
/// instantly.
#[test]
fn a_tail_recursive_list_accumulator_grows_like_the_assign_form() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("listrec.almd");
    std::fs::write(
        &source,
        r#"fn build(acc: List[Int], n: Int) -> List[Int] =
  if n == 0 then acc else build(acc + [n], n - 1)

effect fn main() -> Unit = {
  let xs = build([], 200000)
  println("len=${list.len(xs)} head=${list.get(xs, 0) ?? -1} last=${list.get(xs, list.len(xs) - 1) ?? -1}")
}
"#,
    )
    .expect("source");
    let cap = ["--heap-cap", "33554432"];
    let native = build(dir.path(), &source, "native", &cap);
    let core = build(dir.path(), &source, "core.wasm", &[&cap[..], &["--target", "wasm"]].concat());
    let expected = "len=200000 head=200000 last=1\n";
    assert_eq!(run(&native, false), expected, "native");
    assert_eq!(run(&core, true), expected, "core wasm");
}

/// #3519 row 2, the prepend twin: `go(i + 1, n, [[i]] + acc)` took `$concat`'s
/// full copy per iteration — a +1 on every element of the copy, a -1 on every
/// element of the released `acc` — and each outgrown block was stranded between
/// the inner lists allocated since, so 30,000 prepends reached 1.27 GB and 4 s
/// on wasm (native 0.45 s). The block now grows in place and the old slots
/// shift right; the live list stays under 1 MiB, so the 32 MiB ceiling only
/// the stranded copies could reach is the assertion.
#[test]
fn a_tail_recursive_prepend_onto_a_list_of_lists_grows_in_place() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("prepend.almd");
    std::fs::write(
        &source,
        r#"fn go(i: Int, n: Int, acc: List[List[Int]]) -> List[List[Int]] =
  if i >= n then acc else go(i + 1, n, [[i]] + acc)

effect fn main() -> Unit = {
  let xs = go(0, 30000, [])
  println("len=${list.len(xs)} head=${list.get(xs, 0) ?? []} last=${list.get(xs, list.len(xs) - 1) ?? []}")
}
"#,
    )
    .expect("source");
    let cap = ["--heap-cap", "33554432"];
    let native = build(dir.path(), &source, "native", &cap);
    let core = build(dir.path(), &source, "core.wasm", &[&cap[..], &["--target", "wasm"]].concat());
    let expected = "len=30000 head=[29999] last=[0]\n";
    assert_eq!(run(&native, false), expected, "native");
    assert_eq!(run(&core, true), expected, "core wasm");
}

/// The prepend window moves slots, not credits: every element type the layout
/// gives a slot (8-byte Int/Float, 4-byte String/Bool/list/record/variant
/// handles), an element shared with the caller, an `acc` the caller still
/// holds (the `$cow` copy), and an element that reads `acc` itself — each
/// printed by both legs and compared.
#[test]
fn the_prepend_window_agrees_with_native_on_every_slot_kind_and_on_shared_values() {
    let dir = tempfile::tempdir().expect("scratch");
    let source = dir.path().join("kinds.almd");
    std::fs::write(
        &source,
        r#"type P = { x: Int, s: String }
type T = | Leaf | Node(Int, String)

fn ints(i: Int, n: Int, acc: List[Int]) -> List[Int] =
  if i >= n then acc else ints(i + 1, n, [i * 3] + acc)
fn floats(i: Int, n: Int, acc: List[Float]) -> List[Float] =
  if i >= n then acc else floats(i + 1, n, [int.to_float(i) / 2.0] + acc)
fn strs(i: Int, n: Int, acc: List[String]) -> List[String] =
  if i >= n then acc else strs(i + 1, n, ["s${i}"] + acc)
fn bools(i: Int, n: Int, acc: List[Bool]) -> List[Bool] =
  if i >= n then acc else bools(i + 1, n, [i % 3 == 0] + acc)
fn lists(i: Int, n: Int, acc: List[List[Int]]) -> List[List[Int]] =
  if i >= n then acc else lists(i + 1, n, [[i, i + 1]] + acc)
fn recs(i: Int, n: Int, acc: List[P]) -> List[P] =
  if i >= n then acc else recs(i + 1, n, [{ x: i, s: "r${i}" }] + acc)
fn vars(i: Int, n: Int, acc: List[T]) -> List[T] =
  if i >= n then acc else vars(i + 1, n, [if i % 2 == 0 then Leaf else Node(i, "n${i}")] + acc)
fn shared_elem(i: Int, n: Int, e: List[Int], acc: List[List[Int]]) -> List[List[Int]] =
  if i >= n then acc else shared_elem(i + 1, n, e, [e] + acc)
fn reads_acc(i: Int, n: Int, acc: List[Int]) -> List[Int] =
  if i >= n then acc else reads_acc(i + 1, n, [list.len(acc)] + acc)

effect fn main() -> Unit = {
  println("${ints(0, 40, [])}")
  println("${floats(0, 9, [0.25])}")
  println("${strs(0, 20, ["z"])}")
  println("${bools(0, 10, [])}")
  println("${lists(0, 12, [[9]])}")
  println("${recs(0, 5, [])}")
  println("${vars(0, 7, [])}")
  let base = [100, 200]
  let grown = ints(0, 5, base)
  println("${base} ${grown}")
  let inner = [7, 8]
  let s = shared_elem(0, 4, inner, [])
  println("${inner} ${s}")
  let ll = [[1], [2]]
  let g2 = lists(0, 3, ll)
  println("${ll} ${g2}")
  println("${reads_acc(0, 8, [])}")
}
"#,
    )
    .expect("source");
    let native = build(dir.path(), &source, "native", &[]);
    let core = build(dir.path(), &source, "core.wasm", &["--target", "wasm"]);
    let want = run(&native, false);
    assert!(want.contains("[100, 200] [12, 9, 6, 3, 0, 100, 200]"), "{want}");
    assert_eq!(run(&core, true), want, "core wasm");
}
