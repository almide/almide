// binary-trees — same program shape as binarytrees.almd: a `Box`-linked
// recursive enum, `make` / `check` by value, the per-depth iteration loop run
// sequentially (Almide's `fan.map` is sequential on the native leg, so a
// single-threaded reference is the honest same-shape comparison). This is the
// Rust a person writes for the program; the Almide native leg beats it by
// running `check(make(d))` inside a region window (#1991) — the ratio is
// anchored BELOW 1 on purpose.

enum Tree {
    Leaf,
    Node(Box<Tree>, Box<Tree>),
}

fn make(depth: i64) -> Tree {
    if depth == 0 { Tree::Leaf } else { Tree::Node(Box::new(make(depth - 1)), Box::new(make(depth - 1))) }
}

fn check(tree: Tree) -> i64 {
    match tree {
        Tree::Leaf => 1,
        Tree::Node(left, right) => check(*left) + check(*right) + 1,
    }
}

fn check_trees(iterations: i64, depth: i64) -> i64 {
    let mut total = 0i64;
    for _ in 0..iterations {
        total += check(make(depth));
    }
    total
}

fn main() {
    let n: i64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let min_depth = 4;
    let max_depth = if n > min_depth + 2 { n } else { min_depth + 2 };
    let stretch_depth = max_depth + 1;
    println!("stretch tree of depth {}\t check: {}", stretch_depth, check(make(stretch_depth)));
    let long_lived = make(max_depth);
    let mut d = min_depth;
    while d <= max_depth {
        let iterations = 2i64.pow((max_depth - d + min_depth) as u32);
        let total = check_trees(iterations, d);
        println!("{}\t trees of depth {}\t check: {}", iterations, d, total);
        d += 2;
    }
    println!("long lived tree of depth {}\t check: {}", max_depth, check(long_lived));
}
