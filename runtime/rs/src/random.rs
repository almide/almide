// random extern — Rust native implementations (no external crate)
// Uses simple xorshift for deterministic-free randomness seeded from time

use std::cell::Cell;

thread_local! {
    static ALMIDE_RNG_STATE: Cell<u64> = Cell::new({
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        t.as_nanos() as u64 ^ 0x517cc1b727220a95
    });
}

fn next_u64() -> u64 {
    ALMIDE_RNG_STATE.with(|state| {
        let mut s = state.get();
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        state.set(s);
        s
    })
}

pub fn almide_rt_random_int(min: i64, max: i64) -> i64 {
    if min >= max { return min; }
    let range = (max - min + 1) as u64;
    min + (next_u64() % range) as i64
}

// The 53-bit construction: the top 53 bits of the draw (xorshift's low bits
// are its weakest) scaled by 2^-53 — every value is in [0, 1) exactly, the same
// range the self-host builds from WASI entropy (stdlib/random_float.almd).
// `next_u64() as f64 / u64::MAX as f64` was NOT: the ~1024 largest u64 values
// round to 2^64 as a double, and each of them divided by the same rounded
// denominator is exactly 1.0 (#2495).
fn almide_float_from_u64(r: u64) -> f64 {
    (r >> 11) as f64 * (1.0 / 9007199254740992.0)
}

pub fn almide_rt_random_float() -> f64 {
    almide_float_from_u64(next_u64())
}

pub fn almide_rt_random_choice<T: Clone>(xs: &Vec<T>) -> Option<T> {
    if xs.is_empty() { return None; }
    let idx = (next_u64() as usize) % xs.len();
    Some(xs[idx].clone())
}

pub fn almide_rt_random_shuffle<T: Clone>(mut xs: Vec<T>) -> Vec<T> {
    let n = xs.len();
    for i in (1..n).rev() {
        let j = (next_u64() as usize) % (i + 1);
        xs.swap(i, j);
    }
    xs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_construction_never_reaches_one() {
        // The top of the u64 range is where the old quotient rounded to 1.0.
        for r in [u64::MAX, u64::MAX - 1, u64::MAX - 1023, u64::MAX - 1024, 1u64 << 63] {
            let f = almide_float_from_u64(r);
            assert!((0.0..1.0).contains(&f), "{r} -> {f}");
        }
        assert_eq!(almide_float_from_u64(0), 0.0);
        assert_eq!(almide_float_from_u64(u64::MAX), 1.0 - f64::EPSILON / 2.0);
        for _ in 0..100_000 {
            let f = almide_rt_random_float();
            assert!((0.0..1.0).contains(&f), "{f}");
        }
    }
}
