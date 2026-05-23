use std::time::Instant;
use std::collections::HashMap;

fn bench(name: &str, f: impl FnOnce()) {
    let s = Instant::now();
    f();
    let us = s.elapsed().as_micros();
    println!("  {}: {}.{}ms", name, us / 1000, (us % 1000) / 100);
}

fn fib(n: i64) -> i64 {
    if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}

fn main() {
    println!("=== Rust WASM ===");

    bench("fib35", || { std::hint::black_box(fib(35)); });

    let rev_data: Vec<i64> = (0..100000).rev().collect();

    bench("sort_100k", || {
        let mut v = rev_data.clone();
        v.sort();
        std::hint::black_box(v);
    });

    let data: Vec<i64> = (0..100000).collect();

    bench("list_map_100k", || {
        let v: Vec<i64> = data.iter().map(|x| x * 2).collect();
        std::hint::black_box(v);
    });

    bench("list_filter_100k", || {
        let v: Vec<i64> = data.iter().filter(|x| **x % 2 == 0).copied().collect();
        std::hint::black_box(v);
    });

    bench("list_fold_100k", || {
        let r = data.iter().fold(0i64, |a, x| a + x);
        std::hint::black_box(r);
    });

    bench("str_concat_10k", || {
        let mut s = String::new();
        for _ in 0..10000 { s.push('x'); }
        std::hint::black_box(s);
    });

    bench("map_insert_10k", || {
        let mut m = HashMap::new();
        for i in 0..10000i64 { m.insert(i, i * 2); }
        std::hint::black_box(m);
    });

    bench("map_get_10k", || {
        let mut m = HashMap::new();
        for i in 0..10000i64 { m.insert(i, i); }
        let mut s = 0i64;
        for j in 0..10000i64 { s += m.get(&j).copied().unwrap_or(0); }
        std::hint::black_box(s);
    });

    bench("int_parse_100k", || {
        for _ in 0..100000 { std::hint::black_box("12345".parse::<i64>().unwrap()); }
    });

    bench("int_tostring_100k", || {
        for i in 0..100000i64 { std::hint::black_box(i.to_string()); }
    });

    bench("math_sqrt_100k", || {
        for i in 0..100000 { std::hint::black_box((i as f64).sqrt()); }
    });
}
