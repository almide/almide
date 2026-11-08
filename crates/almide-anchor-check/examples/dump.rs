// Probe only: print a defined function's instructions in a range.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&a[1]).unwrap();
    let pos: usize = a[2].parse().unwrap();
    let (from, to): (usize, usize) = (a[3].parse().unwrap(), a[4].parse().unwrap());
    let bodies = almide_anchor_check::decode(&bytes).unwrap();
    let b = &bodies[pos];
    for k in from..to.min(b.ops.len()) {
        println!("{k:5} d{} {:?}", b.depth[k], b.ops[k]);
    }
}
