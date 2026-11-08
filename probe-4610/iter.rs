fn main() {
    let s: i64 = (0..10_000_000i64).map(|x| x * 3).filter(|x| x % 2 == 0).sum();
    println!("{}", s);
}
