fn go(n: i64, acc: i64) -> i64 {
    if n <= 0 { acc } else { go(n - 1, (acc + n) % 999983) }
}

fn main() {
    let mut r: i64 = 0;
    let mut acc: i64 = 0;
    while r < 30 {
        acc = (acc + go(1000000, 0)) % 999983;
        r += 1;
    }
    println!("{}", acc);
}
