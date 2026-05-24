use std::time::Instant;
use std::collections::HashMap;
fn bench(name: &str, f: impl FnOnce()) {
    let s = Instant::now();
    f();
    let us = s.elapsed().as_micros();
    println!("  {}: {}.{}ms", name, us / 1000, (us % 1000) / 100);
}
fn fib(n: i64) -> i64 { if n <= 1 { n } else { fib(n - 1) + fib(n - 2) } }
fn main() {
    let data: Vec<i64> = (0..1000000).collect();
    let rev_data: Vec<i64> = (0..1000000).rev().collect();

    bench("fib38", || { std::hint::black_box(fib(38)); });
    bench("sort_1M", || { let mut v = rev_data.clone(); v.sort(); std::hint::black_box(v); });
    bench("list_map_1M", || { let v: Vec<i64> = data.iter().map(|x| x * 2).collect(); std::hint::black_box(v); });
    bench("list_filter_1M", || { let v: Vec<i64> = data.iter().filter(|x| **x % 2 == 0).copied().collect(); std::hint::black_box(v); });
    bench("list_fold_1M", || { let r = data.iter().fold(0i64, |a, x| a + x); std::hint::black_box(r); });
    bench("str_concat_1M", || { let mut s = String::new(); for _ in 0..1000000 { s.push('x'); } std::hint::black_box(s); });
    bench("map_insert_100k", || { let mut m = HashMap::new(); for i in 0..100000i64 { m.insert(i, i * 2); } std::hint::black_box(m); });
    bench("map_get_100k", || { let mut m = HashMap::new(); for i in 0..100000i64 { m.insert(i, i); } let mut s = 0i64; for j in 0..100000i64 { s += m.get(&j).copied().unwrap_or(0); } std::hint::black_box(s); });
    bench("int_parse_1M", || { let mut sum = 0i64; for i in 0..1000000i64 { sum += i.to_string().parse::<i64>().unwrap(); } std::hint::black_box(sum); });
    bench("int_tostring_1M", || { for i in 0..1000000i64 { std::hint::black_box(i.to_string()); } });
    bench("math_sqrt_1M", || { for i in 0..1000000 { std::hint::black_box((i as f64).sqrt()); } });
}
