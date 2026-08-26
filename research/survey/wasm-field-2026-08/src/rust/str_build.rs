fn main() {
    let mut i: i64 = 0;
    let mut n: i64 = 0;
    while i < 3000000 {
        let s = "ab".to_string() + &(i % 100).to_string();
        n += s.len() as i64;
        i += 1;
    }
    println!("{}", n);
}
