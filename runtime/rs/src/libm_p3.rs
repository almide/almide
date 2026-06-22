// ────────────────── log / log2 / log10 kernel ──────────────────
// origin: FreeBSD e_log.c / e_log2.c / e_log10.c
// ====================================================
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
// ====================================================
// The three share the degree-14 Remez polynomial `R(z)` (Lg1..Lg7) and the
// 2^k(1+f) reduction.

const LN2_HI: f64 = 6.93147180369123816490e-01; /* 3fe62e42 fee00000 */
const LN2_LO: f64 = 1.90821492927058770002e-10; /* 3dea39ef 35793c76 */
const LG1: f64 = 6.666666666666735130e-01; /* 3FE55555 55555593 */
const LG2: f64 = 3.999999999940941908e-01; /* 3FD99999 9997FA04 */
const LG3: f64 = 2.857142874366239149e-01; /* 3FD24924 94229359 */
const LG4: f64 = 2.222219843214978396e-01; /* 3FCC71C5 1D8E78AF */
const LG5: f64 = 1.818357216161805012e-01; /* 3FC74664 96CB03DE */
const LG6: f64 = 1.531383769920937332e-01; /* 3FC39A09 D078C69F */
const LG7: f64 = 1.479819860511658591e-01; /* 3FC2F112 DF3E5244 */

/// `log(x)` (natural) — bit-identical reference (vendored libm 0.2.16).
fn log(mut x: f64) -> f64 {
    let x1p54 = f64::from_bits(0x4350000000000000); // 2^54

    let mut ui = x.to_bits();
    let mut hx: u32 = (ui >> 32) as u32;
    let mut k: i32 = 0;

    if (hx < 0x00100000) || ((hx >> 31) != 0) {
        /* x < 2**-126  */
        if ui << 1 == 0 {
            return -1. / (x * x); /* log(+-0)=-inf */
        }
        if hx >> 31 != 0 {
            return (x - x) / 0.0; /* log(-#) = NaN */
        }
        /* subnormal number, scale x up */
        k -= 54;
        x *= x1p54;
        ui = x.to_bits();
        hx = (ui >> 32) as u32;
    } else if hx >= 0x7ff00000 {
        return x;
    } else if hx == 0x3ff00000 && ui << 32 == 0 {
        return 0.;
    }

    /* reduce x into [sqrt(2)/2, sqrt(2)] */
    hx += 0x3ff00000 - 0x3fe6a09e;
    k += ((hx >> 20) as i32) - 0x3ff;
    hx = (hx & 0x000fffff) + 0x3fe6a09e;
    ui = ((hx as u64) << 32) | (ui & 0xffffffff);
    x = f64::from_bits(ui);

    let f: f64 = x - 1.0;
    let hfsq: f64 = 0.5 * f * f;
    let s: f64 = f / (2.0 + f);
    let z: f64 = s * s;
    let w: f64 = z * z;
    let t1: f64 = w * (LG2 + w * (LG4 + w * LG6));
    let t2: f64 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    let r: f64 = t2 + t1;
    let dk: f64 = k as f64;
    s * (hfsq + r) + dk * LN2_LO - hfsq + f + dk * LN2_HI
}

const IVLN2HI: f64 = 1.44269504072144627571e+00; /* 0x3ff71547, 0x65200000 */
const IVLN2LO: f64 = 1.67517131648865118353e-10; /* 0x3de705fc, 0x2eefa200 */

/// `log2(x)` — bit-identical reference (vendored libm 0.2.16).
fn log2(mut x: f64) -> f64 {
    let x1p54 = f64::from_bits(0x4350000000000000); // 2^54

    let mut ui: u64 = x.to_bits();
    let mut hx: u32 = (ui >> 32) as u32;
    let mut k: i32 = 0;

    if hx < 0x00100000 || (hx >> 31) > 0 {
        if ui << 1 == 0 {
            return -1. / (x * x); /* log(+-0)=-inf */
        }
        if (hx >> 31) > 0 {
            return (x - x) / 0.0; /* log(-#) = NaN */
        }
        /* subnormal number, scale x up */
        k -= 54;
        x *= x1p54;
        ui = x.to_bits();
        hx = (ui >> 32) as u32;
    } else if hx >= 0x7ff00000 {
        return x;
    } else if hx == 0x3ff00000 && ui << 32 == 0 {
        return 0.;
    }

    /* reduce x into [sqrt(2)/2, sqrt(2)] */
    hx += 0x3ff00000 - 0x3fe6a09e;
    k += (hx >> 20) as i32 - 0x3ff;
    hx = (hx & 0x000fffff) + 0x3fe6a09e;
    ui = ((hx as u64) << 32) | (ui & 0xffffffff);
    x = f64::from_bits(ui);

    let f: f64 = x - 1.0;
    let hfsq: f64 = 0.5 * f * f;
    let s: f64 = f / (2.0 + f);
    let z: f64 = s * s;
    let mut w: f64 = z * z;
    let t1: f64 = w * (LG2 + w * (LG4 + w * LG6));
    let t2: f64 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    let r: f64 = t2 + t1;

    /* hi+lo = f - hfsq + s*(hfsq+R) ~ log(1+f) */
    let mut hi: f64 = f - hfsq;
    ui = hi.to_bits();
    ui &= (-1i64 as u64) << 32;
    hi = f64::from_bits(ui);
    let lo: f64 = f - hi - hfsq + s * (hfsq + r);

    let mut val_hi: f64 = hi * IVLN2HI;
    let mut val_lo: f64 = (lo + hi) * IVLN2LO + lo * IVLN2HI;

    /* spadd(val_hi, val_lo, y), except for not using double_t: */
    let y: f64 = k as f64;
    w = y + val_hi;
    val_lo += (y - w) + val_hi;
    val_hi = w;

    val_lo + val_hi
}

const IVLN10HI: f64 = 4.34294481878168880939e-01; /* 0x3fdbcb7b, 0x15200000 */
const IVLN10LO: f64 = 2.50829467116452752298e-11; /* 0x3dbb9438, 0xca9aadd5 */
const LOG10_2HI: f64 = 3.01029995663611771306e-01; /* 0x3FD34413, 0x509F6000 */
const LOG10_2LO: f64 = 3.69423907715893078616e-13; /* 0x3D59FEF3, 0x11F12B36 */

/// `log10(x)` — bit-identical reference (vendored libm 0.2.16).
fn log10(mut x: f64) -> f64 {
    let x1p54 = f64::from_bits(0x4350000000000000); // 2^54

    let mut ui: u64 = x.to_bits();
    let mut hx: u32 = (ui >> 32) as u32;
    let mut k: i32 = 0;

    if hx < 0x00100000 || (hx >> 31) > 0 {
        if ui << 1 == 0 {
            return -1. / (x * x); /* log(+-0)=-inf */
        }
        if (hx >> 31) > 0 {
            return (x - x) / 0.0; /* log(-#) = NaN */
        }
        /* subnormal number, scale x up */
        k -= 54;
        x *= x1p54;
        ui = x.to_bits();
        hx = (ui >> 32) as u32;
    } else if hx >= 0x7ff00000 {
        return x;
    } else if hx == 0x3ff00000 && ui << 32 == 0 {
        return 0.;
    }

    /* reduce x into [sqrt(2)/2, sqrt(2)] */
    hx += 0x3ff00000 - 0x3fe6a09e;
    k += (hx >> 20) as i32 - 0x3ff;
    hx = (hx & 0x000fffff) + 0x3fe6a09e;
    ui = ((hx as u64) << 32) | (ui & 0xffffffff);
    x = f64::from_bits(ui);

    let f: f64 = x - 1.0;
    let hfsq: f64 = 0.5 * f * f;
    let s: f64 = f / (2.0 + f);
    let z: f64 = s * s;
    let mut w: f64 = z * z;
    let t1: f64 = w * (LG2 + w * (LG4 + w * LG6));
    let t2: f64 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    let r: f64 = t2 + t1;

    /* hi+lo = f - hfsq + s*(hfsq+R) ~ log(1+f) */
    let mut hi: f64 = f - hfsq;
    ui = hi.to_bits();
    ui &= (-1i64 as u64) << 32;
    hi = f64::from_bits(ui);
    let lo: f64 = f - hi - hfsq + s * (hfsq + r);

    /* val_hi+val_lo ~ log10(1+f) + k*log10(2) */
    let mut val_hi: f64 = hi * IVLN10HI;
    let dk: f64 = k as f64;
    let y: f64 = dk * LOG10_2HI;
    let mut val_lo: f64 = dk * LOG10_2LO + (lo + hi) * IVLN10LO + lo * IVLN10HI;

    w = y + val_hi;
    val_lo += (y - w) + val_hi;
    val_hi = w;

    val_lo + val_hi
}

// ───────────────────────────── pow ─────────────────────────────
// origin: FreeBSD /usr/src/lib/msun/src/e_pow.c
// ====================================================
// Copyright (C) 2004 by Sun Microsystems, Inc. All rights reserved.
// ====================================================
// Exhaustive special-case handling (0/inf/nan/neg-base/odd-even-int-exponent)
// followed by log2(x) in extra precision, y*log2(x), 2**(...).

const POW_BP: [f64; 2] = [1.0, 1.5];
const POW_DP_H: [f64; 2] = [0.0, 5.84962487220764160156e-01]; /* 0x3fe2b803_40000000 */
const POW_DP_L: [f64; 2] = [0.0, 1.35003920212974897128e-08]; /* 0x3E4CFDEB, 0x43CFD006 */
const POW_TWO53: f64 = 9007199254740992.0; /* 0x43400000_00000000 */
const POW_HUGE: f64 = 1.0e300;
const POW_TINY: f64 = 1.0e-300;
// poly coefs for (3/2)*(log(x)-2s-2/3*s**3):
const POW_L1: f64 = 5.99999999999994648725e-01; /* 0x3fe33333_33333303 */
const POW_L2: f64 = 4.28571428578550184252e-01; /* 0x3fdb6db6_db6fabff */
const POW_L3: f64 = 3.33333329818377432918e-01; /* 0x3fd55555_518f264d */
const POW_L4: f64 = 2.72728123808534006489e-01; /* 0x3fd17460_a91d4101 */
const POW_L5: f64 = 2.30660745775561754067e-01; /* 0x3fcd864a_93c9db65 */
const POW_L6: f64 = 2.06975017800338417784e-01; /* 0x3fca7e28_4a454eef */
const POW_P1: f64 = 1.66666666666666019037e-01; /* 0x3fc55555_5555553e */
const POW_P2: f64 = -2.77777777770155933842e-03; /* 0xbf66c16c_16bebd93 */
const POW_P3: f64 = 6.61375632143793436117e-05; /* 0x3f11566a_af25de2c */
const POW_P4: f64 = -1.65339022054652515390e-06; /* 0xbebbbd41_c5d26bf1 */
const POW_P5: f64 = 4.13813679705723846039e-08; /* 0x3e663769_72bea4d0 */
const POW_LG2: f64 = 6.93147180559945286227e-01; /* 0x3fe62e42_fefa39ef */
const POW_LG2_H: f64 = 6.93147182464599609375e-01; /* 0x3fe62e43_00000000 */
const POW_LG2_L: f64 = -1.90465429995776804525e-09; /* 0xbe205c61_0ca86c39 */
const POW_OVT: f64 = 8.0085662595372944372e-017; /* -(1024-log2(ovfl+.5ulp)) */
const POW_CP: f64 = 9.61796693925975554329e-01; /* 0x3feec709_dc3a03fd =2/(3ln2) */
const POW_CP_H: f64 = 9.61796700954437255859e-01; /* 0x3feec709_e0000000 =(float)cp */
const POW_CP_L: f64 = -7.02846165095275826516e-09; /* 0xbe3e2fe0_145b01f5 =tail cp_h */
const POW_IVLN2: f64 = 1.44269504088896338700e+00; /* 0x3ff71547_652b82fe =1/ln2 */
const POW_IVLN2_H: f64 = 1.44269502162933349609e+00; /* 0x3ff71547_60000000 =24b 1/ln2*/
const POW_IVLN2_L: f64 = 1.92596299112661746887e-08; /* 0x3e54ae0b_f85ddf44 =1/ln2 tail*/

/// `pow(x, y)` — bit-identical reference (vendored libm 0.2.16).
fn pow(x: f64, y: f64) -> f64 {
    let t1: f64;
    let t2: f64;

    let (hx, lx): (i32, u32) = ((x.to_bits() >> 32) as i32, x.to_bits() as u32);
    let (hy, ly): (i32, u32) = ((y.to_bits() >> 32) as i32, y.to_bits() as u32);

    let mut ix: i32 = hx & 0x7fffffff_i32;
    let iy: i32 = hy & 0x7fffffff_i32;

    /* x**0 = 1, even if x is NaN */
    if ((iy as u32) | ly) == 0 {
        return 1.0;
    }

    /* 1**y = 1, even if y is NaN */
    if hx == 0x3ff00000 && lx == 0 {
        return 1.0;
    }

    /* NaN if either arg is NaN */
    if ix > 0x7ff00000
        || (ix == 0x7ff00000 && lx != 0)
        || iy > 0x7ff00000
        || (iy == 0x7ff00000 && ly != 0)
    {
        return x + y;
    }

    /* determine if y is an odd int when x < 0
     * yisint = 0 ... y is not an integer
     * yisint = 1 ... y is an odd int
     * yisint = 2 ... y is an even int
     */
    let mut yisint: i32 = 0;
    let mut k: i32;
    let mut j: i32;
    if hx < 0 {
        if iy >= 0x43400000 {
            yisint = 2; /* even integer y */
        } else if iy >= 0x3ff00000 {
            k = (iy >> 20) - 0x3ff; /* exponent */

            if k > 20 {
                j = (ly >> (52 - k)) as i32;

                if (j << (52 - k)) == (ly as i32) {
                    yisint = 2 - (j & 1);
                }
            } else if ly == 0 {
                j = iy >> (20 - k);

                if (j << (20 - k)) == iy {
                    yisint = 2 - (j & 1);
                }
            }
        }
    }

    if ly == 0 {
        /* special value of y */
        if iy == 0x7ff00000 {
            /* y is +-inf */
            return if ((ix - 0x3ff00000) | (lx as i32)) == 0 {
                /* (-1)**+-inf is 1 */
                1.0
            } else if ix >= 0x3ff00000 {
                /* (|x|>1)**+-inf = inf,0 */
                if hy >= 0 {
                    y
                } else {
                    0.0
                }
            } else {
                /* (|x|<1)**+-inf = 0,inf */
                if hy >= 0 {
                    0.0
                } else {
                    -y
                }
            };
        }

        if iy == 0x3ff00000 {
            /* y is +-1 */
            return if hy >= 0 { x } else { 1.0 / x };
        }

        if hy == 0x40000000 {
            /* y is 2 */
            return x * x;
        }

        if hy == 0x3fe00000 {
            /* y is 0.5 */
            if hx >= 0 {
                /* x >= +0 */
                return x.sqrt();
            }
        }
    }

    let mut ax: f64 = x.abs();
    if lx == 0 {
        /* special value of x */
        if ix == 0x7ff00000 || ix == 0 || ix == 0x3ff00000 {
            /* x is +-0,+-inf,+-1 */
            let mut z: f64 = ax;

            if hy < 0 {
                /* z = (1/|x|) */
                z = 1.0 / z;
            }

            if hx < 0 {
                if ((ix - 0x3ff00000) | yisint) == 0 {
                    z = (z - z) / (z - z); /* (-1)**non-int is NaN */
                } else if yisint == 1 {
                    z = -z; /* (x<0)**odd = -(|x|**odd) */
                }
            }

            return z;
        }
    }

    let mut s: f64 = 1.0; /* sign of result */
    if hx < 0 {
        if yisint == 0 {
            /* (x<0)**(non-int) is NaN */
            return (x - x) / (x - x);
        }

        if yisint == 1 {
            /* (x<0)**(odd int) */
            s = -1.0;
        }
    }

    /* |y| is HUGE */
    if iy > 0x41e00000 {
        /* if |y| > 2**31 */
        if iy > 0x43f00000 {
            /* if |y| > 2**64, must o/uflow */
            if ix <= 0x3fefffff {
                return if hy < 0 {
                    POW_HUGE * POW_HUGE
                } else {
                    POW_TINY * POW_TINY
                };
            }

            if ix >= 0x3ff00000 {
                return if hy > 0 {
                    POW_HUGE * POW_HUGE
                } else {
                    POW_TINY * POW_TINY
                };
            }
        }

        /* over/underflow if x is not close to one */
        if ix < 0x3fefffff {
            return if hy < 0 {
                s * POW_HUGE * POW_HUGE
            } else {
                s * POW_TINY * POW_TINY
            };
        }
        if ix > 0x3ff00000 {
            return if hy > 0 {
                s * POW_HUGE * POW_HUGE
            } else {
                s * POW_TINY * POW_TINY
            };
        }

        /* now |1-x| is TINY <= 2**-20, suffice to compute
        log(x) by x-x^2/2+x^3/3-x^4/4 */
        let t: f64 = ax - 1.0; /* t has 20 trailing zeros */
        let w: f64 = (t * t) * (0.5 - t * (0.3333333333333333333333 - t * 0.25));
        let u: f64 = POW_IVLN2_H * t; /* ivln2_h has 21 sig. bits */
        let v: f64 = t * POW_IVLN2_L - w * POW_IVLN2;
        t1 = with_set_low_word(u + v, 0);
        t2 = v - (t1 - u);
    } else {
        let mut n: i32 = 0;

        if ix < 0x00100000 {
            /* take care subnormal number */
            ax *= POW_TWO53;
            n -= 53;
            ix = get_high_word(ax) as i32;
        }

        n += (ix >> 20) - 0x3ff;
        j = ix & 0x000fffff;

        /* determine interval */
        let kk: i32;
        ix = j | 0x3ff00000; /* normalize ix */
        if j <= 0x3988E {
            /* |x|<sqrt(3/2) */
            kk = 0;
        } else if j < 0xBB67A {
            /* |x|<sqrt(3)   */
            kk = 1;
        } else {
            kk = 0;
            n += 1;
            ix -= 0x00100000;
        }
        ax = with_set_high_word(ax, ix as u32);

        /* compute ss = s_h+s_l = (x-1)/(x+1) or (x-1.5)/(x+1.5) */
        let u: f64 = ax - POW_BP[kk as usize]; /* bp[0]=1.0, bp[1]=1.5 */
        let v: f64 = 1.0 / (ax + POW_BP[kk as usize]);
        let ss: f64 = u * v;
        let s_h = with_set_low_word(ss, 0);

        /* t_h=ax+bp[k] High */
        let t_h: f64 = with_set_high_word(
            0.0,
            ((ix as u32 >> 1) | 0x20000000) + 0x00080000 + ((kk as u32) << 18),
        );
        let t_l: f64 = ax - (t_h - POW_BP[kk as usize]);
        let s_l: f64 = v * ((u - s_h * t_h) - s_h * t_l);

        /* compute log(ax) */
        let s2: f64 = ss * ss;
        let mut r: f64 =
            s2 * s2 * (POW_L1 + s2 * (POW_L2 + s2 * (POW_L3 + s2 * (POW_L4 + s2 * (POW_L5 + s2 * POW_L6)))));
        r += s_l * (s_h + ss);
        let s2: f64 = s_h * s_h;
        let t_h: f64 = with_set_low_word(3.0 + s2 + r, 0);
        let t_l: f64 = r - ((t_h - 3.0) - s2);

        /* u+v = ss*(1+...) */
        let u: f64 = s_h * t_h;
        let v: f64 = s_l * t_h + t_l * ss;

        /* 2/(3log2)*(ss+...) */
        let p_h: f64 = with_set_low_word(u + v, 0);
        let p_l = v - (p_h - u);
        let z_h: f64 = POW_CP_H * p_h; /* cp_h+cp_l = 2/(3*log2) */
        let z_l: f64 = POW_CP_L * p_h + p_l * POW_CP + POW_DP_L[kk as usize];

        /* log2(ax) = (ss+..)*2/(3*log2) = n + dp_h + z_h + z_l */
        let t: f64 = n as f64;
        t1 = with_set_low_word(((z_h + z_l) + POW_DP_H[kk as usize]) + t, 0);
        t2 = z_l - (((t1 - t) - POW_DP_H[kk as usize]) - z_h);
    }

    /* split up y into y1+y2 and compute (y1+y2)*(t1+t2) */
    let y1: f64 = with_set_low_word(y, 0);
    let p_l: f64 = (y - y1) * t1 + y * t2;
    let mut p_h: f64 = y1 * t1;
    let z: f64 = p_l + p_h;
    let mut j: i32 = (z.to_bits() >> 32) as i32;
    let i: i32 = z.to_bits() as i32;

    if j >= 0x40900000 {
        /* z >= 1024 */
        if (j - 0x40900000) | i != 0 {
            /* if z > 1024 */
            return s * POW_HUGE * POW_HUGE; /* overflow */
        }

        if p_l + POW_OVT > z - p_h {
            return s * POW_HUGE * POW_HUGE; /* overflow */
        }
    } else if (j & 0x7fffffff) >= 0x4090cc00 {
        /* z <= -1075 */
        if (((j as u32) - 0xc090cc00) | (i as u32)) != 0 {
            /* z < -1075 */
            return s * POW_TINY * POW_TINY; /* underflow */
        }

        if p_l <= z - p_h {
            return s * POW_TINY * POW_TINY; /* underflow */
        }
    }

    /* compute 2**(p_h+p_l) */
    let i: i32 = j & 0x7fffffff_i32;
    k = (i >> 20) - 0x3ff;
    let mut n: i32 = 0;

    if i > 0x3fe00000 {
        /* if |z| > 0.5, set n = [z+0.5] */
        n = j + (0x00100000 >> (k + 1));
        k = ((n & 0x7fffffff) >> 20) - 0x3ff; /* new k for n */
        let t: f64 = with_set_high_word(0.0, (n & !(0x000fffff >> k)) as u32);
        n = ((n & 0x000fffff) | 0x00100000) >> (20 - k);
        if j < 0 {
            n = -n;
        }
        p_h -= t;
    }

    let t: f64 = with_set_low_word(p_l + p_h, 0);
    let u: f64 = t * POW_LG2_H;
    let v: f64 = (p_l - (t - p_h)) * POW_LG2 + t * POW_LG2_L;
    let mut z: f64 = u + v;
    let w: f64 = v - (z - u);
    let t: f64 = z * z;
    let t1: f64 = z - t * (POW_P1 + t * (POW_P2 + t * (POW_P3 + t * (POW_P4 + t * POW_P5))));
    let r: f64 = (z * t1) / (t1 - 2.0) - (w + z * w);
    z = 1.0 - (r - z);
    j = get_high_word(z) as i32;
    j += n << 20;

    if (j >> 20) <= 0 {
        /* subnormal output */
        z = vscalbn(z, n);
    } else {
        z = with_set_high_word(z, j as u32);
    }

    s * z
}

// ─────────────────── public runtime entry points ───────────────
// Named `almide_rt_libm_*` so the runtime-registry build script auto-links
// the `libm` module wherever `math` (or any other module) references it, and
// so the flattened single-file crate has no bare `sin`/`cos`/`tan` collisions.

/// Deterministic `sin` for the Almide runtime. See module docs.
#[inline(always)]
pub fn almide_rt_libm_sin(x: f64) -> f64 { sin(x) }
/// Deterministic `cos` for the Almide runtime. See module docs.
#[inline(always)]
pub fn almide_rt_libm_cos(x: f64) -> f64 { cos(x) }
/// Deterministic `tan` for the Almide runtime. See module docs.
#[inline(always)]
pub fn almide_rt_libm_tan(x: f64) -> f64 { tan(x) }
/// Deterministic `exp` for the Almide runtime. See module docs.
#[inline(always)]
pub fn almide_rt_libm_exp(x: f64) -> f64 { exp(x) }
/// Deterministic natural `log` for the Almide runtime. See module docs.
#[inline(always)]
pub fn almide_rt_libm_log(x: f64) -> f64 { log(x) }
/// Deterministic `log2` for the Almide runtime. See module docs.
#[inline(always)]
pub fn almide_rt_libm_log2(x: f64) -> f64 { log2(x) }
/// Deterministic `log10` for the Almide runtime. See module docs.
#[inline(always)]
pub fn almide_rt_libm_log10(x: f64) -> f64 { log10(x) }
/// Deterministic `pow` for the Almide runtime. See module docs.
#[inline(always)]
pub fn almide_rt_libm_pow(base: f64, exp: f64) -> f64 { pow(base, exp) }
