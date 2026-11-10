//! #4006 class P: a heap param its body takes over is a MOVE param on the
//! native render — the callee takes the caller's value, and the caller hands
//! it over at the call instead of the callee copying it
//! (`native_move_params`). The program's `native-calls` witness states both
//! sides (proofs/NativeCalls.v).

fn certified(src: &str) -> (String, Vec<(String, String, String)>) {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source_certified(&src).expect("renders on v1"))
        .expect("spawn")
        .join()
        .expect("join")
}

const KEEP: &str = "fn keep(xs: List[Int]) -> List[Int] = xs\n\
     fn grow(n: Int) -> List[Int] = {\n  var xs = [1, 2]\n  var i = 0\n  while i < n {\n    xs = keep(xs + [i])\n    i = i + 1\n  }\n  xs\n}\n\
     fn main() -> Unit = println(int.to_string(list.len(grow(3))))\n";

#[test]
fn an_identity_param_moves_instead_of_copying() {
    let (rust, certs) = certified(KEEP);
    assert!(rust.contains("fn almd_keep(mut v1: Vec<i64>) -> Vec<i64> {\n    v1\n}"), "keep does not take its list:\n{rust}");
    assert!(!rust.contains(".to_vec()"), "a copy is left:\n{rust}");
    let (_, _, calls) = certs.iter().find(|(p, ..)| p == "native-calls").expect("a native-calls witness");
    // keep's block declares its list param a move; grow's site hands it over (`M`).
    assert!(calls.split('#').any(|b| b.starts_with("1||")), "{calls}");
    assert!(calls.contains('M'), "{calls}");
}

#[test]
fn a_param_the_body_only_reads_keeps_borrowing() {
    let (rust, _) = certified(
        "fn total(xs: List[Int]) -> Int = list.fold(xs, 0, (a, x) => a + x)\n\
         fn main() -> Unit = println(int.to_string(total([1, 2, 3])))\n",
    );
    assert!(rust.contains("fn almd_total(v0: &[i64])"), "{rust}");
}

#[test]
fn a_list_concatenated_with_itself_is_copied_not_grown() {
    // `xs + xs` reads the left list as the right one: growing the left
    // buffer in place would read the moved-out list.
    let (rust, _) = certified(
        "fn twice(n: Int) -> Int = {\n  let xs = [n, n + 1]\n  let ys = xs + xs\n  list.len(ys)\n}\n\
         fn main() -> Unit = println(int.to_string(twice(3)))\n",
    );
    let f = &rust[rust.find("fn almd_twice(").expect("twice")..];
    let f = &f[..f.find("\n}\n").unwrap_or(f.len())];
    assert!(f.contains("Vec::with_capacity"), "the self-concatenation is not a copy:\n{f}");
}

#[test]
fn a_slot_that_starts_at_a_literal_is_read_as_the_slot() {
    // `acc` starts at "" and is rebound each round: its later reads are the
    // slot's string, never the literal it started at.
    let (rust, _) = certified(
        "fn main() -> Unit = println([1, 2, 3] |> list.fold(\"\", (acc, n) => acc + int.to_string(n)))\n",
    );
    assert!(!rust.contains(".prepend(\"\")"), "the slot is read as its first literal:\n{rust}");
}

/// A `mut` buffer the body rebinds (`bytes.push(out, b)` writes `out` back):
/// `emit` takes it over and grows it in place, `escape` hands it to `emit`
/// from inside a `match` arm, and `build`'s loop hands its own buffer over
/// at each call and takes each result back into the slot.
const WRITE_BACK: &str = "fn emit(mut out: Bytes, s: String) -> Unit = {\n  for b in bytes.to_list(bytes.from_string(s)) {\n    bytes.push(out, b)\n  }\n}\n\
     fn escape(mut out: Bytes, n: Int) -> Unit = {\n  var k = 0\n  while k < n {\n    match k % 3 {\n      0 => emit(out, \"&amp;\"),\n      1 => emit(out, \"&lt;\"),\n      _ => bytes.push(out, 65),\n    }\n    k = k + 1\n  }\n}\n\
     fn build(n: Int) -> Bytes = {\n  var out = bytes.zeros(0)\n  var i = 0\n  while i < n {\n    escape(out, i)\n    emit(out, \"!\")\n    i = i + 1\n  }\n  out\n}\n\
     fn main() -> Unit = println(bytes.to_string_lossy(build(4)))\n";

fn body<'a>(rust: &'a str, f: &str) -> &'a str {
    let at = rust.find(&format!("fn almd_{f}(")).unwrap_or_else(|| panic!("no `{f}`:\n{rust}"));
    let b = &rust[at..];
    &b[..b.find("\n}\n").unwrap_or(b.len())]
}

#[test]
fn a_rebound_mut_param_moves_and_every_call_hands_it_over() {
    let (rust, certs) = certified(WRITE_BACK);
    for f in ["emit", "escape"] {
        assert!(body(&rust, f).starts_with(&format!("fn almd_{f}(mut ")), "`{f}` does not take its buffer:\n{rust}");
    }
    for f in ["emit", "escape", "build"] {
        assert!(!body(&rust, f).contains(".clone()"), "`{f}` copies the buffer:\n{}", body(&rust, f));
    }
    let (_, _, calls) = certs.iter().find(|(p, ..)| p == "native-calls").expect("a native-calls witness");
    let moving = |b: &&str| b.split('|').next().is_some_and(|m| m.split(' ').any(|x| x == "1"));
    assert!(calls.split('#').filter(moving).count() >= 2, "{calls}");
    // `build` takes each call's result back into its slot as the share-out of
    // that result (`s` on the result's line, the slot's credit `i`), never as
    // an alias on the slot's own line after it moved out.
    let (_, _, build) = certs.iter().find(|(p, n, _)| p == "ownership" && n == "build").expect("build's witness");
    assert!(build.contains('s') && !build.contains("Ma"), "{build}");
}

const TREE: &str = "type Tree = | Leaf | Node(Tree, Int, Tree)\n\
     fn insert(t: Tree, x: Int) -> Tree =\n  match t {\n    Leaf => Node(Leaf, x, Leaf),\n    \
     Node(l, v, r) => if x < v then Node(insert(l, x), v, r) else Node(l, v, insert(r, x)),\n  }\n\
     fn depth(t: Tree) -> Int =\n  match t { Leaf => 0, Node(l, _, r) => 1 + (if depth(l) > depth(r) then depth(l) else depth(r)) }\n\
     fn go(t: Tree, i: Int) -> Tree = if i >= 9 then t else go(insert(t, (i * 5) % 9), i + 1)\n\
     fn main() -> Unit = println(int.to_string(depth(go(Leaf, 0))))\n";

#[test]
fn a_variant_param_whose_arm_stores_a_payload_on_every_path_is_taken_over() {
    // `insert` rebuilds the node around one payload and recurses into the
    // other on both paths of the arm that reads them: taken over, the stored
    // sibling moves out of the dying node where the borrowing body copied the
    // whole subtree at every level.
    let (rust, _) = certified(TREE);
    assert!(rust.contains("fn almd_insert(mut v0: AlmdRec0, v1: i64)"), "insert does not take its tree:\n{rust}");
    assert!(!body(&rust, "insert").contains(".clone()"), "insert copies a subtree:\n{rust}");
    assert!(!body(&rust, "go").contains("almd_insert(&"), "go lends the tree it hands on:\n{rust}");
}

#[test]
fn a_variant_param_whose_payloads_only_reach_reading_calls_keeps_borrowing() {
    // `depth` hands its payloads to itself alone: nothing stores them, so a
    // takeover would rest on its own assumption and copy a subtree for every
    // call that is not the payload's last.
    let (rust, _) = certified(TREE);
    assert!(rust.contains("fn almd_depth(v0: &AlmdRec0)"), "depth takes a tree it only reads:\n{rust}");
    assert!(!body(&rust, "depth").contains(".clone()"), "depth copies a subtree:\n{rust}");
}
