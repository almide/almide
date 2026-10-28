//! #4249 / #4250 / #4251 / #4252 — values of a recursive type, on both legs.
//!
//! - #4251: a record that reaches itself through an `Option`, a tuple, a
//!   generic parameter or another record emitted a Rust struct with no
//!   indirection (E0072), so native refused programs wasm ran. Every field
//!   that closes a cycle of the type graph is an `AlmideBox` now.
//! - #4249: a self tail call whose argument is a field of the matched value
//!   cloned the whole value every iteration on native — O(n²) and, with a
//!   recursive clone, a stack overflow at 200 000 levels where wasm answered.
//! - #4250: `==`, a map key, a set member and the interpolated literal form
//!   of a 200 000-deep value recursed once per level (native, and wasm for
//!   the list payload and the generic tree).
//! - #4252: dropping such a value recursed once per level (wasm through an
//!   `Option` or a variant's record, both legs through a `List`).
//!
//! Each program runs on BOTH legs; they must print the same bytes, and those
//! bytes are the answer below. The depth is the issues' 200 000 — far past
//! what one frame per level fits in the 8 MiB stack either leg has.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn run_leg(name: &str, src: &str, wasm: bool) -> String {
    let leg = if wasm { "wasm" } else { "native" };
    let dir = std::env::temp_dir().join(format!("almd_rec_depth_{name}_{leg}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("prog.almd");
    std::fs::write(&file, src).unwrap();
    let mut cmd = Command::new(almide());
    cmd.current_dir(&dir).args(["run", file.to_str().unwrap()]);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    let out = cmd.output().expect("almide run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "{name}: {leg} run failed (exit {:?}):\n{}\n--- program ---\n{src}",
        out.status.code(),
        stderr.lines().filter(|l| !l.starts_with("note:") && !l.starts_with("  reason:")).collect::<Vec<_>>().join("\n")
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

/// Run on both legs; they must print the same bytes, and that is the answer.
fn run_both(name: &str, src: &str) -> String {
    let native = run_leg(name, src, false);
    let wasm = run_leg(name, src, true);
    assert_eq!(native, wasm, "{name}: native and wasm disagree");
    native
}

/// #4251: every record shape that closes a cycle builds natively and
/// prints what wasm prints — through `Option`, through a variant holding a
/// record, through a tuple inside an `Option`, through a generic parameter,
/// across two records, and through a `Map` value.
#[test]
fn a_record_that_reaches_itself_builds_on_both_legs() {
    let src = r#"type T = { v: Int, left: T?, right: T? }
type L = { v: Int, next: L? }
type Cell = { v: Int, rest: M }
type M = | More(Cell) | End
type P = { v: Int, pair: (Int, P)? }
type Chain[A] = { v: A, next: Chain[A]? }
type Person = { name: String, pet: Animal? }
type Animal = { kind: String, owner: Person? }
type K = { v: Int, kids: Map[String, K] }

fn sum_t(t: T) -> Int = t.v + (match t.left { some(l) => sum_t(l), none => 0 }) + (match t.right { some(r) => sum_t(r), none => 0 })
fn sum_l(l: L) -> Int = l.v + (match l.next { some(n) => sum_l(n), none => 0 })
fn sum_m(m: M) -> Int = match m { More(c) => c.v + sum_m(c.rest), End => 0 }
fn sum_p(p: P) -> Int = p.v + (match p.pair { some((k, n)) => k + sum_p(n), none => 0 })
fn len[A](c: Chain[A]) -> Int = 1 + (match c.next { some(n) => len(n), none => 0 })

effect fn main() -> Unit = {
  let t = T { v: 1, left: some(T { v: 2, left: none, right: none }), right: some(T { v: 4, left: none, right: none }) }
  let l = L { v: 1, next: some(L { v: 2, next: some(L { v: 3, next: none }) }) }
  let m = More(Cell { v: 1, rest: More(Cell { v: 2, rest: End }) })
  let p = P { v: 1, pair: some((10, P { v: 2, pair: none })) }
  let c = Chain { v: "a", next: some(Chain { v: "b", next: none }) }
  let who = Person { name: "a", pet: some(Animal { kind: "cat", owner: some(Person { name: "b", pet: none }) }) }
  let k = K { v: 1, kids: map.set(map.new(), "a", K { v: 2, kids: map.new() }) }
  println("${sum_t(t)} ${sum_l(l)} ${sum_m(m)} ${sum_p(p)} ${len(c)}")
  println("${t == t} ${l == l} ${m == m} ${p == p} ${c == c} ${who == who} ${k == k}")
  println("${l}")
  println("${m}")
  println("${p}")
  println("${c}")
  println("${who}")
  println("${k}")
  let l2 = { ...l, v: 9 }
  var w = L { v: 0, next: none }
  w.next = some(l2)
  println("${w}")
}
"#;
    assert_eq!(
        run_both("records", src),
        "7 6 3 13 2\n\
         true true true true true true true\n\
         L { v: 1, next: some(L { v: 2, next: some(L { v: 3, next: none }) }) }\n\
         More(Cell { v: 1, rest: More(Cell { v: 2, rest: End }) })\n\
         P { v: 1, pair: some((10, P { v: 2, pair: none })) }\n\
         Chain { v: \"a\", next: some(Chain { v: \"b\", next: none }) }\n\
         Person { name: \"a\", pet: some(Animal { kind: \"cat\", owner: some(Person { name: \"b\", pet: none }) }) }\n\
         K { v: 1, kids: [\"a\": K { v: 2, kids: [:] }] }\n\
         L { v: 0, next: some(L { v: 9, next: some(L { v: 2, next: some(L { v: 3, next: none }) }) }) }"
    );
}

/// #4249: a self tail call on a field of the matched value walks 200 000
/// levels in a loop on both legs — a variant payload, a record payload
/// field, and an `Option` field (the sum reads a second field of the same
/// binder, after the field that moves on).
#[test]
fn a_tail_walk_over_a_match_bound_field_is_a_loop() {
    let src = r#"type L = | Cons(Int, L) | Nil
type C = { v: Int, next: R }
type R = | Link(C) | Stop
type O = { v: Int, next: O? }

fn size(l: L, acc: Int) -> Int = match l { Cons(_, rest) => size(rest, acc + 1), Nil => acc }
fn size_r(r: R, acc: Int) -> Int = match r { Link(c) => size_r(c.next, acc + 1), Stop => acc }
fn sum_o(o: O?, acc: Int) -> Int = match o { some(n) => sum_o(n.next, acc + n.v), none => acc }

effect fn main() -> Unit = {
  let l = list.range(0, 200000) |> list.fold(Nil, (acc, i) => Cons(i, acc))
  let r = list.range(0, 200000) |> list.fold(Stop, (acc, i) => Link(C { v: i, next: acc }))
  let o = list.range(0, 200000) |> list.fold(O { v: 0, next: none }, (acc, i) => O { v: i, next: some(acc) })
  println("${size(l, 0)} ${size_r(r, 0)} ${sum_o(some(o), 0)}")
}
"#;
    assert_eq!(run_both("tail_walk", src), "200000 200000 19999900000");
}

/// #4250: `==` (equal, unequal near the far end, unequal by length), a map
/// key, a set member and the interpolated form of 200 000-deep values — the
/// self-recursive variant, a record chain through `Option`, a `List`
/// payload, and a generic tree that is deep in its FIRST field.
#[test]
fn a_deep_value_compares_hashes_and_displays_on_a_bounded_stack() {
    let src = r#"type L = | Cons(Int, L) | Nil
type O = { v: Int, next: O? }
type N = | N(Int, List[N])
type Tree[A] = | Leaf | Node(Tree[A], A, Tree[A])

fn build(n: Int, poison: Int) -> L = list.range(0, n) |> list.fold(Nil, (acc, i) => Cons(if i == poison then -1 else i, acc))

effect fn main() -> Unit = {
  let a = build(200000, -5)
  let b = build(200000, -5)
  println("${a == a} ${a == b} ${a == build(200000, 3)} ${a == build(199999, -5)} ${a != build(200000, 3)}")
  println("${map.get(map.set(map.new(), a, 7), b) ?? 0} ${set.contains(set.from_list([a]), b)} ${set.contains(set.from_list([a]), build(10, -5))}")
  let o = list.range(0, 200000) |> list.fold(O { v: 0, next: none }, (acc, i) => O { v: i, next: some(acc) })
  let s = "${a}"
  let t = "${o}"
  println("${string.len(s)} ${string.slice(s, 0, 24)} ${string.len(t)} ${string.slice(t, 0, 28)} ${o == o}")
  let n = list.range(0, 200000) |> list.fold(N(0, []), (acc, i) => N(i, [acc]))
  let tree = list.range(0, 200000) |> list.fold(Leaf, (acc, i) => Node(acc, i, Leaf))
  println("${n == n} ${tree == tree} ${tree == (list.range(1, 200000) |> list.fold(Leaf, (acc, i) => Node(acc, i, Leaf)))}")
  println("${string.len("${n}")} ${string.len("${tree}")}")
}
"#;
    assert_eq!(
        run_both("deep_ops", src),
        "true true false false true\n\
         7 true false\n\
         2688893 Cons(199999, Cons(199998 5688912 O { v: 199999, next: some(O  true\n\
         true true false\n\
         2488898 3888894"
    );
}

/// #4250: a deep value displayed where its text is NOT the last of its
/// parent's — a left-deep tree, a record whose list holds the next level
/// beside a sibling, two deep values in one line, inside a list and a
/// tuple — places every level where a shallow display would, on both legs.
#[test]
fn a_deep_value_displays_in_place_wherever_it_sits() {
    let src = r#"type Tree[A] = | Leaf | Node(Tree[A], A, Tree[A])
type T = { v: Int, kids: List[T] }
fn mk(n: Int) -> Tree[Int] = if n == 0 then Leaf else if n % 3 == 0 then Node(mk(n - 1), n, Node(Leaf, -n, Leaf)) else Node(Node(Leaf, n, Leaf), n, mk(n - 1))
effect fn main() -> Unit = {
  let t = list.range(0, 5000) |> list.fold(Leaf, (acc, i) => Node(acc, i, Node(Leaf, i, Leaf)))
  println("${t}")
  let k = list.range(0, 4000) |> list.fold(T { v: 0, kids: [] }, (acc, i) => T { v: i, kids: [acc, T { v: -i, kids: [] }, acc] |> list.take(if i % 2 == 0 then 2 else 1) })
  println("x ${k} y ${k} z")
  println("${mk(3000)}")
  println("${[mk(1500), mk(1600)]} and ${(t, 1)}")
}
"#;
    let out = run_both("in_place", src);
    assert_eq!(out.len(), 855303);
    assert!(out.starts_with("Node(Node(Node("), "{}", &out[..80]);
}

/// #4252: a 200 000-deep value is dropped without one frame per level after
/// its program has printed — through an `Option`, through a variant holding
/// a record, through a `List`, across two records, and through a generic
/// parameter.
#[test]
fn a_deep_value_drops_on_a_bounded_stack() {
    let src = r#"type O = { v: Int, next: O? }
type Cell = { v: Int, rest: M }
type M = | More(Cell) | End
type T = { v: Int, kids: List[T] }
type Person = { name: String, pet: Animal? }
type Animal = { kind: String, owner: Person? }
type Chain[A] = { v: A, next: Chain[A]? }

effect fn main() -> Unit = {
  let o = list.range(0, 200000) |> list.fold(O { v: 0, next: none }, (acc, i) => O { v: i, next: some(acc) })
  let m = list.range(0, 200000) |> list.fold(End, (acc, i) => More(Cell { v: i, rest: acc }))
  let t = list.range(0, 200000) |> list.fold(T { v: 0, kids: [] }, (acc, i) => T { v: i, kids: [acc] })
  let p = list.range(0, 100000) |> list.fold(Person { name: "p", pet: none }, (acc, i) => Person { name: "q", pet: some(Animal { kind: "k", owner: some(acc) }) })
  let c = list.range(0, 200000) |> list.fold(Chain { v: "a", next: none }, (acc, i) => Chain { v: "b", next: some(acc) })
  println("${o.v} ${match m { More(x) => x.v, End => -1 }} ${t.v} ${p.name} ${c.v}")
}
"#;
    assert_eq!(run_both("deep_drop", src), "199999 199999 199999 q b");
}
