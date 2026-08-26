fn main() {
    let mut out: Vec<i64> = Vec::new();
    let mut i: i64 = 0;
    while i < 2000 {
        out.push(i);
        i += 1;
    }
    let mut acc: i64 = 0;
    let mut r: i64 = 0;
    while r < 2000 {
        let m: Vec<i64> = out.iter().map(|x| x * 3 + 1).collect();
        let f: Vec<i64> = m.into_iter().filter(|x| x % 2 == 0).collect();
        let v = f.iter().fold(0i64, |a, x| (a + x) % 999983);
        acc = (acc + v) % 999983;
        r += 1;
    }
    println!("{}", acc);
}
