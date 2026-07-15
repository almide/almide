//! WASM port of the vendored musl-libm trig (sin/cos/tan).
//!
//! This is the WASM-emit twin of `runtime/rs/src/libm.rs` (the NATIVE vendored
//! reference). It compiles the SAME algorithm — same coefficients, same branch
//! structure, same bit manipulations — into hand-written WASM so that
//! `math.sin/cos/tan` are **bit-identical native ↔ wasm** AND deterministic.
//!
//! Provenance: `libm` crate v0.2.16 (rust-lang/compiler-builtins commit
//! `dfd2203a4d6110820ad7bb65cafe1bf331a03a3d`), itself a port of FreeBSD `msun`
//! (Sun fdlibm). Dual MIT/Apache-2.0. The published `S*`/`C*`/`T`/`PIO2*` numbers
//! are upstream constants, not magic numbers — each carries its native-file
//! cross-reference.
//!
//! Layout: the 2/pi table (`IPIO2`, 690×i32) and the extended-precision `PIO2`
//! table (8×f64) are embedded in the front protected data region by
//! `WasmEmitter::embed_libm_tables`; the runtime functions read them by the
//! absolute addresses recorded in `LibmTableOffsets`. Per-call working arrays for
//! `rem_pio2_large` (`iq`,`f`,`q`,`fq` and the `tx`/`ty` triples) are
//! bump-allocated via `rt.alloc`.
//!
//! Functions emitted (mirror the native ones 1:1):
//!   __libm_floor, __libm_scalbn, __libm_rem_pio2_large, __libm_rem_pio2,
//!   __libm_k_sin, __libm_k_cos, __libm_k_tan, __math_sin, __math_cos, __math_tan.

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::ValType;
use super::TrackedFunction as Function;

// ─────────────────────────── tables ───────────────────────────

pub(super) const IPIO2: [i32; 690] = [
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

/// Extended-precision pi/2 split (libm `rem_pio2_large` `PIO2[0..8]`), embedded
/// as 8×f64 in the front data region.
pub(super) const PIO2: [f64; 8] = [
    1.57079625129699707031e+00, /* 0x3FF921FB, 0x40000000 */
    7.54978941586159635335e-08, /* 0x3E74442D, 0x00000000 */
    5.39030252995776476554e-15, /* 0x3CF84698, 0x80000000 */
    3.28200341580791294123e-22, /* 0x3B78CC51, 0x60000000 */
    1.27065575308067607349e-29, /* 0x39F01B83, 0x80000000 */
    1.22933308981111328932e-36, /* 0x387A2520, 0x40000000 */
    2.73370053816464559624e-44, /* 0x36E38222, 0x80000000 */
    2.16741683877804819444e-51, /* 0x3569F31D, 0x00000000 */
];

// libm `INIT_JK = [3, 4, 4, 6]` (the per-precision jk seed) is inlined directly
// into `compile_rem_pio2_large` as an `if_i32` chain on `prec` rather than a data
// table, since only `prec == 1` (double, jk = 4) is ever used by trig.

/// Absolute data addresses of the embedded libm tables (set by `embed_libm_tables`).
#[derive(Default, Clone, Copy)]
pub struct LibmTableOffsets {
    /// Address of `IPIO2[0]` (i32 little-endian array).
    pub ipio2_base: u32,
    /// Address of `PIO2[0]` (f64 little-endian array, 8-byte aligned).
    pub pio2_base: u32,
}

/// Function indices for the vendored-libm runtime (trig + exp/log/pow).
#[derive(Default)]
pub struct LibmRuntime {
    pub floor: u32,            // __libm_floor(x: f64) -> f64
    pub scalbn: u32,          // __libm_scalbn(x: f64, n: i32) -> f64
    pub rem_pio2_large: u32,  // __libm_rem_pio2_large(x_ptr, nx, y_ptr, e0, prec) -> i32(n&7)
    pub rem_pio2: u32,        // __libm_rem_pio2(x: f64, y_ptr: i32) -> i32(n)  (writes y[0],y[1])
    pub k_sin: u32,           // __libm_k_sin(x: f64, y: f64, iy: i32) -> f64
    pub k_cos: u32,           // __libm_k_cos(x: f64, y: f64) -> f64
    pub k_tan: u32,           // __libm_k_tan(x: f64, y: f64, odd: i32) -> f64
    pub exp: u32,             // __libm_exp(x: f64) -> f64
    pub log: u32,             // __libm_log(x: f64) -> f64
    pub log2: u32,            // __libm_log2(x: f64) -> f64
    pub log10: u32,           // __libm_log10(x: f64) -> f64
    pub pow: u32,             // __libm_pow(x: f64, y: f64) -> f64
    pub expm1: u32,           // __libm_expm1(x: f64) -> f64
}

// ───────────────────── named layout constants ──────────────────

/// rem_pio2_large per-call scratch block (one bump-alloc). Holds four 20-element
/// working arrays: `iq`(i32) `f`,`q`,`fq`(f64). Indices are byte offsets.
const ARR_N: i32 = 20;            // libm fixed-size working arrays
const IQ_OFF: i32 = 0;            // iq: [i32; 20]
const F_OFF: i32 = IQ_OFF + ARR_N * 4;   // f:  [f64; 20]   (80)
const Q_OFF: i32 = F_OFF + ARR_N * 8;    // q:  [f64; 20]   (240)
const FQ_OFF: i32 = Q_OFF + ARR_N * 8;   // fq: [f64; 20]   (400)
const SCRATCH_BYTES: i32 = FQ_OFF + ARR_N * 8; // 560

/// rem_pio2 owns the `tx[3]`/`ty[3]` triples (for the large path) + the caller's
/// `y[2]` it writes through. tx/ty are f64 triples allocated here.
const TX_BYTES: i32 = 3 * 8;
const TY_BYTES: i32 = 3 * 8;

// IEEE-754 binary64 helpers as named consts (so the bit math reads as IEEE-754).
const HI_SHIFT: i64 = 32;          // high word = bits >> 32


pub fn register(emitter: &mut WasmEmitter) {
    let f64_f64 = emitter.register_type(vec![ValType::F64], vec![ValType::F64]);
    let f64i32_f64 = emitter.register_type(vec![ValType::F64, ValType::I32], vec![ValType::F64]);
    let f64f64i32_f64 = emitter.register_type(vec![ValType::F64, ValType::F64, ValType::I32], vec![ValType::F64]);
    let f64f64_f64 = emitter.register_type(vec![ValType::F64, ValType::F64], vec![ValType::F64]);
    // rem_pio2_large(x_ptr, nx, y_ptr, e0, prec) -> i32
    let rpl = emitter.register_type(vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32], vec![ValType::I32]);
    // rem_pio2(x, y_ptr) -> i32
    let rp = emitter.register_type(vec![ValType::F64, ValType::I32], vec![ValType::I32]);

    emitter.rt.libm.floor = emitter.register_func("__libm_floor", f64_f64);
    emitter.rt.libm.scalbn = emitter.register_func("__libm_scalbn", f64i32_f64);
    emitter.rt.libm.rem_pio2_large = emitter.register_func("__libm_rem_pio2_large", rpl);
    emitter.rt.libm.rem_pio2 = emitter.register_func("__libm_rem_pio2", rp);
    emitter.rt.libm.k_sin = emitter.register_func("__libm_k_sin", f64f64i32_f64);
    emitter.rt.libm.k_cos = emitter.register_func("__libm_k_cos", f64f64_f64);
    emitter.rt.libm.k_tan = emitter.register_func("__libm_k_tan", f64f64i32_f64);
    emitter.rt.libm.exp = emitter.register_func("__libm_exp", f64_f64);
    emitter.rt.libm.log = emitter.register_func("__libm_log", f64_f64);
    emitter.rt.libm.log2 = emitter.register_func("__libm_log2", f64_f64);
    emitter.rt.libm.log10 = emitter.register_func("__libm_log10", f64_f64);
    emitter.rt.libm.pow = emitter.register_func("__libm_pow", f64f64_f64);
    emitter.rt.libm.expm1 = emitter.register_func("__libm_expm1", f64_f64);
}

/// Compile the libm helper bodies (floor/scalbn/rem_pio2_large/rem_pio2/kernels).
/// Registration + compile order MUST match (see `compile_runtime`). The top-level
/// `__math_sin/cos/tan` bodies are compiled by `rt_numeric` in the existing slots
/// and just dispatch to these (see `compile_math_sin` etc.).
pub fn compile_helpers(emitter: &mut WasmEmitter) {
    compile_floor(emitter);
    compile_scalbn(emitter);
    compile_rem_pio2_large(emitter);
    compile_rem_pio2(emitter);
    compile_k_sin(emitter);
    compile_k_cos(emitter);
    compile_k_tan(emitter);
    compile_exp(emitter);
    compile_log(emitter);
    compile_log2(emitter);
    compile_log10(emitter);
    compile_pow(emitter);
    compile_expm1(emitter);
}

// ───────────────────────── __libm_floor ───────────────────────
// Faithful port of libm::floor (generic path). Round toward -inf.
fn compile_floor(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.floor];
    // param 0=f64 x. locals: 1=i64 ui, 2=i32 e, 3=i64 m
    let mut f = Function::new([(1, ValType::I64), (1, ValType::I32), (1, ValType::I64)]);
    wasm!(f, {
        local_get(0); i64_reinterpret_f64; local_set(1);              // ui = x.to_bits()
        // e = ((ui >> 52) & 0x7ff) - 0x3ff
        local_get(1); i64_const(52); i64_shr_u; i64_const(0x7ff); i64_and;
        i32_wrap_i64; i32_const(0x3ff); i32_sub; local_set(2);
        // if e >= 52 { return x }
        local_get(2); i32_const(52); i32_ge_s;
        if_empty; local_get(0); return_; end;
        // if e >= 0
        local_get(2); i32_const(0); i32_ge_s;
        if_f64;
            // m = 0x000f_ffff_ffff_ffff >> e
            i64_const(0x000f_ffff_ffff_ffff); local_get(2); i64_extend_i32_s; i64_shr_u; local_set(3);
            // if ui & m == 0 { return x }
            local_get(1); local_get(3); i64_and; i64_eqz;
            if_empty; local_get(0); return_; end;
            // if (ui >> 63) != 0 { ui += m }
            local_get(1); i64_const(63); i64_shr_u; i64_eqz;
            if_empty; else_;
                local_get(1); local_get(3); i64_add; local_set(1);
            end;
            // ui &= !m
            local_get(1); local_get(3); i64_const(-1); i64_xor; i64_and; local_set(1);
            local_get(1); f64_reinterpret_i64;
        else_;
            // |x| < 1
            // if (ui >> 63) == 0 { 0.0 } else if (ui << 1) != 0 { -1.0 } else { x }
            local_get(1); i64_const(63); i64_shr_u; i64_eqz;
            if_f64;
                f64_const(0.0);
            else_;
                local_get(1); i64_const(1); i64_shl; i64_eqz;
                if_f64; local_get(0); else_; f64_const(-1.0); end;
            end;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.floor, type_idx, f));
}

// ──────────────────────── __libm_scalbn ───────────────────────
// Faithful port of libm::scalbn (generic, f64). x * 2^n with prescaling.
fn compile_scalbn(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.scalbn];
    // params: 0=f64 x, 1=i32 n.
    // f64 consts (built once): 2=f_exp_max(2^1023), 3=f_exp_min(2^-1022),
    //   4=mul(f_exp_min*2^53). i32: keep n in param 1.
    let mut f = Function::new([(3, ValType::F64)]);
    // f_exp_max = 2^1023 = bits (1023<<1)<<52 = 2046<<52
    wasm!(f, {
        i64_const((2046i64) << 52); f64_reinterpret_i64; local_set(2);
        i64_const(1i64 << 52); f64_reinterpret_i64; local_set(3);     // f_exp_min = 2^-1022
        // mul = f_exp_min * 2^53 ; 2^53 bits = (53+1023)<<52 = 1076<<52
        local_get(3); i64_const((1076i64) << 52); f64_reinterpret_i64; f64_mul; local_set(4);
    });
    // if n > 1023
    wasm!(f, {
        local_get(1); i32_const(1023); i32_gt_s;
        if_empty;
            local_get(0); local_get(2); f64_mul; local_set(0);
            local_get(1); i32_const(1023); i32_sub; local_set(1);
            local_get(1); i32_const(1023); i32_gt_s;
            if_empty;
                local_get(0); local_get(2); f64_mul; local_set(0);
                local_get(1); i32_const(1023); i32_sub; local_set(1);
                local_get(1); i32_const(1023); i32_gt_s;
                if_empty; i32_const(1023); local_set(1); end;
            end;
        else_;
            // if n < -1022
            local_get(1); i32_const(-1022); i32_lt_s;
            if_empty;
                // add = -(-1022) - 53 = 1022 - 53 = 969
                local_get(0); local_get(4); f64_mul; local_set(0);
                local_get(1); i32_const(969); i32_add; local_set(1);
                local_get(1); i32_const(-1022); i32_lt_s;
                if_empty;
                    local_get(0); local_get(4); f64_mul; local_set(0);
                    local_get(1); i32_const(969); i32_add; local_set(1);
                    local_get(1); i32_const(-1022); i32_lt_s;
                    if_empty; i32_const(-1022); local_set(1); end;
                end;
            end;
        end;
    });
    // scale = bits((1023 + n) << 52) ; return x * scale
    wasm!(f, {
        local_get(0);
        i32_const(1023); local_get(1); i32_add; i64_extend_i32_s; i64_const(52); i64_shl;
        f64_reinterpret_i64;
        f64_mul;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.scalbn, type_idx, f));
}

// ───────────── kernel coefficients (libm k_sin/k_cos/k_tan) ────
const S1: f64 = -1.66666666666666324348e-01;
const S2: f64 = 8.33333333332248946124e-03;
const S3: f64 = -1.98412698298579493134e-04;
const S4: f64 = 2.75573137070700676789e-06;
const S5: f64 = -2.50507602534068634195e-08;
const S6: f64 = 1.58969099521155010221e-10;

const C1: f64 = 4.16666666666666019037e-02;
const C2: f64 = -1.38888888888741095749e-03;
const C3: f64 = 2.48015872894767294178e-05;
const C4: f64 = -2.75573143513906633035e-07;
const C5: f64 = 2.08757232129817482790e-09;
const C6: f64 = -1.13596475577881948265e-11;

// k_tan polynomial T[0..13]
const T0: f64 = 3.33333333333334091986e-01;
const T1: f64 = 1.33333333333201242699e-01;
const T2: f64 = 5.39682539762260521377e-02;
const T3: f64 = 2.18694882948595424599e-02;
const T4: f64 = 8.86323982359930005737e-03;
const T5: f64 = 3.59207910759131235356e-03;
const T6: f64 = 1.45620945432529025516e-03;
const T7: f64 = 5.88041240820264096874e-04;
const T8: f64 = 2.46463134818469906812e-04;
const T9: f64 = 7.81794442939557092300e-05;
const T10: f64 = 7.14072491382608190305e-05;
const T11: f64 = -1.85586374855275456654e-05;
const T12: f64 = 2.59073051863633712884e-05;
const PIO4: f64 = 7.85398163397448278999e-01;
const PIO4_LO: f64 = 3.06161699786838301793e-17;

// ───────────────────────── __libm_k_sin ───────────────────────
// k_sin(x, y, iy): z=x*x; w=z*z; r=S2+z*(S3+z*S4)+z*w*(S5+z*S6); v=z*x;
//   iy==0 -> x + v*(S1+z*r)  else  x - ((z*(0.5*y - v*r) - y) - v*S1)
fn compile_k_sin(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.k_sin];
    // params: 0=x, 1=y, 2=iy. locals: 3=z, 4=w, 5=r, 6=v
    let mut f = Function::new([(4, ValType::F64)]);
    wasm!(f, {
        local_get(0); local_get(0); f64_mul; local_set(3);           // z = x*x
        local_get(3); local_get(3); f64_mul; local_set(4);           // w = z*z
        // r = S2 + z*(S3 + z*S4) + z*w*(S5 + z*S6)
        f64_const(S2);
        local_get(3); f64_const(S3); local_get(3); f64_const(S4); f64_mul; f64_add; f64_mul;
        f64_add;
        local_get(3); local_get(4); f64_mul; f64_const(S5); local_get(3); f64_const(S6); f64_mul; f64_add; f64_mul;
        f64_add; local_set(5);
        local_get(3); local_get(0); f64_mul; local_set(6);           // v = z*x
        // if iy == 0
        local_get(2); i32_eqz;
        if_f64;
            // x + v*(S1 + z*r)
            local_get(0);
            local_get(6); f64_const(S1); local_get(3); local_get(5); f64_mul; f64_add; f64_mul;
            f64_add;
        else_;
            // x - ((z*(0.5*y - v*r) - y) - v*S1)
            local_get(0);
            local_get(3); f64_const(0.5); local_get(1); f64_mul; local_get(6); local_get(5); f64_mul; f64_sub; f64_mul;
            local_get(1); f64_sub;
            local_get(6); f64_const(S1); f64_mul; f64_sub;
            f64_sub;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.k_sin, type_idx, f));
}

// ───────────────────────── __libm_k_cos ───────────────────────
// k_cos(x, y): z=x*x; w=z*z; r=z*(C1+z*(C2+z*C3))+w*w*(C4+z*(C5+z*C6));
//   hz=0.5*z; w=1-hz; return w + (((1-w)-hz) + (z*r - x*y))
fn compile_k_cos(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.k_cos];
    // params: 0=x, 1=y. locals: 2=z, 3=w(z*z then reused as 1-hz), 4=r, 5=hz, 6=w2(=1-hz)
    let mut f = Function::new([(5, ValType::F64)]);
    wasm!(f, {
        local_get(0); local_get(0); f64_mul; local_set(2);           // z = x*x
        local_get(2); local_get(2); f64_mul; local_set(3);           // w = z*z
        // r = z*(C1 + z*(C2 + z*C3)) + w*w*(C4 + z*(C5 + z*C6))
        local_get(2); f64_const(C1); local_get(2); f64_const(C2); local_get(2); f64_const(C3); f64_mul; f64_add; f64_mul; f64_add; f64_mul;
        local_get(3); local_get(3); f64_mul; f64_const(C4); local_get(2); f64_const(C5); local_get(2); f64_const(C6); f64_mul; f64_add; f64_mul; f64_add; f64_mul;
        f64_add; local_set(4);
        local_get(2); f64_const(0.5); f64_mul; local_set(5);         // hz = 0.5*z
        f64_const(1.0); local_get(5); f64_sub; local_set(6);         // w2 = 1 - hz
        // return w2 + (((1 - w2) - hz) + (z*r - x*y))
        local_get(6);
        f64_const(1.0); local_get(6); f64_sub; local_get(5); f64_sub;
        local_get(2); local_get(4); f64_mul; local_get(0); local_get(1); f64_mul; f64_sub;
        f64_add;
        f64_add;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.k_cos, type_idx, f));
}

// ───────────────────────── __libm_k_tan ───────────────────────
// Faithful port of libm k_tan(x, y, odd). See runtime/rs/src/libm.rs.
fn compile_k_tan(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.k_tan];
    // params: 0=f64 x, 1=f64 y, 2=i32 odd
    // f64 locals: 3=z, 4=w, 5=r, 6=v, 7=s, 8=a, 9=a0, 10=w0, 11=(reserved)
    // i32 locals: 12=hx, 13=big, 14=sign
    let mut f = Function::new([(9, ValType::F64), (3, ValType::I32)]);
    const X: u32 = 0; const Y: u32 = 1; const ODD: u32 = 2;
    const Z: u32 = 3; const W: u32 = 4; const R: u32 = 5; const V: u32 = 6;
    const S: u32 = 7; const A: u32 = 8; const A0: u32 = 9; const W0: u32 = 10;
    const HX: u32 = 12; const BIG: u32 = 13; const SIGN: u32 = 14;
    wasm!(f, {
        // hx = (to_bits(x) >> 32) as u32
        local_get(X); i64_reinterpret_f64; i64_const(HI_SHIFT); i64_shr_u; i32_wrap_i64; local_set(HX);
        // big = (hx & 0x7fffffff) >= 0x3FE59428
        local_get(HX); i32_const(0x7fffffff); i32_and; i32_const(0x3FE59428); i32_ge_u; local_set(BIG);
        // sign = hx >> 31  (logical)
        local_get(HX); i32_const(31); i32_shr_u; local_set(SIGN);
        // if big { if sign { x=-x; y=-y } x=(PIO4-x)+(PIO4_LO-y); y=0 }
        local_get(BIG);
        if_empty;
            local_get(SIGN);
            if_empty;
                local_get(X); f64_neg; local_set(X);
                local_get(Y); f64_neg; local_set(Y);
            end;
            f64_const(PIO4); local_get(X); f64_sub;
            f64_const(PIO4_LO); local_get(Y); f64_sub;
            f64_add; local_set(X);
            f64_const(0.0); local_set(Y);
        end;
        // z = x*x; w = z*z
        local_get(X); local_get(X); f64_mul; local_set(Z);
        local_get(Z); local_get(Z); f64_mul; local_set(W);
        // r = T1 + w*(T3 + w*(T5 + w*(T7 + w*(T9 + w*T11))))
        f64_const(T1);
        local_get(W); f64_const(T3);
        local_get(W); f64_const(T5);
        local_get(W); f64_const(T7);
        local_get(W); f64_const(T9);
        local_get(W); f64_const(T11); f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add; local_set(R);
        // v = z*(T2 + w*(T4 + w*(T6 + w*(T8 + w*(T10 + w*T12)))))
        local_get(Z);
        f64_const(T2);
        local_get(W); f64_const(T4);
        local_get(W); f64_const(T6);
        local_get(W); f64_const(T8);
        local_get(W); f64_const(T10);
        local_get(W); f64_const(T12); f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; local_set(V);
        // s = z*x
        local_get(Z); local_get(X); f64_mul; local_set(S);
        // r = y + z*(s*(r+v) + y) + s*T0
        local_get(Y);
        local_get(Z); local_get(S); local_get(R); local_get(V); f64_add; f64_mul; local_get(Y); f64_add; f64_mul;
        f64_add;
        local_get(S); f64_const(T0); f64_mul;
        f64_add; local_set(R);
        // w = x + r
        local_get(X); local_get(R); f64_add; local_set(W);
        // if big { ... return ±v }
        local_get(BIG);
        if_empty;
            // s = 1 - 2*odd
            f64_const(1.0); f64_const(2.0); local_get(ODD); f64_convert_i32_s; f64_mul; f64_sub; local_set(S);
            // v = s - 2*(x + (r - w*w/(w+s)))
            local_get(S);
            f64_const(2.0);
            local_get(X);
            local_get(R); local_get(W); local_get(W); f64_mul; local_get(W); local_get(S); f64_add; f64_div; f64_sub;
            f64_add;
            f64_mul;
            f64_sub; local_set(V);
            // return sign ? -v : v
            local_get(SIGN);
            if_f64; local_get(V); f64_neg; else_; local_get(V); end;
            return_;
        end;
        // if odd == 0 { return w }
        local_get(ODD); i32_eqz;
        if_empty; local_get(W); return_; end;
        // w0 = zero_low_word(w)
        local_get(W); i64_reinterpret_f64; i64_const(-4294967296); i64_and; f64_reinterpret_i64; local_set(W0); // !0xFFFFFFFF == 0xFFFFFFFF00000000 = -4294967296
        // v = r - (w0 - x)
        local_get(R); local_get(W0); local_get(X); f64_sub; f64_sub; local_set(V);
        // a = -1.0 / w
        f64_const(-1.0); local_get(W); f64_div; local_set(A);
        // a0 = zero_low_word(a)
        local_get(A); i64_reinterpret_f64; i64_const(-4294967296); i64_and; f64_reinterpret_i64; local_set(A0);
        // return a0 + a*(1 + a0*w0 + a0*v)
        local_get(A0);
        local_get(A);
        f64_const(1.0); local_get(A0); local_get(W0); f64_mul; f64_add; local_get(A0); local_get(V); f64_mul; f64_add;
        f64_mul;
        f64_add;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.k_tan, type_idx, f));
}

include!("rt_libm_p2.rs");
include!("rt_libm_p3.rs");
include!("rt_libm_p4.rs");
