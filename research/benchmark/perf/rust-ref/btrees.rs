// The same-shape `Box` program for btrees.almd: a Box per node, a free per
// node, the depth a literal. The argument bench.py passes is ignored, as on the
// Almide side.
enum Tree { Leaf, Node(Box<Tree>, Box<Tree>) }
fn make(d: i64) -> Tree { if d == 0 { Tree::Leaf } else { Tree::Node(Box::new(make(d-1)), Box::new(make(d-1))) } }
fn check(t: &Tree) -> i64 { match t { Tree::Leaf => 1, Tree::Node(l, r) => check(l) + check(r) + 1 } }
fn main() { let mut total = 0i64; for _ in 0..12 { total += check(&make(19)); } println!("{}", total); }
