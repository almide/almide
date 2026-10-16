//! Integer `%` by a literal divisor (`x % 1000003`). rustc lowers `checked_rem`
//! by a constant to a magic-number multiply, but its sign fix sits on the
//! remainder's critical path (shift the quotient, add its sign bit, multiply
//! back), and it unrolls a loop around it with the induction offsets on that
//! path too. A remainder chain like `acc = (acc * 31 + j) % p` then runs ~9%
//! slower than Go's (docs/benchmarks/fan-floor.md). `almide_rem_const` takes
//! the sign from the dividend, beside the multiply, and multiplies with the
//! signed one-operand `imul` (see `almide_mulhs`). A 90M-step chain measured
//! (GCE, x86-64-v3, 2026-10-17) 290 ms against rustc's 366 ms and Go's 310 ms
//! on the Intel 8581C, and 370 ms against 455 ms and 400-420 ms on the AMD
//! EPYC 7B13.
//!
//! The body is ordinary Rust, compiled into the tests below and emitted
//! verbatim (with the prelude's visibility) into every generated program.

const BODY: &str = include_str!("prelude_rem_body.rs");

pub(crate) fn rem_const_prelude(vis: &str) -> String {
    BODY.replace("pub ", vis)
}

/// The divisors `almide_rem_const` is emitted for: `>= 3` and not a power of
/// two (a power of two is a shift and a mask, which rustc already emits).
pub(crate) fn rem_const_applies(d: i64) -> bool {
    d >= 3 && (d as u64).count_ones() != 1
}

#[cfg(test)]
mod tests {
    include!("prelude_rem_body.rs");

    fn check<const D: i64>(xs: &[i64]) {
        for &x in xs {
            assert_eq!(almide_rem_const::<D>(x), x % D, "{x} % {}", D);
        }
    }

    #[test]
    fn the_constant_remainder_is_the_truncated_remainder() {
        let mut xs = vec![0, 1, -1, 2, -2, i64::MAX, i64::MIN, i64::MIN + 1, i64::MAX - 1];
        let mut s: u64 = 0x9e3779b97f4a7c15;
        for _ in 0..20000 {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            xs.push(s as i64);
            xs.push((s >> (s % 63)) as i64);
            xs.push(-((s >> (s % 63)) as i64));
        }
        for d in [3i64, 5, 6, 7, 10, 1000003, 1_000_000_007, 641, 6700417] {
            for k in -3i64..=3 {
                xs.push(d.wrapping_mul(k));
                xs.push(d.wrapping_mul(k) + 1);
                xs.push(d.wrapping_mul(k) - 1);
            }
        }
        check::<3>(&xs);
        check::<5>(&xs);
        check::<6>(&xs);
        check::<7>(&xs);
        check::<10>(&xs);
        check::<12>(&xs);
        check::<31>(&xs);
        check::<641>(&xs);
        check::<1000>(&xs);
        check::<1000003>(&xs);
        check::<1_000_000_007>(&xs);
        check::<6700417>(&xs);
        check::<{ i64::MAX }>(&xs);
        check::<{ i64::MAX - 1 }>(&xs);
        check::<{ (1i64 << 62) + 1 }>(&xs);
        check::<{ (1i64 << 62) - 1 }>(&xs);
        check::<{ (1i64 << 40) + 3 }>(&xs);
    }

    #[test]
    fn every_small_divisor_has_an_exact_magic() {
        // The magic is exact for every dividend iff it is exact at the
        // dividends nearest the multiples of d; sweep those for small d.
        for d in 3i64..2000 {
            if !super::rem_const_applies(d) { continue; }
            let (m, s) = almide_rem_magic(d);
            let rem = |x: i64| {
                let hi = almide_mulhs(x, m);
                let t = if m < 0 { hi.wrapping_add(x) } else { hi };
                x.wrapping_sub((x >> 63) & d).wrapping_sub((t >> s).wrapping_mul(d))
            };
            for x in [i64::MIN, i64::MIN + 1, i64::MAX, i64::MAX - 1, 0, 1, -1] {
                assert_eq!(rem(x), x % d, "{x} % {d}");
            }
            for k in [1i64, 2, 3, 1 << 20, 1 << 40, i64::MAX / d] {
                for x in [k * d - 1, k * d, k * d + 1, -(k * d) - 1, -(k * d), -(k * d) + 1] {
                    assert_eq!(rem(x), x % d, "{x} % {d}");
                }
            }
        }
    }
}
