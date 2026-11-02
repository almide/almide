// random extern — Rust native implementations (no external crate)
// int/float/choice/shuffle use a per-thread xorshift seeded from OS entropy
// (#4122; not for secrets); random.bytes reads the OS CSPRNG (getrandom / getentropy /
// BCryptGenRandom).


// The seed (#4122): std's `RandomState` draws its SipHash keys from the OS
// CSPRNG (getrandom / getentropy / BCryptGenRandom) once per THREAD, and that
// is the only std-only OS-entropy door without `unsafe` or a crate dependency
// (the generated program has neither). Hashing the thread's id through those
// keys keeps two threads apart even if their keys ever coincided. The former
// seed — SystemTime nanos — handed every `fan` arm started in the same clock
// tick the SAME stream. xorshift's all-zero state is a fixed point: `| 1`.
fn almide_rng_seed() -> u64 {
    use std::hash::{BuildHasher, Hash, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    std::thread::current().id().hash(&mut h);
    h.finish() | 1
}

thread_local! {
    static ALMIDE_RNG_STATE: std::cell::Cell<u64> = std::cell::Cell::new(almide_rng_seed());
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

// The span is counted in u64: `max - min + 1` overflows i64 once it passes
// 2^63 - 1, and the full range wraps it to 0 (a division by zero).
pub fn almide_rt_random_int(min: i64, max: i64) -> i64 {
    if min >= max { return min; }
    let span = (max as u64).wrapping_sub(min as u64).wrapping_add(1);
    let r = next_u64();
    if span == 0 { return r as i64; }
    min.wrapping_add((r % span) as i64)
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

pub fn almide_rt_random_choice<T: Clone>(xs: &[T]) -> Option<T> {
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

// `random.bytes(n)` (C-394): the one CSPRNG — n bytes from the operating
// system's cryptographic source, never the xorshift above, read through
// sys.rs's `fill_entropy` (getentropy(3) on unix, BCryptGenRandom on
// Windows; #4007 keeps every foreign call there). getentropy serves at most
// 256 bytes a call, so the fill goes in 256-byte chunks on every OS. A
// negative n is a contract violation (the ALS-T6 abort, the text the wasm
// self-host `random_bytes` dies with). A source that fails is an abort too:
// handing back fewer or predictable bytes would be silent.
#[cfg(any(unix, windows))]
fn almide_os_entropy(chunk: &mut [u8]) -> bool {
    almide_rt_sys_ffi::fill_entropy(chunk)
}

// No OS source known for this target: every draw aborts rather than return
// predictable bytes. tests/runtime_random_float_construction_test.rs runs the
// fill on each CI OS, so a host that lands here is a red test, not a silent one.
#[cfg(not(any(unix, windows)))]
fn almide_os_entropy(_chunk: &mut [u8]) -> bool {
    false
}

pub fn almide_rt_random_bytes(n: i64) -> Vec<u8> {
    if n < 0 {
        almide_abort(format!("random.bytes: negative length {n}"));
    }
    let mut v: Vec<u8> = Vec::new();
    if v.try_reserve_exact(n as usize).is_err() {
        almide_abort_oom();
    }
    v.resize(n as usize, 0);
    for chunk in v.chunks_mut(256) {
        if !almide_os_entropy(chunk) {
            almide_abort("random.bytes: the OS random source failed");
        }
    }
    v
}

// The 53-bit construction's boundary cells are EXECUTED, but not from here:
// tests/runtime_random_float_construction_test.rs `include!`s this file and
// asserts them under `cargo test --workspace`. That works because this module
// is self-contained (std alone); most of runtime/rs is
// not, which is why the general rule stays "the oracle is spec/" — see
// runtime/rs/README.md.
