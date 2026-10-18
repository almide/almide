// fan floor probe: usage  floor <seq|par> <sweep|nest|busy> <iters> <offers>
use rayon::prelude::*;

const P: i64 = 1000003;

fn work(s: i64, i: i64, iters: i64) -> i64 {
    let mut acc = s + i;
    for j in 0..iters {
        acc = (acc * 31 + j) % P;
    }
    acc
}

fn fan_map(par: bool, n: i64, f: impl Fn(i64) -> i64 + Sync + Send) -> Vec<i64> {
    if par { (0..n).into_par_iter().map(f).collect() } else { (0..n).map(f).collect() }
}

fn step(par: bool, s: i64, iters: i64) -> Vec<i64> {
    fan_map(par, 10, |i| work(s, i, iters))
}

fn lane(par: bool, seed: i64, offers: i64, iters: i64) -> i64 {
    let mut s = seed;
    for _ in 0..offers {
        s = (s + step(par, s, iters).iter().sum::<i64>()) % P;
    }
    s
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let par = a[1] == "par";
    let iters: i64 = std::hint::black_box(a[3].parse().unwrap());
    let offers: i64 = a[4].parse().unwrap();
    match a[2].as_str() {
        "sweep" => println!("{}", lane(par, 1, offers, iters)),
        "nest" => {
            let mut s = 1;
            for _ in 0..offers {
                let ys = fan_map(par, 4, |q| step(par, s + q, iters).iter().fold(0, |a, x| (a + x) % P));
                s = (s + ys.iter().sum::<i64>()) % P;
            }
            println!("{s}");
        }
        _ => {
            let (x, y) = if par {
                rayon::join(|| lane(par, 1, offers, iters), || lane(par, 2, offers, iters))
            } else {
                (lane(par, 1, offers, iters), lane(par, 2, offers, iters))
            };
            println!("{x} {y}");
        }
    }
}
