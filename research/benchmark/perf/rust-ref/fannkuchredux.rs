// fannkuch-redux — the ORDINARY sequential Rust for the program: the
// Benchmarks-Game algorithm as a person writes it with `Vec<usize>` and plain
// loops, one thread, no `unsafe`, no SIMD, no rayon. Same output lines as
// fannkuchredux.almd (checksum, then `Pfannkuchen(n) = max_flips`).
//
// The Almide side splits the permutation space into n chunks and maps
// `process_chunk` over them; that map is a pure lambda, so the native leg's
// AutoParallel pass runs it on a thread per core. This reference is the honest
// single-thread comparison the #1330 claim is measured against — the win is
// the parallelism the effect system proves legal, and the ablation leg
// (`ALMIDE_AUTO_PARALLEL_OFF=1`, same source) shows the ratio without it.

fn fannkuch(n: usize) -> (i64, i64) {
    let mut perm1: Vec<usize> = (0..n).collect();
    let mut perm: Vec<usize> = vec![0; n];
    let mut count: Vec<usize> = vec![0; n];
    let mut max_flips: i64 = 0;
    let mut checksum: i64 = 0;
    let mut perm_count: i64 = 0;
    let mut r = n;
    loop {
        while r > 1 {
            count[r - 1] = r;
            r -= 1;
        }
        perm.copy_from_slice(&perm1);
        let mut flips: i64 = 0;
        let mut k = perm[0];
        while k != 0 {
            perm[..=k].reverse();
            flips += 1;
            k = perm[0];
        }
        if perm_count % 2 == 0 { checksum += flips } else { checksum -= flips }
        if flips > max_flips {
            max_flips = flips;
        }
        loop {
            if r == n {
                return (checksum, max_flips);
            }
            let p0 = perm1[0];
            for i in 0..r {
                perm1[i] = perm1[i + 1];
            }
            perm1[r] = p0;
            count[r] -= 1;
            if count[r] > 0 {
                break;
            }
            r += 1;
        }
        perm_count += 1;
    }
}

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(7);
    let (checksum, max_flips) = fannkuch(n);
    println!("{}", checksum);
    println!("Pfannkuchen({}) = {}", n, max_flips);
}
