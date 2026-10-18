//! #3661 — a dying source rebound into a `var` hands its credit over.
//!
//! `var a = acc` in a fold callback, and `var a = s` before a self tail
//! call, SHARED the source's block into the new var (+1) although the
//! source is never read again. The first in-place write on `a` then met the
//! block at rc 2 and copied it whole, so a push / set loop in either shape
//! stayed quadratic in bytes — whatever the mutator (`list.push`,
//! `bytes.set_u8`, `bytes.append_u8`, a string append). The rebind of a
//! source that dies there now MOVES the credit, so the write finds rc 1.
//!
//! Each row runs its loop N and 2N times and pins the bytes the allocator
//! was asked for by the last N iterations: a constant per iteration (the
//! amortized growth of the one block), never a copy of the block. Every row
//! also releases every block it allocates. The aliasing row keeps a holder
//! of the source alive past the rebind: there the source does NOT die, the
//! write must still copy, and the holder keeps its value (C-033).

mod harness;
use harness::run_wasm;

/// The run's stdout and its `(allocs, bytes requested, frees)`.
fn run(src: &str) -> (String, u64, u64, u64) {
    let ir = almide_spine::s5::lower_to_ir("dying_rebind.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    let c = out.alloc_count.expect("the armed module exports the counters");
    (out.stdout, c.allocs, c.bytes, c.frees)
}

/// A fold whose callback rebinds its accumulator into a var and mutates it.
fn fold(init: &str, mutate: &str, show: &str, n: u32) -> String {
    format!(
        "fn main() -> Unit = {{\n  let r = list.fold(list.range(0, {n}), {init}, (acc, x) => {{\n    var a = acc\n    {mutate}\n    a\n  }})\n  println(int.to_string({show}))\n}}\n"
    )
}

/// A self tail call that rebinds its param into a var and mutates it.
fn tail(ty: &str, init: &str, mutate: &str, show: &str, n: u32) -> String {
    format!(
        "fn go(s: {ty}, x: Int) -> {ty} = if x == {n} then s else {{\n  var a = s\n  {mutate}\n  go(a, x + 1)\n}}\n\nfn main() -> Unit = {{\n  let r = go({init}, 0)\n  println(int.to_string({show}))\n}}\n"
    )
}

const N: u32 = 1000;

/// The row at N and 2N: the expected stdout, every block released, and the
/// last N iterations asking for at most `per` bytes each.
fn in_place(name: &str, program: impl Fn(u32) -> String, expect: impl Fn(u32) -> String) {
    let (o1, a1, b1, f1) = run(&program(N));
    let (o2, a2, b2, f2) = run(&program(2 * N));
    assert_eq!(o1, expect(N), "{name}: stdout at N");
    assert_eq!(o2, expect(2 * N), "{name}: stdout at 2N");
    assert_eq!(a1, f1, "{name}: every block allocated at N is released");
    assert_eq!(a2, f2, "{name}: every block allocated at 2N is released");
    let per = (b2 - b1) / N as u64;
    assert!(
        per <= 64,
        "{name}: the last {N} iterations requested {per} bytes each — the rebound var's first write copies its block"
    );
}

fn len_line(n: u32) -> String {
    format!("{n}\n")
}

#[test]
fn a_fold_accumulator_rebound_into_a_var_is_written_in_place() {
    in_place("fold list.push", |n| fold("[]", "list.push(a, x)", "list.len(r)", n), len_line);
    in_place("fold string append", |n| fold(r#""""#, r#"a = a + "b""#, "string.len(r)", n), len_line);
    in_place("fold bytes.append_u8", |n| fold("bytes.new(0)", "bytes.append_u8(a, x % 256)", "bytes.len(r)", n), len_line);
    in_place(
        "fold bytes.set_u8",
        |n| fold("bytes.new(4096)", "bytes.set_u8(a, x % 4096, x % 256)", "bytes.len(r)", n),
        |_| "4096\n".to_string(),
    );
}

#[test]
fn a_param_rebound_into_a_var_before_a_self_tail_call_is_written_in_place() {
    in_place("tail list.push", |n| tail("List[Int]", "[]", "list.push(a, x)", "list.len(r)", n), len_line);
    in_place("tail string append", |n| tail("String", r#""""#, r#"a = a + "b""#, "string.len(r)", n), len_line);
    in_place(
        "tail bytes.append_u8",
        |n| tail("Bytes", "bytes.new(0)", "bytes.append_u8(a, x % 256)", "bytes.len(r)", n),
        len_line,
    );
    in_place(
        "tail bytes.set_u8",
        |n| tail("Bytes", "bytes.new(4096)", "bytes.set_u8(a, x % 4096, x % 256)", "bytes.len(r)", n),
        |_| "4096\n".to_string(),
    );
}

/// The source read after the rebind does not die there: the write copies,
/// and every holder keeps its own value. The expected stdout is native's
/// (`almide run` on the same program).
pub const ALIASING: &str = r#"fn go(s: List[Int], x: Int) -> List[Int] = if x == 3 then s else {
  var a = s
  list.push(a, x)
  println("go ${list.len(s)} ${list.len(a)}")
  go(a, x + 1)
}

fn keep(s: String) -> String = {
  var a = s
  a = a + "!"
  s + "|" + a
}

fn main() -> Unit = {
  let r = list.fold(list.range(0, 3), [9], (acc, x) => {
    var a = acc
    list.push(a, x)
    println("fold ${list.len(acc)} ${list.len(a)}")
    a
  })
  println("${r}")
  let held = [7]
  let t = go(held, 0)
  println("${held} ${t}")
  var b = bytes.new(2)
  var c = b
  bytes.set_u8(c, 0, 5)
  println("${bytes.get_or(b, 0, 0 - 1)} ${bytes.get_or(c, 0, 0 - 1)}")
  println(keep("hi"))
  let base = "x"
  let both = list.fold(list.range(0, 2), base, (acc, k) => {
    var a = acc
    a = a + "y"
    acc + a
  })
  println("${base} ${both}")
}
"#;

pub const ALIASING_NATIVE: &str = "fold 1 2\nfold 2 3\nfold 3 4\n[9, 0, 1, 2]\ngo 1 2\ngo 2 3\ngo 3 4\n[7] [7, 0, 1, 2]\n0 5\nhi|hi!\nx xxyxxyy\n";

#[test]
fn a_source_read_after_the_rebind_still_keeps_its_value() {
    let (out, allocs, _, frees) = run(ALIASING);
    assert_eq!(out, ALIASING_NATIVE, "wasm stdout must equal native's");
    assert_eq!(allocs, frees, "every block is released");
}
