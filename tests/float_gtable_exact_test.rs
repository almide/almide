//! The compact power-of-ten table of `float.to_string` (stdlib/float_to_string.almd,
//! `__sf_gtab`, #2099) reproduces Schubfach's EXACT 126-bit g(j) for every j the printer
//! can ask for — C-023.
//!
//! The printer reads g(j) = floor(10^j * 2^-r) + 1 (r = floor(log2 10^j) - 125) off a
//! 24-entry table: a 127-bit C(b) every 27 powers, times the exact 5^(j - b), shifted,
//! plus a stored correction bit. This test decodes the table from the stdlib source and
//! replays the printer's own 64-bit word arithmetic (wrapping mul, the unsigned high
//! product, the carry, the shifts) for every j, then checks the result against the
//! definition with big-integer arithmetic: T = g - 1 must satisfy T * 2^r <= 10^j <
//! (T + 1) * 2^r. The random and exponent sweeps of float_to_string_cross_target_test
//! reach a wrong low-order table bit only when a rounding decision happens to depend on
//! it; this test reaches every bit of every entry.

const SRC: &str = include_str!("../stdlib/float_to_string.almd");
const M63: u64 = (1 << 63) - 1;

fn flog10pow2(e: i64) -> i64 {
    (e * 661971961083) >> 41
}
fn flog10_3q_pow2(e: i64) -> i64 {
    (e * 661971961083 - 274743187321) >> 41
}
fn flog2pow10(e: i64) -> i64 {
    (e * 913124641741) >> 38
}

/// The bytes of the `__sf_gtab` string literal (`\xNN` escapes and plain ASCII).
fn table() -> Vec<u8> {
    let start = SRC.find("fn __sf_gtab() -> Int = prim.handle(\"").expect("__sf_gtab present")
        + "fn __sf_gtab() -> Int = prim.handle(\"".len();
    let end = start + SRC[start..].find('"').expect("literal closes");
    let lit = SRC[start..end].as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lit.len() {
        if lit[i] == b'\\' {
            assert_eq!(lit[i + 1], b'x', "only \\xNN escapes in the table literal");
            let hex = std::str::from_utf8(&lit[i + 2..i + 4]).unwrap();
            out.push(u8::from_str_radix(hex, 16).unwrap());
            i += 4;
        } else {
            out.push(lit[i]);
            i += 1;
        }
    }
    out
}

fn mulhi(a: u64, b: u64) -> u64 {
    ((a as u128 * b as u128) >> 64) as u64
}

/// n table bytes from p, 7 bits each, little-endian, top byte first — `__sf_rd`.
fn rd(t: &[u8], p: usize, n: usize) -> u64 {
    (0..n).rev().fold(0u64, |acc, i| (acc.wrapping_shl(7)) | t[p + i] as u64)
}

/// g(j) as (g1, g0), replaying `__sf_core`'s table read word for word.
fn g_from_table(t: &[u8], j: i64) -> (u64, u64) {
    let e = (j + 297) / 27;
    let f = j + 297 - e * 27;
    let p = (e * 23) as usize;
    let lo = rd(t, p, 9);
    let hi = rd(t, p + 9, 10);
    let c0 = lo | hi.wrapping_shl(63);
    let c1 = hi >> 1;
    let pf = 5u64.pow(f as u32);
    let l1 = c1.wrapping_mul(pf);
    let w1 = l1.wrapping_add(mulhi(c0, pf));
    let w2 = mulhi(c1, pf) + u64::from(w1 < l1);
    let w0 = c0.wrapping_mul(pf);
    let u = flog2pow10(j) - j - (flog2pow10(j - f) - (j - f)) + 1;
    assert!((1..=62).contains(&u), "j = {j}: shift {u} out of 1..=62");
    let u = u as u32;
    let tlo = (w1 << (64 - u)) | (w0 >> u);
    let thi = (w2 << (64 - u)) | (w1 >> u);
    let d = (t[p + 19 + (f / 7) as usize] as u64 >> (f % 7)) & 1;
    let g0x = (tlo & M63) + d + 1;
    let g1 = ((thi << 1) | (tlo >> 63)) + (g0x >> 63);
    (g1, g0x & M63)
}

/// Little-endian u32 limbs.
#[derive(Clone)]
struct Big(Vec<u32>);

impl Big {
    fn from_u128(mut v: u128) -> Big {
        let mut l = Vec::new();
        while v != 0 {
            l.push(v as u32);
            v >>= 32;
        }
        Big(l)
    }
    fn mul_small(&mut self, m: u32) {
        let mut carry = 0u64;
        for x in self.0.iter_mut() {
            let p = *x as u64 * m as u64 + carry;
            *x = p as u32;
            carry = p >> 32;
        }
        if carry != 0 {
            self.0.push(carry as u32);
        }
    }
    fn pow10(n: u32) -> Big {
        let mut b = Big(vec![1]);
        for _ in 0..n {
            b.mul_small(10);
        }
        b
    }
    fn shl(&self, n: u32) -> Big {
        let (ls, bs) = ((n / 32) as usize, n % 32);
        let mut l = vec![0u32; ls];
        let mut carry = 0u32;
        for &x in &self.0 {
            l.push(if bs == 0 { x } else { (x << bs) | carry });
            carry = if bs == 0 { 0 } else { x >> (32 - bs) };
        }
        l.push(carry);
        Big(l)
    }
    fn trimmed(&self) -> &[u32] {
        let mut n = self.0.len();
        while n > 0 && self.0[n - 1] == 0 {
            n -= 1;
        }
        &self.0[..n]
    }
    fn le(&self, o: &Big) -> bool {
        let (a, b) = (self.trimmed(), o.trimmed());
        if a.len() != b.len() {
            return a.len() < b.len();
        }
        for i in (0..a.len()).rev() {
            if a[i] != b[i] {
                return a[i] < b[i];
            }
        }
        true
    }
}

/// T * 2^r <= 10^j < (T + 1) * 2^r, both sides scaled to integers.
fn is_exact(t: u128, j: i64) -> bool {
    let r = flog2pow10(j) - 125;
    let scale = |x: u128| -> Big {
        // x * 2^r * 10^-j, times 2^max(-r, 0) * 10^max(-j, 0) on both sides
        let mut b = Big::from_u128(x);
        if j < 0 {
            for _ in 0..(-j) {
                b.mul_small(10);
            }
        }
        if r > 0 { b.shl(r as u32) } else { b }
    };
    let mut rhs = if j >= 0 { Big::pow10(j as u32) } else { Big(vec![1]) };
    if r < 0 {
        rhs = rhs.shl((-r) as u32);
    }
    let lo = scale(t);
    let hi = scale(t + 1);
    lo.le(&rhs) && !hi.le(&rhs)
}

#[test]
fn every_j_the_printer_asks_for_is_in_the_table_range() {
    let mut js: Vec<i64> = (-1074..=971)
        .flat_map(|q| [flog10pow2(q), flog10_3q_pow2(q)])
        .map(|k| -k)
        .collect();
    js.sort();
    js.dedup();
    assert_eq!(js.first(), Some(&-292));
    assert_eq!(js.last(), Some(&324));
    assert_eq!(js.len(), 617, "j is every integer in -292..=324");
}

#[test]
fn the_table_reproduces_the_exact_g_for_every_j() {
    let t = table();
    assert_eq!(t.len(), 24 * 23, "24 entries of 23 bytes");
    assert!(t.iter().all(|&b| b < 0x80), "every byte carries 7 bits");
    let mut bad = Vec::new();
    for j in -292..=324i64 {
        let (g1, g0) = g_from_table(&t, j);
        assert!(g1 < (1 << 63) && g0 < (1 << 63));
        let g = ((g1 as u128) << 63) | g0 as u128;
        if !is_exact(g - 1, j) {
            bad.push(j);
        }
    }
    assert!(bad.is_empty(), "g(j) is not floor(10^j * 2^-r) + 1 for j in {bad:?}");
}
