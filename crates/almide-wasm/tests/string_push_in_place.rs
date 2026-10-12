//! `string.push` appends in place on a unique receiver (#3639). The wasm leg
//! lowered every push as `$concat` plus a fresh rebind: a new block of the
//! whole string and a full copy per push, so a builder loop moved O(len)
//! bytes per push and was quadratic. `s = s + x` on the same var already took
//! the `$str_append` window (in place under rc == 1 with capacity slack); a
//! push is that append.
//!
//! The pin: each shape runs `n` and `2n` pushes onto a 4096-byte string, and
//! the bytes the LAST half requested, per push, must stay under a small
//! constant — independent of the string's length. Before the fix every push
//! requested the whole string (>= 4096 bytes). Shapes: a var in a loop, a
//! fresh pushed operand, a `mut` parameter, a self tail call threading a
//! `mut` parameter (8 bytes a call for the boxed parameter, not for the
//! push), and a record field (the `b.text = b.text + x` field window).
//!
//! Not pinned: `var a = acc` in a fold callback and `var a = s` before a self
//! tail call share the block with the dying parameter, so the first push
//! copies it — the receiver position's cost, which `list.push` pays alike.
//!
//! A shared receiver is still copied, so an alias keeps its text; the stdout
//! below is native's (`almide run`).

mod harness;
use harness::run_wasm;

/// `(stdout, payload bytes requested)` of the armed run; every block is
/// released by exit.
fn counted(src: &str) -> (String, u64) {
    let ir = almide_spine::s5::lower_to_ir("string_push.almd", src).expect("front");
    let bytes = {
        let _armed = almide_wasm::alloc_count::CountGuard::set();
        almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe")
    };
    let r = run_wasm(&bytes).expect("run");
    assert_eq!(r.exit, 0, "the probe exits 0: {}", r.stdout);
    let c = r.alloc_count.expect("the armed module exports the counters");
    assert_eq!(c.live(), 0, "every block is released by exit");
    (r.stdout, c.bytes)
}

/// Each shape pushes `{N}` times onto a 4096-byte start and prints the length.
const SHAPES: &[(&str, &str)] = &[
    (
        "a var in a loop",
        r#"effect fn main() -> Unit = {
  var s = string.repeat("a", 4096)
  for i in 0..<{N} {
    string.push(s, "b")
  }
  println("${string.len(s)}")
}
"#,
    ),
    (
        "a fresh pushed operand",
        r#"effect fn main() -> Unit = {
  var s = string.repeat("a", 4096)
  for i in 0..<{N} {
    string.push(s, int.to_string(i % 10))
  }
  println("${string.len(s)}")
}
"#,
    ),
    (
        "a mut parameter",
        r#"fn fill(mut s: String, n: Int) -> Unit = {
  var i = 0
  while i < n {
    string.push(s, "b")
    i = i + 1
  }
}

effect fn main() -> Unit = {
  var s = string.repeat("a", 4096)
  fill(s, {N})
  println("${string.len(s)}")
}
"#,
    ),
    (
        "a self tail call threading a mut parameter",
        r#"fn go(mut s: String, i: Int) -> String =
  if i == 0 then s
  else {
    string.push(s, "b")
    go(s, i - 1)
  }

effect fn main() -> Unit = {
  var s = string.repeat("a", 4096)
  let r = go(s, {N})
  println("${string.len(r)}")
}
"#,
    ),
    (
        "a record field",
        r#"type Buf = { text: String, n: Int }

effect fn main() -> Unit = {
  var b = Buf { text: string.repeat("a", 4096), n: 0 }
  for i in 0..<{N} {
    string.push(b.text, "b")
  }
  println("${string.len(b.text)}")
}
"#,
    ),
];

#[test]
fn the_last_half_of_the_pushes_requests_a_constant_per_push() {
    const N: u64 = 1000;
    let mut failures = Vec::new();
    for (name, src) in SHAPES {
        let (out_n, bytes_n) = counted(&src.replace("{N}", &N.to_string()));
        let (out_2n, bytes_2n) = counted(&src.replace("{N}", &(2 * N).to_string()));
        assert_eq!((out_n.trim(), out_2n.trim()), ("5096", "6096"), "{name}: the pushes landed");
        let per_push = bytes_2n.saturating_sub(bytes_n) / N;
        if per_push > 64 {
            failures.push(format!(
                "{name}: the last {N} pushes requested {per_push} bytes each ({bytes_n} -> {bytes_2n} total) — \
                 a push copied the whole string"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

const ALIASING: &str = r#"type Buf = { text: String, n: Int }

fn grow(mut s: String, x: String) -> Unit = {
  string.push(s, x)
  string.push(s, x)
}

effect fn main() -> Unit = {
  var s = "s:"
  let before = s
  var mid = ""
  for i in 0..<8 {
    string.push(s, int.to_string(i))
    if i == 3 then mid = s else ()
  }
  println("${before} ${mid} ${s}")
  var w = "ab"
  string.push(w, w)
  string.push(w, w)
  println(w)
  var m = "m"
  let m0 = m
  grow(m, "xy")
  println("${m0} ${m}")
  var b = Buf { text: "f:", n: 0 }
  let old = b
  let t = b.text
  for i in 0..<4 {
    string.push(b.text, int.to_string(i))
  }
  println("${old.text} ${t} ${b.text}")
  var e = ""
  string.push(e, "")
  string.push(e, "é")
  println("${e} ${string.len(e)}")
}
"#;

const ALIASING_NATIVE: &str = "\
s: s:0123 s:01234567
abababab
m mxyxy
f: f: f:0123
é 1
";

#[test]
fn a_shared_receiver_is_copied_and_its_alias_keeps_its_text() {
    let (out, _) = counted(ALIASING);
    assert_eq!(out, ALIASING_NATIVE);
}
