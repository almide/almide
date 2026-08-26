fn mk(n: i64) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    let mut i: i64 = 0;
    while i < n {
        out.push((i * 7919) % 10007);
        i += 1;
    }
    out
}

fn main() {
    let xs = mk(2000);
    let mut acc: i64 = 0;
    let mut r: i64 = 0;
    while r < 300 {
        let mut s = xs.clone();
        s.sort();
        acc = acc + s[0] + s[1999];
        r += 1;
    }
    println!("{}", acc);
}
