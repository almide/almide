// random extern — Rust native implementations (no external crate)
// Uses simple xorshift for deterministic-free randomness seeded from time

use std::cell::Cell;

thread_local! {
    static RNG_STATE: Cell<u64> = Cell::new({
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        t.as_nanos() as u64 ^ 0x517cc1b727220a95
    });
}

fn next_u64() -> u64 {
    RNG_STATE.with(|state| {
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

pub fn almide_rt_random_float() -> f64 {
    (next_u64() as f64) / (u64::MAX as f64)
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
