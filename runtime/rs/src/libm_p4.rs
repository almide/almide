// ────────────────── atan / expm1 / tanh ──────────────────
// origin: FreeBSD s_atan.c / s_expm1.c / s_tanh.c (vendored libm 0.2.16)
// ====================================================
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
// ====================================================
// Same discipline as the earlier vendored fns: identical coefficients, branch
// structure, and bit manipulations; the upstream `i!`/`force_eval!` macros are
// replaced by plain indexing / dropped (force_eval only raises the FP
// underflow flag, which the runtime does not observe).

const ATANHI: [f64; 4] = [
    4.63647609000806093515e-01, /* atan(0.5)hi 0x3FDDAC67, 0x0561BB4F */
    7.85398163397448278999e-01, /* atan(1.0)hi 0x3FE921FB, 0x54442D18 */
    9.82793723247329054082e-01, /* atan(1.5)hi 0x3FEF730B, 0xD281F69B */
    1.57079632679489655800e+00, /* atan(inf)hi 0x3FF921FB, 0x54442D18 */
];

const ATANLO: [f64; 4] = [
    2.26987774529616870924e-17, /* atan(0.5)lo 0x3C7A2B7F, 0x222F65E2 */
    3.06161699786838301793e-17, /* atan(1.0)lo 0x3C81A626, 0x33145C07 */
    1.39033110312309984516e-17, /* atan(1.5)lo 0x3C700788, 0x7AF0CBBD */
    6.12323399573676603587e-17, /* atan(inf)lo 0x3C91A626, 0x33145C07 */
];

const AT: [f64; 11] = [
    3.33333333333329318027e-01,  /* 0x3FD55555, 0x5555550D */
    -1.99999999998764832476e-01, /* 0xBFC99999, 0x9998EBC4 */
    1.42857142725034663711e-01,  /* 0x3FC24924, 0x920083FF */
    -1.11111104054623557880e-01, /* 0xBFBC71C6, 0xFE231671 */
    9.09088713343650656196e-02,  /* 0x3FB745CD, 0xC54C206E */
    -7.69187620504482999495e-02, /* 0xBFB3B0F2, 0xAF749A6D */
    6.66107313738753120669e-02,  /* 0x3FB10D66, 0xA0D03D51 */
    -5.83357013379057348645e-02, /* 0xBFADDE2D, 0x52DEFD9A */
    4.97687799461593236017e-02,  /* 0x3FA97B4B, 0x24760DEB */
    -3.65315727442169155270e-02, /* 0xBFA2B444, 0x2C6A6C2F */
    1.62858201153657823623e-02,  /* 0x3F90AD3A, 0xE322DA11 */
];

/// `atan(x)` — bit-identical reference (vendored libm 0.2.16).
fn atan(x: f64) -> f64 {
    let mut x = x;
    let mut ix = (x.to_bits() >> 32) as u32;
    let sign = ix >> 31;
    ix &= 0x7fff_ffff;
    if ix >= 0x4410_0000 {
        if x.is_nan() {
            return x;
        }
        let z = ATANHI[3] + f64::from_bits(0x0380_0000); // 0x1p-120f
        return if sign != 0 { -z } else { z };
    }

    let id = if ix < 0x3fdc_0000 {
        /* |x| < 0.4375 */
        if ix < 0x3e40_0000 {
            /* |x| < 2^-27: return x (upstream force_eval only raises underflow) */
            return x;
        }
        -1
    } else {
        x = x.abs();
        if ix < 0x3ff30000 {
            /* |x| < 1.1875 */
            if ix < 0x3fe60000 {
                /* 7/16 <= |x| < 11/16 */
                x = (2. * x - 1.) / (2. + x);
                0
            } else {
                /* 11/16 <= |x| < 19/16 */
                x = (x - 1.) / (x + 1.);
                1
            }
        } else if ix < 0x40038000 {
            /* |x| < 2.4375 */
            x = (x - 1.5) / (1. + 1.5 * x);
            2
        } else {
            /* 2.4375 <= |x| < 2^66 */
            x = -1. / x;
            3
        }
    };

    let z = x * x;
    let w = z * z;
    /* break sum from i=0 to 10 AT[i]z**(i+1) into odd and even poly */
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));

    if id < 0 {
        return x - x * (s1 + s2);
    }

    let z = ATANHI[id as usize] - (x * (s1 + s2) - ATANLO[id as usize] - x);

    if sign != 0 { -z } else { z }
}

const EXPM1_O_THRESHOLD: f64 = 7.09782712893383973096e+02; /* 0x40862E42, 0xFEFA39EF */
const EXPM1_LN2_HI: f64 = 6.93147180369123816490e-01; /* 0x3fe62e42, 0xfee00000 */
const EXPM1_LN2_LO: f64 = 1.90821492927058770002e-10; /* 0x3dea39ef, 0x35793c76 */
const EXPM1_INVLN2: f64 = 1.44269504088896338700e+00; /* 0x3ff71547, 0x652b82fe */
/* Scaled Q's: Qn_here = 2**n * Qn_above, for R(2*z) where z = hxs = x*x/2: */
const EXPM1_Q1: f64 = -3.33333333333331316428e-02; /* BFA11111 111110F4 */
const EXPM1_Q2: f64 = 1.58730158725481460165e-03; /* 3F5A01A0 19FE5585 */
const EXPM1_Q3: f64 = -7.93650757867487942473e-05; /* BF14CE19 9EAADBB7 */
const EXPM1_Q4: f64 = 4.00821782732936239552e-06; /* 3ED0CFCA 86E65239 */
const EXPM1_Q5: f64 = -2.01099218183624371326e-07; /* BE8AFDB7 6E09C32D */

/// `expm1(x)` — bit-identical reference (vendored libm 0.2.16).
fn expm1(mut x: f64) -> f64 {
    let hi: f64;
    let lo: f64;
    let k: i32;
    let c: f64;
    let mut t: f64;
    let mut y: f64;

    let mut ui = x.to_bits();
    let hx = ((ui >> 32) & 0x7fffffff) as u32;
    let sign = (ui >> 63) as i32;

    /* filter out huge and non-finite argument */
    if hx >= 0x4043687A {
        /* if |x|>=56*ln2 */
        if x.is_nan() {
            return x;
        }
        if sign != 0 {
            return -1.0;
        }
        if x > EXPM1_O_THRESHOLD {
            x *= f64::from_bits(0x7fe0000000000000);
            return x;
        }
    }

    /* argument reduction */
    if hx > 0x3fd62e42 {
        /* if  |x| > 0.5 ln2 */
        if hx < 0x3FF0A2B2 {
            /* and |x| < 1.5 ln2 */
            if sign == 0 {
                hi = x - EXPM1_LN2_HI;
                lo = EXPM1_LN2_LO;
                k = 1;
            } else {
                hi = x + EXPM1_LN2_HI;
                lo = -EXPM1_LN2_LO;
                k = -1;
            }
        } else {
            k = (EXPM1_INVLN2 * x + if sign != 0 { -0.5 } else { 0.5 }) as i32;
            t = k as f64;
            hi = x - t * EXPM1_LN2_HI; /* t*ln2_hi is exact here */
            lo = t * EXPM1_LN2_LO;
        }
        x = hi - lo;
        c = (hi - x) - lo;
    } else if hx < 0x3c900000 {
        /* |x| < 2**-54, return x (upstream force_eval only raises underflow) */
        return x;
    } else {
        c = 0.0;
        k = 0;
    }

    /* x is now in primary range */
    let hfx = 0.5 * x;
    let hxs = x * hfx;
    let r1 = 1.0
        + hxs * (EXPM1_Q1 + hxs * (EXPM1_Q2 + hxs * (EXPM1_Q3 + hxs * (EXPM1_Q4 + hxs * EXPM1_Q5))));
    t = 3.0 - r1 * hfx;
    let mut e = hxs * ((r1 - t) / (6.0 - x * t));
    if k == 0 {
        /* c is 0 */
        return x - (x * e - hxs);
    }
    e = x * (e - c) - c;
    e -= hxs;
    /* exp(x) ~ 2^k (x_reduced - e + 1) */
    if k == -1 {
        return 0.5 * (x - e) - 0.5;
    }
    if k == 1 {
        if x < -0.25 {
            return -2.0 * (e - (x + 0.5));
        }
        return 1.0 + 2.0 * (x - e);
    }
    ui = ((0x3ff + k) as u64) << 52; /* 2^k */
    let twopk = f64::from_bits(ui);
    if !(0..=56).contains(&k) {
        /* suffice to return exp(x)-1 */
        y = x - e + 1.0;
        if k == 1024 {
            y = y * 2.0 * f64::from_bits(0x7fe0000000000000);
        } else {
            y = y * twopk;
        }
        return y - 1.0;
    }
    ui = ((0x3ff - k) as u64) << 52; /* 2^-k */
    let uf = f64::from_bits(ui);
    if k < 20 {
        y = (x - e + (1.0 - uf)) * twopk;
    } else {
        y = (x - (e + uf) + 1.0) * twopk;
    }
    y
}

/* tanh(x) = (exp(x) - exp(-x))/(exp(x) + exp(-x))
 *         = (exp(2*x) - 1)/(exp(2*x) - 1 + 2)
 *         = (1 - exp(-2*x))/(exp(-2*x) - 1 + 2)
 */

/// `tanh(x)` — bit-identical reference (vendored libm 0.2.16).
fn tanh(mut x: f64) -> f64 {
    let mut uf: f64 = x;
    let mut ui: u64 = f64::to_bits(uf);

    let w: u32;
    let sign: bool;
    let mut t: f64;

    /* x = |x| */
    sign = ui >> 63 != 0;
    ui &= !1u64 / 2;
    uf = f64::from_bits(ui);
    x = uf;
    w = (ui >> 32) as u32;

    if w > 0x3fe193ea {
        /* |x| > log(3)/2 ~= 0.5493 or nan */
        if w > 0x40340000 {
            /* |x| > 20 or nan */
            /* note: this branch avoids raising overflow */
            t = 1.0 - 0.0 / x;
        } else {
            t = expm1(2.0 * x);
            t = 1.0 - 2.0 / (t + 2.0);
        }
    } else if w > 0x3fd058ae {
        /* |x| > log(5/3)/2 ~= 0.2554 */
        t = expm1(2.0 * x);
        t = t / (t + 2.0);
    } else if w >= 0x00100000 {
        /* |x| >= 0x1p-1022, up to 2ulp error in [0.1,0.2554] */
        t = expm1(-2.0 * x);
        t = -t / (t + 2.0);
    } else {
        /* |x| is subnormal (upstream force_eval only raises underflow) */
        t = x;
    }

    if sign { -t } else { t }
}

/// `atan(x)` — public bit-identical entry (vendored libm 0.2.16).
pub fn almide_rt_libm_atan(x: f64) -> f64 { atan(x) }

/// `expm1(x)` — public bit-identical entry (vendored libm 0.2.16).
pub fn almide_rt_libm_expm1(x: f64) -> f64 { expm1(x) }

/// `tanh(x)` — public bit-identical entry (vendored libm 0.2.16).
pub fn almide_rt_libm_tanh(x: f64) -> f64 { tanh(x) }
