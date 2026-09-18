// treealloc — same program shape as treealloc.almd: a `Box`-linked recursive
// enum, `make` / `check` by value, twelve build-then-walk rounds of one depth,
// one thread. This is the ORDINARY Rust for the program — no arena, no
// `unsafe`, no custom allocator: a `Box` per node in, a free per node out.
// The Almide native leg runs each `check(make(d))` inside a region window
// (#1991), so the row sits below 1 (#1330); `ALMIDE_REGION_OFF=1` is the
// same-source ablation that shows the ratio without it.

enum Tree {
    Leaf,
    Node(Box<Tree>, Box<Tree>),
}

fn make(depth: i64) -> Tree {
    if depth == 0 { Tree::Leaf } else { Tree::Node(Box::new(make(depth - 1)), Box::new(make(depth - 1))) }
}

fn check(t: Tree) -> i64 {
    match t {
        Tree::Leaf => 1,
        Tree::Node(l, r) => check(*l) + check(*r) + 1,
    }
}

fn main() {
    let depth: i64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(19);
    let mut total = 0i64;
    let mut i = 0;
    while i < 12 {
        total += check(make(depth));
        i += 1;
    }
    println!("{}", total);
}
