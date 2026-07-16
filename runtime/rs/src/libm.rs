// Vendored musl-libm transcendental functions (deterministic, cross-platform).
//
// WHY THIS EXISTS
// ---------------
// Almide's correctness contract requires `math.sin/cos/tan` to be **bit-identical
// across targets** (native ↔ WASM) AND **deterministic across platforms**. The
// Rust standard library's `f64::sin/cos/tan` delegate to the host's system libm,
// whose last-ULP result is platform-specific (glibc vs macOS vs musl differ), so
// there is no stable oracle. Following the Java `StrictMath`/fdlibm decision, we
// REPLACE the native implementation with a fixed reference algorithm so the same
// bits come out everywhere — and the WASM port (`emit_wasm/rt_libm.rs`) mirrors
// THIS file function-for-function.
//
// PROVENANCE
// ----------
// Vendored from the `libm` crate, version 0.2.16, which lives in the
// `rust-lang/compiler-builtins` repository at git commit
// `dfd2203a4d6110820ad7bb65cafe1bf331a03a3d` (path_in_vcs: `libm`). The `libm`
// crate is itself a faithful Rust port of FreeBSD's `msun` (Sun Microsystems
// fdlibm). Dual-licensed MIT OR Apache-2.0; the upstream Sun copyright notice is
// preserved on each kernel below.
//
// Functions vendored:
//   - trig (sin/cos/tan): `rem_pio2`, `rem_pio2_large`, `k_sin`, `k_cos`,
//     `k_tan`, `sin`, `cos`, `tan`, plus `scalbn`/`floor`.
//   - exp / log / log2 / log10 / pow: `exp`, `log`, `log2`, `log10`, `pow` plus
//     the word-manipulation helpers `pow` needs (`get_high_word`,
//     `with_set_high_word`, `with_set_low_word`; `fabs`/`sqrt` are the obvious
//     `f64::abs`/`f64::sqrt`, which are bit-exact / IEEE-754 correctly rounded
//     and so match the WASM `f64.abs`/`f64.sqrt` opcodes).
// Translated faithfully: same coefficients, same branch structure, same bit
// manipulations. The only mechanical changes are removing the upstream
// `i!`/`div!`/`force_eval!`/`no_panic` macros (replaced by plain
// indexing/division — the runtime rlib is built with overflow-checks=off) and
// dropping `#[cfg]`-gated arch intrinsics so a single portable path remains.

// ─────────────────────────── helpers ───────────────────────────

/// Faithful port of `libm::floor` (generic path): round toward -inf.
#[inline]
fn vfloor(x: f64) -> f64 {
    let ui = x.to_bits();
    let e = ((ui >> 52) & 0x7ff) as i32 - 0x3ff; // unbiased exponent
    if e >= 52 {
        return x; // already integral (or inf/NaN)
    }
    if e >= 0 {
        let m: u64 = 0x000f_ffff_ffff_ffff >> e;
        if ui & m == 0 {
            return x; // integral
        }
        // raise inexact would happen here in C; not observable
        let mut ui = ui;
        if (ui >> 63) != 0 {
            ui += m; // negative: round away from zero (toward -inf)
        }
        ui &= !m;
        f64::from_bits(ui)
    } else {
        // |x| < 1
        if (ui >> 63) == 0 {
            0.0 // positive fraction floors to +0
        } else if ui << 1 != 0 {
            -1.0 // negative non-zero fraction floors to -1
        } else {
            x // -0.0
        }
    }
}

/// Faithful port of `libm::scalbn` (generic path): `x * 2^n`, exact when no
/// over/underflow, with prescaling so out-of-`f64`-range `2^n` is handled.
#[inline]
fn vscalbn(mut x: f64, mut n: i32) -> f64 {
    // f64 parameters
    const SIG_BITS: i32 = 52;
    const EXP_MAX: i32 = 1023;
    const EXP_MIN: i32 = -1022;
    const EXP_BIAS: u64 = 1023;
    let sig_total_bits = SIG_BITS + 1; // 53

    let f_exp_max = f64::from_bits((EXP_BIAS << 1) << 52); // 2^1023
    let f_exp_min = f64::from_bits(1u64 << 52);            // 2^-1022
    let f_pow_subnorm = f64::from_bits(((sig_total_bits as u64) + EXP_BIAS) << 52); // 2^53

    if n > EXP_MAX {
        x *= f_exp_max;
        n -= EXP_MAX;
        if n > EXP_MAX {
            x *= f_exp_max;
            n -= EXP_MAX;
            if n > EXP_MAX {
                n = EXP_MAX;
            }
        }
    } else if n < EXP_MIN {
        let mul = f_exp_min * f_pow_subnorm;
        let add = -EXP_MIN - sig_total_bits;
        x *= mul;
        n += add;
        if n < EXP_MIN {
            x *= mul;
            n += add;
            if n < EXP_MIN {
                n = EXP_MIN;
            }
        }
    }

    let scale = f64::from_bits(((EXP_BIAS as i32 + n) as u64) << 52);
    x * scale
}

// ─────────────────────── rem_pio2_large ────────────────────────
// origin: FreeBSD /usr/src/lib/msun/src/k_rem_pio2.c
// ====================================================
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
// Developed at SunSoft, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this software is freely
// granted, provided that this notice is preserved.
// ====================================================

// initial value for jk
const INIT_JK: [usize; 4] = [3, 4, 4, 6];

// Table of constants for 2/pi, 396 Hex digits (476 decimal) of 2/pi:
//   the (24*i)-th .. (24*i+23)-th bit of 2/pi after the binary point;
//   IPIO2[i] * 2^(-24(i+1)).
const IPIO2: [i32; 690] = [
    0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62, 0x95993C, 0x439041, 0xFE5163,
    0xABDEBB, 0xC561B7, 0x246E3A, 0x424DD2, 0xE00649, 0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129,
    0xA73EE8, 0x8235F5, 0x2EBB44, 0x84E99C, 0x7026B4, 0x5F7E41, 0x3991D6, 0x398353, 0x39F49C,
    0x845F8B, 0xBDF928, 0x3B1FF8, 0x97FFDE, 0x05980F, 0xEF2F11, 0x8B5A0A, 0x6D1F6D, 0x367ECF,
    0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5, 0xF17B3D, 0x0739F7, 0x8A5292,
    0xEA6BFB, 0x5FB11F, 0x8D5D08, 0x560330, 0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3,
    0x91615E, 0xE61B08, 0x659985, 0x5F14A0, 0x68408D, 0xFFD880, 0x4D7327, 0x310606, 0x1556CA,
    0x73A8C9, 0x60E27B, 0xC08C6B, 0x47C419, 0xC367CD, 0xDCE809, 0x2A8359, 0xC4768B, 0x961CA6,
    0xDDAF44, 0xD15719, 0x053EA5, 0xFF0705, 0x3F7E33, 0xE832C2, 0xDE4F98, 0x327DBB, 0xC33D26,
    0xEF6B1E, 0x5EF89F, 0x3A1F35, 0xCAF27F, 0x1D87F1, 0x21907C, 0x7C246A, 0xFA6ED5, 0x772D30,
    0x433B15, 0xC614B5, 0x9D19C3, 0xC2C4AD, 0x414D2C, 0x5D000C, 0x467D86, 0x2D71E3, 0x9AC69B,
    0x006233, 0x7CD2B4, 0x97A7B4, 0xD55537, 0xF63ED7, 0x1810A3, 0xFC764D, 0x2A9D64, 0xABD770,
    0xF87C63, 0x57B07A, 0xE71517, 0x5649C0, 0xD9D63B, 0x3884A7, 0xCB2324, 0x778AD6, 0x23545A,
    0xB91F00, 0x1B0AF1, 0xDFCE19, 0xFF319F, 0x6A1E66, 0x615799, 0x47FBAC, 0xD87F7E, 0xB76522,
    0x89E832, 0x60BFE6, 0xCDC4EF, 0x09366C, 0xD43F5D, 0xD7DE16, 0xDE3B58, 0x929BDE, 0x2822D2,
    0xE88628, 0x4D58E2, 0x32CAC6, 0x16E308, 0xCB7DE0, 0x50C017, 0xA71DF3, 0x5BE018, 0x34132E,
    0x621283, 0x014883, 0x5B8EF5, 0x7FB0AD, 0xF2E91E, 0x434A48, 0xD36710, 0xD8DDAA, 0x425FAE,
    0xCE616A, 0xA4280A, 0xB499D3, 0xF2A606, 0x7F775C, 0x83C2A3, 0x883C61, 0x78738A, 0x5A8CAF,
    0xBDD76F, 0x63A62D, 0xCBBFF4, 0xEF818D, 0x67C126, 0x45CA55, 0x36D9CA, 0xD2A828, 0x8D61C2,
    0x77C912, 0x142604, 0x9B4612, 0xC459C4, 0x44C5C8, 0x91B24D, 0xF31700, 0xAD43D4, 0xE54929,
    0x10D5FD, 0xFCBE00, 0xCC941E, 0xEECE70, 0xF53E13, 0x80F1EC, 0xC3E7B3, 0x28F8C7, 0x940593,
    0x3E71C1, 0xB3092E, 0xF3450B, 0x9C1288, 0x7B20AB, 0x9FB52E, 0xC29247, 0x2F327B, 0x6D550C,
    0x90A772, 0x1FE76B, 0x96CB31, 0x4A1679, 0xE27941, 0x89DFF4, 0x9794E8, 0x84E6E2, 0x973199,
    0x6BED88, 0x365F5F, 0x0EFDBB, 0xB49A48, 0x6CA467, 0x427271, 0x325D8D, 0xB8159F, 0x09E5BC,
    0x25318D, 0x3974F7, 0x1C0530, 0x010C0D, 0x68084B, 0x58EE2C, 0x90AA47, 0x02E774, 0x24D6BD,
    0xA67DF7, 0x72486E, 0xEF169F, 0xA6948E, 0xF691B4, 0x5153D1, 0xF20ACF, 0x339820, 0x7E4BF5,
    0x6863B2, 0x5F3EDD, 0x035D40, 0x7F8985, 0x295255, 0xC06437, 0x10D86D, 0x324832, 0x754C5B,
    0xD4714E, 0x6E5445, 0xC1090B, 0x69F52A, 0xD56614, 0x9D0727, 0x50045D, 0xDB3BB4, 0xC576EA,
    0x17F987, 0x7D6B49, 0xBA271D, 0x296996, 0xACCCC6, 0x5414AD, 0x6AE290, 0x89D988, 0x50722C,
    0xBEA404, 0x940777, 0x7030F3, 0x27FC00, 0xA871EA, 0x49C266, 0x3DE064, 0x83DD97, 0x973FA3,
    0xFD9443, 0x8C860D, 0xDE4131, 0x9D3992, 0x8C70DD, 0xE7B717, 0x3BDF08, 0x2B3715, 0xA0805C,
    0x93805A, 0x921110, 0xD8E80F, 0xAF806C, 0x4BFFDB, 0x0F9038, 0x761859, 0x15A562, 0xBBCB61,
    0xB989C7, 0xBD4010, 0x04F2D2, 0x277549, 0xF6B6EB, 0xBB22DB, 0xAA140A, 0x2F2689, 0x768364,
    0x333B09, 0x1A940E, 0xAA3A51, 0xC2A31D, 0xAEEDAF, 0x12265C, 0x4DC26D, 0x9C7A2D, 0x9756C0,
    0x833F03, 0xF6F009, 0x8C402B, 0x99316D, 0x07B439, 0x15200C, 0x5BC3D8, 0xC492F5, 0x4BADC6,
    0xA5CA4E, 0xCD37A7, 0x36A9E6, 0x9492AB, 0x6842DD, 0xDE6319, 0xEF8C76, 0x528B68, 0x37DBFC,
    0xABA1AE, 0x3115DF, 0xA1AE00, 0xDAFB0C, 0x664D64, 0xB705ED, 0x306529, 0xBF5657, 0x3AFF47,
    0xB9F96A, 0xF3BE75, 0xDF9328, 0x3080AB, 0xF68C66, 0x15CB04, 0x0622FA, 0x1DE4D9, 0xA4B33D,
    0x8F1B57, 0x09CD36, 0xE9424E, 0xA4BE13, 0xB52333, 0x1AAAF0, 0xA8654F, 0xA5C1D2, 0x0F3F0B,
    0xCD785B, 0x76F923, 0x048B7B, 0x721789, 0x53A6C6, 0xE26E6F, 0x00EBEF, 0x584A9B, 0xB7DAC4,
    0xBA66AA, 0xCFCF76, 0x1D02D1, 0x2DF1B1, 0xC1998C, 0x77ADC3, 0xDA4886, 0xA05DF7, 0xF480C6,
    0x2FF0AC, 0x9AECDD, 0xBC5C3F, 0x6DDED0, 0x1FC790, 0xB6DB2A, 0x3A25A3, 0x9AAF00, 0x9353AD,
    0x0457B6, 0xB42D29, 0x7E804B, 0xA707DA, 0x0EAA76, 0xA1597B, 0x2A1216, 0x2DB7DC, 0xFDE5FA,
    0xFEDB89, 0xFDBE89, 0x6C76E4, 0xFCA906, 0x70803E, 0x156E85, 0xFF87FD, 0x073E28, 0x336761,
    0x86182A, 0xEABD4D, 0xAFE7B3, 0x6E6D8F, 0x396795, 0x5BBF31, 0x48D784, 0x16DF30, 0x432DC7,
    0x356125, 0xCE70C9, 0xB8CB30, 0xFD6CBF, 0xA200A4, 0xE46C05, 0xA0DD5A, 0x476F21, 0xD21262,
    0x845CB9, 0x496170, 0xE0566B, 0x015299, 0x375550, 0xB7D51E, 0xC4F133, 0x5F6E13, 0xE4305D,
    0xA92E85, 0xC3B21D, 0x3632A1, 0xA4B708, 0xD4B1EA, 0x21F716, 0xE4698F, 0x77FF27, 0x80030C,
    0x2D408D, 0xA0CD4F, 0x99A520, 0xD3A2B3, 0x0A5D2F, 0x42F9B4, 0xCBDA11, 0xD0BE7D, 0xC1DB9B,
    0xBD17AB, 0x81A2CA, 0x5C6A08, 0x17552E, 0x550027, 0xF0147F, 0x8607E1, 0x640B14, 0x8D4196,
    0xDEBE87, 0x2AFDDA, 0xB6256B, 0x34897B, 0xFEF305, 0x9EBFB9, 0x4F6A68, 0xA82A4A, 0x5AC44F,
    0xBCF82D, 0x985AD7, 0x95C7F4, 0x8D4D0D, 0xA63A20, 0x5F57A4, 0xB13F14, 0x953880, 0x0120CC,
    0x86DD71, 0xB6DEC9, 0xF560BF, 0x11654D, 0x6B0701, 0xACB08C, 0xD0C0B2, 0x485551, 0x0EFB1E,
    0xC37295, 0x3B06A3, 0x3540C0, 0x7BDC06, 0xCC45E0, 0xFA294E, 0xC8CAD6, 0x41F3E8, 0xDE647C,
    0xD8649B, 0x31BED9, 0xC397A4, 0xD45877, 0xC5E369, 0x13DAF0, 0x3C3ABA, 0x461846, 0x5F7555,
    0xF5BDD2, 0xC6926E, 0x5D2EAC, 0xED440E, 0x423E1C, 0x87C461, 0xE9FD29, 0xF3D6E7, 0xCA7C22,
    0x35916F, 0xC5E008, 0x8DD7FF, 0xE26A6E, 0xC6FDB0, 0xC10893, 0x745D7C, 0xB2AD6B, 0x9D6ECD,
    0x7B723E, 0x6A11C6, 0xA9CFF7, 0xDF7329, 0xBAC9B5, 0x5100B7, 0x0DB2E2, 0x24BA74, 0x607DE5,
    0x8AD874, 0x2C150D, 0x0C1881, 0x94667E, 0x162901, 0x767A9F, 0xBEFDFD, 0xEF4556, 0x367ED9,
    0x13D9EC, 0xB9BA8B, 0xFC97C4, 0x27A831, 0xC36EF1, 0x36C594, 0x56A8D8, 0xB5A8B4, 0x0ECCCF,
    0x2D8912, 0x34576F, 0x89562C, 0xE3CE99, 0xB920D6, 0xAA5E6B, 0x9C2A3E, 0xCC5F11, 0x4A0BFD,
    0xFBF4E1, 0x6D3B8E, 0x2C86E2, 0x84D4E9, 0xA9B4FC, 0xD1EEEF, 0xC9352E, 0x61392F, 0x442138,
    0xC8D91B, 0x0AFC81, 0x6A4AFB, 0xD81C2F, 0x84B453, 0x8C994E, 0xCC2254, 0xDC552A, 0xD6C6C0,
    0x96190B, 0xB8701A, 0x649569, 0x605A26, 0xEE523F, 0x0F117F, 0x11B5F4, 0xF5CBFC, 0x2DBC34,
    0xEEBC34, 0xCC5DE8, 0x605EDD, 0x9B8E67, 0xEF3392, 0xB817C9, 0x9B5861, 0xBC57E1, 0xC68351,
    0x103ED8, 0x4871DD, 0xDD1C2D, 0xA118AF, 0x462C21, 0xD7F359, 0x987AD9, 0xC0549E, 0xFA864F,
    0xFC0656, 0xAE79E5, 0x362289, 0x22AD38, 0xDC9367, 0xAAE855, 0x382682, 0x9BE7CA, 0xA40D51,
    0xB13399, 0x0ED7A9, 0x480569, 0xF0B265, 0xA7887F, 0x974C88, 0x36D1F9, 0xB39221, 0x4A827B,
    0x21CF98, 0xDC9F40, 0x5547DC, 0x3A74E1, 0x42EB67, 0xDF9DFE, 0x5FD45E, 0xA4677B, 0x7AACBA,
    0xA2F655, 0x23882B, 0x55BA41, 0x086E59, 0x862A21, 0x834739, 0xE6E389, 0xD49EE5, 0x40FB49,
    0xE956FF, 0xCA0F1C, 0x8A59C5, 0x2BFA94, 0xC5C1D3, 0xCFC50F, 0xAE5ADB, 0x86C547, 0x624385,
    0x3B8621, 0x94792C, 0x876110, 0x7B4C2A, 0x1A2C80, 0x12BF43, 0x902688, 0x893C78, 0xE4C4A8,
    0x7BDBE5, 0xC23AC4, 0xEAF426, 0x8A67F7, 0xBF920D, 0x2BA365, 0xB1933D, 0x0B7CBD, 0xDC51A4,
    0x63DD27, 0xDDE169, 0x19949A, 0x9529A8, 0x28CE68, 0xB4ED09, 0x209F44, 0xCA984E, 0x638270,
    0x237C7E, 0x32B90F, 0x8EF5A7, 0xE75614, 0x08F121, 0x2A9DB5, 0x4D7E6F, 0x5119A5, 0xABF9B5,
    0xD6DF82, 0x61DD96, 0x023616, 0x9F3AC4, 0xA1A283, 0x6DED72, 0x7A8D39, 0xA9B882, 0x5C326B,
    0x5B2746, 0xED3400, 0x7700D2, 0x55F4FC, 0x4D5901, 0x8071E0,
];

const PIO2: [f64; 8] = [
    1.57079625129699707031e+00, /* 0x3FF921FB, 0x40000000 */
    7.54978941586159635335e-08, /* 0x3E74442D, 0x00000000 */
    5.39030252995776476554e-15, /* 0x3CF84698, 0x80000000 */
    3.28200341580791294123e-22, /* 0x3B78CC51, 0x60000000 */
    1.27065575308067607349e-29, /* 0x39F01B83, 0x80000000 */
    1.22933308981111328932e-36, /* 0x387A2520, 0x40000000 */
    2.73370053816464559624e-44, /* 0x36E38222, 0x80000000 */
    2.16741683877804819444e-51, /* 0x3569F31D, 0x00000000 */
];

/// Return the last three digits of N with y = x - N*pi/2 so that |y| < pi/2,
/// computing the integer (mod 8) and fraction parts of (2/pi)*x without the
/// full multiplication. Operations are independent of the input's exponent.
fn rem_pio2_large(x: &[f64], y: &mut [f64], e0: i32, prec: usize) -> i32 {
    let x1p24 = f64::from_bits(0x4170000000000000); // 2^24
    let x1p_24 = f64::from_bits(0x3e70000000000000); // 2^-24

    let nx = x.len();

    let mut fw: f64;
    let mut n: i32;
    let mut ih: i32;
    let mut z: f64;
    let mut f: [f64; 20] = [0.; 20];
    let mut fq: [f64; 20] = [0.; 20];
    let mut q: [f64; 20] = [0.; 20];
    let mut iq: [i32; 20] = [0; 20];

    /* initialize jk */
    let jk = INIT_JK[prec];
    let jp = jk;

    /* determine jx,jv,q0, note that 3>q0 */
    let jx = nx - 1;
    let mut jv = (e0 - 3) / 24;
    if jv < 0 {
        jv = 0;
    }
    let mut q0 = e0 - 24 * (jv + 1);
    let jv = jv as usize;

    /* set up f[0] to f[jx+jk] where f[jx+jk] = ipio2[jv+jk] */
    let mut j = (jv as i32) - (jx as i32);
    let m = jx + jk;
    for i in 0..=m {
        f[i] = if j < 0 { 0. } else { IPIO2[j as usize] as f64 };
        j += 1;
    }

    /* compute q[0],q[1],...q[jk] */
    for i in 0..=jk {
        fw = 0f64;
        for j in 0..=jx {
            fw += x[j] * f[jx + i - j];
        }
        q[i] = fw;
    }

    let mut jz = jk;

    'recompute: loop {
        /* distill q[] into iq[] reversingly */
        let mut i = 0i32;
        z = q[jz];
        for j in (1..=jz).rev() {
            fw = (x1p_24 * z) as i32 as f64;
            iq[i as usize] = (z - x1p24 * fw) as i32;
            z = q[j - 1] + fw;
            i += 1;
        }

        /* compute n */
        z = vscalbn(z, q0); /* actual value of z */
        z -= 8.0 * vfloor(z * 0.125); /* trim off integer >= 8 */
        n = z as i32;
        z -= n as f64;
        ih = 0;
        if q0 > 0 {
            /* need iq[jz-1] to determine n */
            i = iq[jz - 1] >> (24 - q0);
            n += i;
            iq[jz - 1] -= i << (24 - q0);
            ih = iq[jz - 1] >> (23 - q0);
        } else if q0 == 0 {
            ih = iq[jz - 1] >> 23;
        } else if z >= 0.5 {
            ih = 2;
        }

        if ih > 0 {
            /* q > 0.5 */
            n += 1;
            let mut carry = 0i32;
            for i in 0..jz {
                /* compute 1-q */
                let j = iq[i];
                if carry == 0 {
                    if j != 0 {
                        carry = 1;
                        iq[i] = 0x1000000 - j;
                    }
                } else {
                    iq[i] = 0xffffff - j;
                }
            }
            if q0 > 0 {
                /* rare case: chance is 1 in 12 */
                match q0 {
                    1 => {
                        iq[jz - 1] &= 0x7fffff;
                    }
                    2 => {
                        iq[jz - 1] &= 0x3fffff;
                    }
                    _ => {}
                }
            }
            if ih == 2 {
                z = 1. - z;
                if carry != 0 {
                    z -= vscalbn(1., q0);
                }
            }
        }

        /* check if recomputation is needed */
        if z == 0. {
            let mut j = 0;
            for i in (jk..=jz - 1).rev() {
                j |= iq[i];
            }
            if j == 0 {
                /* need recomputation */
                let mut k = 1;
                while iq[jk - k] == 0 {
                    k += 1; /* k = no. of terms needed */
                }

                for i in (jz + 1)..=(jz + k) {
                    /* add q[jz+1] to q[jz+k] */
                    f[jx + i] = IPIO2[jv + i] as f64;
                    fw = 0f64;
                    for j in 0..=jx {
                        fw += x[j] * f[jx + i - j];
                    }
                    q[i] = fw;
                }
                jz += k;
                continue 'recompute;
            }
        }

        break;
    }

    /* chop off zero terms */
    if z == 0. {
        jz -= 1;
        q0 -= 24;
        while iq[jz] == 0 {
            jz -= 1;
            q0 -= 24;
        }
    } else {
        /* break z into 24-bit if necessary */
        z = vscalbn(z, -q0);
        if z >= x1p24 {
            fw = (x1p_24 * z) as i32 as f64;
            iq[jz] = (z - x1p24 * fw) as i32;
            jz += 1;
            q0 += 24;
            iq[jz] = fw as i32;
        } else {
            iq[jz] = z as i32;
        }
    }

    /* convert integer "bit" chunk to floating-point value */
    fw = vscalbn(1., q0);
    for i in (0..=jz).rev() {
        q[i] = fw * (iq[i] as f64);
        fw *= x1p_24;
    }

    /* compute PIo2[0,...,jp]*q[jz,...,0] */
    for i in (0..=jz).rev() {
        fw = 0f64;
        let mut k = 0;
        while (k <= jp) && (k <= jz - i) {
            fw += PIO2[k] * q[i + k];
            k += 1;
        }
        fq[jz - i] = fw;
    }

    /* compress fq[] into y[] */
    match prec {
        0 => {
            fw = 0f64;
            for i in (0..=jz).rev() {
                fw += fq[i];
            }
            y[0] = if ih == 0 { fw } else { -fw };
        }
        1 | 2 => {
            fw = 0f64;
            for i in (0..=jz).rev() {
                fw += fq[i];
            }
            y[0] = if ih == 0 { fw } else { -fw };
            fw = fq[0] - fw;
            for i in 1..=jz {
                fw += fq[i];
            }
            y[1] = if ih == 0 { fw } else { -fw };
        }
        3 => {
            /* painful */
            for i in (1..=jz).rev() {
                fw = fq[i - 1] + fq[i];
                fq[i] += fq[i - 1] - fw;
                fq[i - 1] = fw;
            }
            for i in (2..=jz).rev() {
                fw = fq[i - 1] + fq[i];
                fq[i] += fq[i - 1] - fw;
                fq[i - 1] = fw;
            }
            fw = 0f64;
            for i in (2..=jz).rev() {
                fw += fq[i];
            }
            if ih == 0 {
                y[0] = fq[0];
                y[1] = fq[1];
                y[2] = fw;
            } else {
                y[0] = -fq[0];
                y[1] = -fq[1];
                y[2] = -fw;
            }
        }
        _ => {}
    }
    n & 7
}

// ───────────────────────── rem_pio2 ────────────────────────────
// origin: FreeBSD /usr/src/lib/msun/src/e_rem_pio2.c (optimized by Bruce D. Evans)
// ====================================================
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
// ====================================================

const EPS: f64 = 2.2204460492503131e-16;
const TO_INT: f64 = 1.5 / EPS;
/// 53 bits of 2/pi
const INV_PIO2: f64 = 6.36619772367581382433e-01; /* 0x3FE45F30, 0x6DC9C883 */
/// first 33 bits of pi/2
const PIO2_1: f64 = 1.57079632673412561417e+00; /* 0x3FF921FB, 0x54400000 */
/// pi/2 - PIO2_1
const PIO2_1T: f64 = 6.07710050650619224932e-11; /* 0x3DD0B461, 0x1A626331 */
/// second 33 bits of pi/2
const PIO2_2: f64 = 6.07710050630396597660e-11; /* 0x3DD0B461, 0x1A600000 */
/// pi/2 - (PIO2_1+PIO2_2)
const PIO2_2T: f64 = 2.02226624879595063154e-21; /* 0x3BA3198A, 0x2E037073 */
/// third 33 bits of pi/2
const PIO2_3: f64 = 2.02226624871116645580e-21; /* 0x3BA3198A, 0x2E000000 */
/// pi/2 - (PIO2_1+PIO2_2+PIO2_3)
const PIO2_3T: f64 = 8.47842766036889956997e-32; /* 0x397B839A, 0x252049C1 */

/// Remainder of x rem pi/2 in (n, y0, y1). Caller handles |x| ~<= pi/4.
fn rem_pio2(x: f64) -> (i32, f64, f64) {
    let x1p24 = f64::from_bits(0x4170000000000000); // 2^24

    let sign = (f64::to_bits(x) >> 63) as i32;
    let ix = (f64::to_bits(x) >> 32) as u32 & 0x7fffffff;

    fn medium(x: f64, ix: u32) -> (i32, f64, f64) {
        /* rint(x/(pi/2)), assume round-to-nearest. */
        let tmp = x * INV_PIO2 + TO_INT;
        let f_n = tmp - TO_INT;
        let n = f_n as i32;
        let mut r = x - f_n * PIO2_1;
        let mut w = f_n * PIO2_1T; /* 1st round, good to 85 bits */
        let mut y0 = r - w;
        let ui = f64::to_bits(y0);
        let ey = (ui >> 52) as i32 & 0x7ff;
        let ex = (ix >> 20) as i32;
        if ex - ey > 16 {
            /* 2nd round, good to 118 bits */
            let t = r;
            w = f_n * PIO2_2;
            r = t - w;
            w = f_n * PIO2_2T - ((t - r) - w);
            y0 = r - w;
            let ey = (f64::to_bits(y0) >> 52) as i32 & 0x7ff;
            if ex - ey > 49 {
                /* 3rd round, good to 151 bits, covers all cases */
                let t = r;
                w = f_n * PIO2_3;
                r = t - w;
                w = f_n * PIO2_3T - ((t - r) - w);
                y0 = r - w;
            }
        }
        let y1 = (r - y0) - w;
        (n, y0, y1)
    }

    if ix <= 0x400f6a7a {
        /* |x| ~<= 5pi/4 */
        if (ix & 0xfffff) == 0x921fb {
            /* |x| ~= pi/2 or 2pi/2 */
            return medium(x, ix); /* cancellation -- use medium case */
        }
        if ix <= 0x4002d97c {
            /* |x| ~<= 3pi/4 */
            if sign == 0 {
                let z = x - PIO2_1; /* one round good to 85 bits */
                let y0 = z - PIO2_1T;
                let y1 = (z - y0) - PIO2_1T;
                return (1, y0, y1);
            } else {
                let z = x + PIO2_1;
                let y0 = z + PIO2_1T;
                let y1 = (z - y0) + PIO2_1T;
                return (-1, y0, y1);
            }
        } else if sign == 0 {
            let z = x - 2.0 * PIO2_1;
            let y0 = z - 2.0 * PIO2_1T;
            let y1 = (z - y0) - 2.0 * PIO2_1T;
            return (2, y0, y1);
        } else {
            let z = x + 2.0 * PIO2_1;
            let y0 = z + 2.0 * PIO2_1T;
            let y1 = (z - y0) + 2.0 * PIO2_1T;
            return (-2, y0, y1);
        }
    }
    if ix <= 0x401c463b {
        /* |x| ~<= 9pi/4 */
        if ix <= 0x4015fdbc {
            /* |x| ~<= 7pi/4 */
            if ix == 0x4012d97c {
                /* |x| ~= 3pi/2 */
                return medium(x, ix);
            }
            if sign == 0 {
                let z = x - 3.0 * PIO2_1;
                let y0 = z - 3.0 * PIO2_1T;
                let y1 = (z - y0) - 3.0 * PIO2_1T;
                return (3, y0, y1);
            } else {
                let z = x + 3.0 * PIO2_1;
                let y0 = z + 3.0 * PIO2_1T;
                let y1 = (z - y0) + 3.0 * PIO2_1T;
                return (-3, y0, y1);
            }
        } else {
            if ix == 0x401921fb {
                /* |x| ~= 4pi/2 */
                return medium(x, ix);
            }
            if sign == 0 {
                let z = x - 4.0 * PIO2_1;
                let y0 = z - 4.0 * PIO2_1T;
                let y1 = (z - y0) - 4.0 * PIO2_1T;
                return (4, y0, y1);
            } else {
                let z = x + 4.0 * PIO2_1;
                let y0 = z + 4.0 * PIO2_1T;
                let y1 = (z - y0) + 4.0 * PIO2_1T;
                return (-4, y0, y1);
            }
        }
    }
    if ix < 0x413921fb {
        /* |x| ~< 2^20*(pi/2), medium size */
        return medium(x, ix);
    }
    /* all other (large) arguments */
    if ix >= 0x7ff00000 {
        /* x is inf or NaN */
        let y0 = x - x;
        let y1 = y0;
        return (0, y0, y1);
    }
    /* set z = scalbn(|x|,-ilogb(x)+23) */
    let mut ui = f64::to_bits(x);
    ui &= (!1u64) >> 12;
    ui |= (0x3ff + 23) << 52;
    let mut z = f64::from_bits(ui);
    let mut tx = [0.0; 3];
    for i in 0..2 {
        tx[i] = z as i32 as f64;
        z = (z - tx[i]) * x1p24;
    }
    tx[2] = z;
    /* skip zero terms, first term is non-zero */
    let mut i = 2;
    while i != 0 && tx[i] == 0.0 {
        i -= 1;
    }
    let mut ty = [0.0; 3];
    let n = rem_pio2_large(&tx[..=i], &mut ty, ((ix as i32) >> 20) - (0x3ff + 23), 1);
    if sign != 0 {
        return (-n, -ty[0], -ty[1]);
    }
    (n, ty[0], ty[1])
}

include!("libm_p2.rs");
include!("libm_p3.rs");
include!("libm_p4.rs");
