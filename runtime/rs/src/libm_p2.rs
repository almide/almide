// ─────────────────────── kernel sin/cos/tan ────────────────────
// origin: FreeBSD /usr/src/lib/msun/src/k_sin.c, k_cos.c, k_tan.c
// ====================================================
// Copyright (C) 1993, 2004 by Sun Microsystems, Inc. All rights reserved.
// ====================================================

const S1: f64 = -1.66666666666666324348e-01; /* 0xBFC55555, 0x55555549 */
const S2: f64 = 8.33333333332248946124e-03; /* 0x3F811111, 0x1110F8A6 */
const S3: f64 = -1.98412698298579493134e-04; /* 0xBF2A01A0, 0x19C161D5 */
const S4: f64 = 2.75573137070700676789e-06; /* 0x3EC71DE3, 0x57B1FE7D */
const S5: f64 = -2.50507602534068634195e-08; /* 0xBE5AE5E6, 0x8A2B9CEB */
const S6: f64 = 1.58969099521155010221e-10; /* 0x3DE5D93A, 0x5ACFD57C */

/// kernel sin on ~[-pi/4, pi/4]. y is the tail of x; iy != 0 means y is used.
fn k_sin(x: f64, y: f64, iy: i32) -> f64 {
    let z = x * x;
    let w = z * z;
    let r = S2 + z * (S3 + z * S4) + z * w * (S5 + z * S6);
    let v = z * x;
    if iy == 0 {
        x + v * (S1 + z * r)
    } else {
        x - ((z * (0.5 * y - v * r) - y) - v * S1)
    }
}

const C1: f64 = 4.16666666666666019037e-02; /* 0x3FA55555, 0x5555554C */
const C2: f64 = -1.38888888888741095749e-03; /* 0xBF56C16C, 0x16C15177 */
const C3: f64 = 2.48015872894767294178e-05; /* 0x3EFA01A0, 0x19CB1590 */
const C4: f64 = -2.75573143513906633035e-07; /* 0xBE927E4F, 0x809C52AD */
const C5: f64 = 2.08757232129817482790e-09; /* 0x3E21EE9E, 0xBDB4B1C4 */
const C6: f64 = -1.13596475577881948265e-11; /* 0xBDA8FAE9, 0xBE8838D4 */

/// kernel cos on [-pi/4, pi/4]. y is the tail of x.
fn k_cos(x: f64, y: f64) -> f64 {
    let z = x * x;
    let w = z * z;
    let r = z * (C1 + z * (C2 + z * C3)) + w * w * (C4 + z * (C5 + z * C6));
    let hz = 0.5 * z;
    let w = 1.0 - hz;
    w + (((1.0 - w) - hz) + (z * r - x * y))
}

static T: [f64; 13] = [
    3.33333333333334091986e-01,  /* 3FD55555, 55555563 */
    1.33333333333201242699e-01,  /* 3FC11111, 1110FE7A */
    5.39682539762260521377e-02,  /* 3FABA1BA, 1BB341FE */
    2.18694882948595424599e-02,  /* 3F9664F4, 8406D637 */
    8.86323982359930005737e-03,  /* 3F8226E3, E96E8493 */
    3.59207910759131235356e-03,  /* 3F6D6D22, C9560328 */
    1.45620945432529025516e-03,  /* 3F57DBC8, FEE08315 */
    5.88041240820264096874e-04,  /* 3F4344D8, F2F26501 */
    2.46463134818469906812e-04,  /* 3F3026F7, 1A8D1068 */
    7.81794442939557092300e-05,  /* 3F147E88, A03792A6 */
    7.14072491382608190305e-05,  /* 3F12B80F, 32F0A7E9 */
    -1.85586374855275456654e-05, /* BEF375CB, DB605373 */
    2.59073051863633712884e-05,  /* 3EFB2A70, 74BF7AD4 */
];
const PIO4: f64 = 7.85398163397448278999e-01; /* 3FE921FB, 54442D18 */
const PIO4_LO: f64 = 3.06161699786838301793e-17; /* 3C81A626, 33145C07 */

#[inline]
fn zero_low_word(x: f64) -> f64 {
    f64::from_bits(f64::to_bits(x) & 0xFFFF_FFFF_0000_0000)
}

/// kernel tan on ~[-pi/4, pi/4]. odd != 0 returns -1/tan, else tan.
fn k_tan(mut x: f64, mut y: f64, odd: i32) -> f64 {
    let hx = (f64::to_bits(x) >> 32) as u32;
    let big = (hx & 0x7fffffff) >= 0x3FE59428; /* |x| >= 0.6744 */
    if big {
        let sign = hx >> 31;
        if sign != 0 {
            x = -x;
            y = -y;
        }
        x = (PIO4 - x) + (PIO4_LO - y);
        y = 0.0;
    }
    let z = x * x;
    let w = z * z;
    let r = T[1] + w * (T[3] + w * (T[5] + w * (T[7] + w * (T[9] + w * T[11]))));
    let v = z * (T[2] + w * (T[4] + w * (T[6] + w * (T[8] + w * (T[10] + w * T[12])))));
    let s = z * x;
    let r = y + z * (s * (r + v) + y) + s * T[0];
    let w = x + r;
    if big {
        let sign = hx >> 31;
        let s = 1.0 - 2.0 * odd as f64;
        let v = s - 2.0 * (x + (r - w * w / (w + s)));
        return if sign != 0 { -v } else { v };
    }
    if odd == 0 {
        return w;
    }
    /* -1.0/(x+r) has up to 2ulp error, so compute it accurately */
    let w0 = zero_low_word(w);
    let v = r - (w0 - x); /* w0+v = r+x */
    let a = -1.0 / w;
    let a0 = zero_low_word(a);
    a0 + a * (1.0 + a0 * w0 + a0 * v)
}

// ───────────────────────── sin / cos / tan ─────────────────────
// origin: FreeBSD /usr/src/lib/msun/src/s_sin.c, s_cos.c, s_tan.c

/// `sin(x)` — bit-identical reference (vendored libm 0.2.16).
fn sin(x: f64) -> f64 {
    let x1p120 = f64::from_bits(0x4770000000000000); // 2^120

    let ix = (f64::to_bits(x) >> 32) as u32 & 0x7fffffff;

    /* |x| ~< pi/4 */
    if ix <= 0x3fe921fb {
        if ix < 0x3e500000 {
            /* |x| < 2**-26: raise inexact/underflow, return x */
            if ix < 0x00100000 {
                let _ = x / x1p120;
            } else {
                let _ = x + x1p120;
            }
            return x;
        }
        return k_sin(x, 0.0, 0);
    }

    /* sin(Inf or NaN) is NaN */
    if ix >= 0x7ff00000 {
        return x - x;
    }

    /* argument reduction needed */
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => k_sin(y0, y1, 1),
        1 => k_cos(y0, y1),
        2 => -k_sin(y0, y1, 1),
        _ => -k_cos(y0, y1),
    }
}

/// `cos(x)` — bit-identical reference (vendored libm 0.2.16).
fn cos(x: f64) -> f64 {
    let ix = (f64::to_bits(x) >> 32) as u32 & 0x7fffffff;

    /* |x| ~< pi/4 */
    if ix <= 0x3fe921fb {
        if ix < 0x3e46a09e {
            /* if x < 2**-27 * sqrt(2): return 1 (inexact if x != 0) */
            if x as i32 == 0 {
                return 1.0;
            }
        }
        return k_cos(x, 0.0);
    }

    /* cos(Inf or NaN) is NaN */
    if ix >= 0x7ff00000 {
        return x - x;
    }

    /* argument reduction needed */
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => k_cos(y0, y1),
        1 => -k_sin(y0, y1, 1),
        2 => -k_cos(y0, y1),
        _ => k_sin(y0, y1, 1),
    }
}

/// `tan(x)` — bit-identical reference (vendored libm 0.2.16).
fn tan(x: f64) -> f64 {
    let x1p120 = f32::from_bits(0x7b800000); // 2^120

    let ix = (f64::to_bits(x) >> 32) as u32 & 0x7fffffff;
    /* |x| ~< pi/4 */
    if ix <= 0x3fe921fb {
        if ix < 0x3e400000 {
            /* |x| < 2**-27: raise inexact/underflow, return x */
            if ix < 0x00100000 {
                let _ = x / x1p120 as f64;
            } else {
                let _ = x + x1p120 as f64;
            }
            return x;
        }
        return k_tan(x, 0.0, 0);
    }

    /* tan(Inf or NaN) is NaN */
    if ix >= 0x7ff00000 {
        return x - x;
    }

    /* argument reduction */
    let (n, y0, y1) = rem_pio2(x);
    k_tan(y0, y1, n & 1)
}

// ───────────────────── word-manipulation helpers ───────────────
// libm's `get_high_word`/`with_set_high_word`/`with_set_low_word` (the `support`
// module). Trivial bit splits; named here so `pow` reads like the upstream.

/// High 32 bits of `x`'s IEEE-754 encoding.
#[inline]
fn get_high_word(x: f64) -> u32 {
    (x.to_bits() >> 32) as u32
}
/// `x` with its high 32 bits replaced by `hi`.
#[inline]
fn with_set_high_word(x: f64, hi: u32) -> f64 {
    f64::from_bits((x.to_bits() & 0x0000_0000_ffff_ffff) | ((hi as u64) << 32))
}
/// `x` with its low 32 bits replaced by `lo`.
#[inline]
fn with_set_low_word(x: f64, lo: u32) -> f64 {
    f64::from_bits((x.to_bits() & 0xffff_ffff_0000_0000) | (lo as u64))
}

// ───────────────────────────── exp ─────────────────────────────
// origin: FreeBSD /usr/src/lib/msun/src/e_exp.c
// ====================================================
// Copyright (C) 2004 by Sun Microsystems, Inc. All rights reserved.
// ====================================================

const EXP_HALF: [f64; 2] = [0.5, -0.5];
const LN2HI: f64 = 6.93147180369123816490e-01; /* 0x3fe62e42, 0xfee00000 */
const LN2LO: f64 = 1.90821492927058770002e-10; /* 0x3dea39ef, 0x35793c76 */
const EXP_INVLN2: f64 = 1.44269504088896338700e+00; /* 0x3ff71547, 0x652b82fe */
const EXP_P1: f64 = 1.66666666666666019037e-01; /* 0x3FC55555, 0x5555553E */
const EXP_P2: f64 = -2.77777777770155933842e-03; /* 0xBF66C16C, 0x16BEBD93 */
const EXP_P3: f64 = 6.61375632143793436117e-05; /* 0x3F11566A, 0xAF25DE2C */
const EXP_P4: f64 = -1.65339022054652515390e-06; /* 0xBEBBBD41, 0xC5D26BF1 */
const EXP_P5: f64 = 4.13813679705723846039e-08; /* 0x3E663769, 0x72BEA4D0 */

/// `exp(x)` — bit-identical reference (vendored libm 0.2.16).
fn exp(mut x: f64) -> f64 {
    let x1p1023 = f64::from_bits(0x7fe0000000000000); // 2^1023
    let x1p_149 = f64::from_bits(0x36a0000000000000); // 2^-149

    let hi: f64;
    let lo: f64;
    let c: f64;
    let xx: f64;
    let y: f64;
    let k: i32;
    let sign: i32;
    let mut hx: u32;

    hx = (x.to_bits() >> 32) as u32;
    sign = (hx >> 31) as i32;
    hx &= 0x7fffffff; /* high word of |x| */

    /* special cases */
    if hx >= 0x4086232b {
        /* if |x| >= 708.39... */
        if x.is_nan() {
            return x;
        }
        if x > 709.782712893383973096 {
            /* overflow if x!=inf */
            x *= x1p1023;
            return x;
        }
        if x < -708.39641853226410622 {
            /* underflow if x!=-inf */
            // force_eval of (-x1p_149 / x) as f32 — only sets the FP underflow
            // flag in C; the value is unused and unobservable. Kept faithful.
            let _ = (-x1p_149 / x) as f32;
            if x < -745.13321910194110842 {
                return 0.;
            }
        }
    }

    /* argument reduction */
    if hx > 0x3fd62e42 {
        /* if |x| > 0.5 ln2 */
        if hx >= 0x3ff0a2b2 {
            /* if |x| >= 1.5 ln2 */
            k = (EXP_INVLN2 * x + EXP_HALF[sign as usize]) as i32;
        } else {
            k = 1 - sign - sign;
        }
        hi = x - k as f64 * LN2HI; /* k*ln2hi is exact here */
        lo = k as f64 * LN2LO;
        x = hi - lo;
    } else if hx > 0x3e300000 {
        /* if |x| > 2**-28 */
        k = 0;
        hi = x;
        lo = 0.;
    } else {
        /* inexact if x!=0 */
        let _ = x1p1023 + x;
        return 1. + x;
    }

    /* x is now in primary range */
    xx = x * x;
    c = x - xx * (EXP_P1 + xx * (EXP_P2 + xx * (EXP_P3 + xx * (EXP_P4 + xx * EXP_P5))));
    y = 1. + (x * c / (2. - c) - lo + hi);
    if k == 0 {
        y
    } else {
        vscalbn(y, k)
    }
}
