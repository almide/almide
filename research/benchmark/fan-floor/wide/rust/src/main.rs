// fan floor, wide probes: usage  wide <seq|rayon|tuned|scope> <size|skew|heap|fail> <n> <iters> <reps>
// Every implementation runs every element and keeps the lowest-index error
// (Almide's fan.map rule), so the work and the error policy are the same.
use rayon::prelude::*;

const P: i64 = 1000003;

fn work(s: i64, i: i64, iters: i64) -> i64 {
    let mut acc = s + i;
    for j in 0..iters {
        acc = (acc * 31 + j) % P;
    }
    acc
}

// One element of `kind`: Ok(value) or Err(the error's length, as Almide's
// `string.len("bad ${i}")`).
fn elem(kind: &str, s: i64, i: i64, iters: i64) -> Result<i64, i64> {
    match kind {
        "size" => Ok(work(s, i, iters)),
        "skew" => Ok(work(s, i, if i % 10 == 0 { iters * 20 } else { iters })),
        "heap" => {
            let parts: Vec<String> = (0..20).map(|j| ((s + i) * j % 1000).to_string()).collect();
            Ok(parts.join(",").len() as i64)
        }
        _ => {
            if i % 97 == 13 { Err(format!("bad {i}").len() as i64) } else { Ok(work(s, i, iters)) }
        }
    }
}

fn fold(out: Vec<Result<i64, i64>>, s: i64) -> i64 {
    // the lowest-index Err, else the sum of the values
    if let Some(Err(e)) = out.iter().find(|r| r.is_err()) {
        return (s + e) % P;
    }
    (s + out.into_iter().map(|r| r.unwrap()).fold(0, |a, x| (a + x) % P)) % P
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (imp, kind) = (a[1].as_str(), a[2].as_str());
    let n: i64 = std::hint::black_box(a[3].parse().unwrap());
    let iters: i64 = std::hint::black_box(a[4].parse().unwrap());
    let reps: i64 = a[5].parse().unwrap();
    let threads = rayon::current_num_threads();
    let mut s = 1i64;
    for _ in 0..reps {
        let s0 = s;
        let f = |i: i64| elem(kind, s0, i, iters);
        let out: Vec<Result<i64, i64>> = match imp {
            "seq" => (0..n).map(f).collect(),
            "rayon" => (0..n as usize).into_par_iter().map(|i| i as i64).map(f).collect(),
            // tuned: no split below n / (4 threads), so a small element is
            // never a task of its own
            "tuned" => (0..n as usize).into_par_iter().map(|i| i as i64).with_min_len(((n as usize) / (threads * 4)).max(1)).map(f).collect(),
            // scope: one task per thread over a contiguous chunk
            _ => {
                let mut out: Vec<Result<i64, i64>> = vec![Ok(0); n as usize];
                let chunk = (n as usize).div_ceil(threads).max(1);
                rayon::scope(|sc| {
                    for (c, part) in out.chunks_mut(chunk).enumerate() {
                        let f = &f;
                        sc.spawn(move |_| {
                            for (k, slot) in part.iter_mut().enumerate() {
                                *slot = f((c * chunk + k) as i64);
                            }
                        });
                    }
                });
                out
            }
        };
        s = fold(out, s);
    }
    println!("{s}");
}
