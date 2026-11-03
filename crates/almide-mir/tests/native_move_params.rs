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
