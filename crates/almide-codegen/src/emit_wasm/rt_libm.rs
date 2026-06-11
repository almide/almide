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

// ───────────── rem_pio2 medium-case + small-case helpers ───────
// rem_pio2 constants (libm).
const INV_PIO2: f64 = 6.36619772367581382433e-01;
const PIO2_1: f64 = 1.57079632673412561417e+00;
const PIO2_1T: f64 = 6.07710050650619224932e-11;
const PIO2_2: f64 = 6.07710050630396597660e-11;
const PIO2_2T: f64 = 2.02226624879595063154e-21;
const PIO2_3: f64 = 2.02226624871116645580e-21;
const PIO2_3T: f64 = 8.47842766036889956997e-32;
const EPS: f64 = 2.2204460492503131e-16;
// TO_INT = 1.5 / EPS  (folded at gen time to match native exactly: same Rust expr)
const TO_INT: f64 = 1.5 / EPS;

// __libm_rem_pio2 local map. y_ptr in param 1.
const RP_X: u32 = 0;
const RP_YP: u32 = 1;
const RP_SIGN: u32 = 2;     // i32
const RP_IX: u32 = 3;       // i32
const RP_Z: u32 = 4;        // f64
const RP_Y0: u32 = 5;       // f64
const RP_Y1: u32 = 6;       // f64
const RP_N: u32 = 7;        // i32 result accumulator
// slot 8: f64 scratch (reserved for index parity; not read)
const RP_FN: u32 = 9;       // f64 f_n
const RP_R: u32 = 10;       // f64 r
const RP_W: u32 = 11;       // f64 w
const RP_T: u32 = 12;       // f64 t
const RP_EX: u32 = 13;      // i32 ex
const RP_EY: u32 = 14;      // i32 ey
const RP_UI: u32 = 15;      // i64 ui
const RP_TXP: u32 = 16;     // i32 tx ptr
const RP_TYP: u32 = 17;     // i32 ty ptr
// slot 18: i64 z bits (reserved for index parity; not read)
const RP_I: u32 = 19;       // i32 loop i
// slot 20: i32 jx (reserved for index parity; not read)

/// Emit the libm `medium` case: rint(x/(pi/2)), three rounds; writes y0/y1 to
/// `y_ptr` and leaves `n` (i32) in `RP_N`. Reads x from `RP_X`, ix from `RP_IX`.
fn emit_medium(f: &mut Function) {
    wasm!(f, {
        // f_n = (x*INV_PIO2 + TO_INT) - TO_INT
        local_get(RP_X); f64_const(INV_PIO2); f64_mul; f64_const(TO_INT); f64_add;
        f64_const(TO_INT); f64_sub; local_set(RP_FN);
        // n = f_n as i32
        local_get(RP_FN); i64_trunc_f64_s; i32_wrap_i64; local_set(RP_N);
        // r = x - f_n*PIO2_1
        local_get(RP_X); local_get(RP_FN); f64_const(PIO2_1); f64_mul; f64_sub; local_set(RP_R);
        // w = f_n*PIO2_1T
        local_get(RP_FN); f64_const(PIO2_1T); f64_mul; local_set(RP_W);
        // y0 = r - w
        local_get(RP_R); local_get(RP_W); f64_sub; local_set(RP_Y0);
        // ey = (to_bits(y0) >> 52) & 0x7ff ; ex = ix >> 20
        local_get(RP_Y0); i64_reinterpret_f64; i64_const(52); i64_shr_u; i32_wrap_i64; i32_const(0x7ff); i32_and; local_set(RP_EY);
        local_get(RP_IX); i32_const(20); i32_shr_u; local_set(RP_EX);
        // if ex - ey > 16
        local_get(RP_EX); local_get(RP_EY); i32_sub; i32_const(16); i32_gt_s;
        if_empty;
            // t = r; w = f_n*PIO2_2; r = t - w; w = f_n*PIO2_2T - ((t - r) - w); y0 = r - w
            local_get(RP_R); local_set(RP_T);
            local_get(RP_FN); f64_const(PIO2_2); f64_mul; local_set(RP_W);
            local_get(RP_T); local_get(RP_W); f64_sub; local_set(RP_R);
            local_get(RP_FN); f64_const(PIO2_2T); f64_mul;
            local_get(RP_T); local_get(RP_R); f64_sub; local_get(RP_W); f64_sub;
            f64_sub; local_set(RP_W);
            local_get(RP_R); local_get(RP_W); f64_sub; local_set(RP_Y0);
            // ey = (to_bits(y0) >> 52) & 0x7ff
            local_get(RP_Y0); i64_reinterpret_f64; i64_const(52); i64_shr_u; i32_wrap_i64; i32_const(0x7ff); i32_and; local_set(RP_EY);
            // if ex - ey > 49
            local_get(RP_EX); local_get(RP_EY); i32_sub; i32_const(49); i32_gt_s;
            if_empty;
                local_get(RP_R); local_set(RP_T);
                local_get(RP_FN); f64_const(PIO2_3); f64_mul; local_set(RP_W);
                local_get(RP_T); local_get(RP_W); f64_sub; local_set(RP_R);
                local_get(RP_FN); f64_const(PIO2_3T); f64_mul;
                local_get(RP_T); local_get(RP_R); f64_sub; local_get(RP_W); f64_sub;
                f64_sub; local_set(RP_W);
                local_get(RP_R); local_get(RP_W); f64_sub; local_set(RP_Y0);
            end;
        end;
        // y1 = (r - y0) - w
        local_get(RP_R); local_get(RP_Y0); f64_sub; local_get(RP_W); f64_sub; local_set(RP_Y1);
    });
    emit_store_y(f);
}

/// Store RP_Y0 / RP_Y1 to y_ptr[0], y_ptr[1].
fn emit_store_y(f: &mut Function) {
    wasm!(f, {
        local_get(RP_YP); local_get(RP_Y0); f64_store(0);
        local_get(RP_YP); local_get(RP_Y1); f64_store(8);
    });
}

/// Emit a small-case branch `(n, y0, y1)` with sign-dependent ± (libm pattern):
///   pos: z = x - k*PIO2_1; y0 = z - k*PIO2_1T; y1 = (z - y0) - k*PIO2_1T; n =  k
///   neg: z = x + k*PIO2_1; y0 = z + k*PIO2_1T; y1 = (z - y0) + k*PIO2_1T; n = -k
/// `k` is the integer multiple; `kf` its f64 form (1.0/2.0/3.0/4.0).
fn emit_small(f: &mut Function, k: i32) {
    let kf = k as f64;
    wasm!(f, {
        local_get(RP_SIGN); i32_eqz;
        if_empty;
            // positive
            local_get(RP_X); f64_const(kf); f64_const(PIO2_1); f64_mul; f64_sub; local_set(RP_Z);
            local_get(RP_Z); f64_const(kf); f64_const(PIO2_1T); f64_mul; f64_sub; local_set(RP_Y0);
            local_get(RP_Z); local_get(RP_Y0); f64_sub; f64_const(kf); f64_const(PIO2_1T); f64_mul; f64_sub; local_set(RP_Y1);
            i32_const(k); local_set(RP_N);
        else_;
            local_get(RP_X); f64_const(kf); f64_const(PIO2_1); f64_mul; f64_add; local_set(RP_Z);
            local_get(RP_Z); f64_const(kf); f64_const(PIO2_1T); f64_mul; f64_add; local_set(RP_Y0);
            local_get(RP_Z); local_get(RP_Y0); f64_sub; f64_const(kf); f64_const(PIO2_1T); f64_mul; f64_add; local_set(RP_Y1);
            i32_const(-k); local_set(RP_N);
        end;
    });
    emit_store_y(f);
}

// ──────────────────────── __libm_rem_pio2 ─────────────────────
// __libm_rem_pio2(x: f64, y_ptr: i32) -> i32(n). Writes y[0],y[1] to y_ptr.
fn compile_rem_pio2(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.rem_pio2];
    let alloc = emitter.rt.alloc;
    let rpl = emitter.rt.libm.rem_pio2_large;
    let x1p24_bits: i64 = 0x4170000000000000u64 as i64; // 2^24
    // locals: see RP_* (params 0,1). Need f64+i32+i64 locals.
    // f64: RP_Z..RP_T (4..12) minus the i32 ones; lay out explicitly.
    let mut f = Function::new([
        (1, ValType::I32), // 2 RP_SIGN
        (1, ValType::I32), // 3 RP_IX
        (1, ValType::F64), // 4 RP_Z
        (1, ValType::F64), // 5 RP_Y0
        (1, ValType::F64), // 6 RP_Y1
        (1, ValType::I32), // 7 RP_N
        (1, ValType::F64), // 8 RP_TMP
        (1, ValType::F64), // 9 RP_FN
        (1, ValType::F64), // 10 RP_R
        (1, ValType::F64), // 11 RP_W
        (1, ValType::F64), // 12 RP_T
        (1, ValType::I32), // 13 RP_EX
        (1, ValType::I32), // 14 RP_EY
        (1, ValType::I64), // 15 RP_UI
        (1, ValType::I32), // 16 RP_TXP
        (1, ValType::I32), // 17 RP_TYP
        (1, ValType::I64), // 18 RP_ZI (unused placeholder kept for index parity)
        (1, ValType::I32), // 19 RP_I
        (1, ValType::I32), // 20 RP_JX
    ]);
    wasm!(f, {
        // sign = (to_bits(x) >> 63) as i32
        local_get(RP_X); i64_reinterpret_f64; i64_const(63); i64_shr_u; i32_wrap_i64; local_set(RP_SIGN);
        // ix = (to_bits(x) >> 32) as u32 & 0x7fffffff
        local_get(RP_X); i64_reinterpret_f64; i64_const(HI_SHIFT); i64_shr_u; i32_wrap_i64; i32_const(0x7fffffff); i32_and; local_set(RP_IX);
    });

    // if ix <= 0x400f6a7a  (|x| ~<= 5pi/4)
    wasm!(f, { local_get(RP_IX); i32_const(0x400f6a7a); i32_le_u; if_empty; });
    // if (ix & 0xfffff) == 0x921fb { medium; return n }
    wasm!(f, { local_get(RP_IX); i32_const(0xfffff); i32_and; i32_const(0x921fb); i32_eq; if_empty; });
    emit_medium(&mut f);
    wasm!(f, { local_get(RP_N); return_; });
    wasm!(f, { end; }); // end the 0x921fb if
    // if ix <= 0x4002d97c { small(1) } else { small(2) }
    wasm!(f, { local_get(RP_IX); i32_const(0x4002d97c); i32_le_u; if_empty; });
    emit_small(&mut f, 1);
    wasm!(f, { local_get(RP_N); return_; });
    wasm!(f, { else_; });
    emit_small(&mut f, 2);
    wasm!(f, { local_get(RP_N); return_; });
    wasm!(f, { end; });   // end 3pi/4 split
    wasm!(f, { end; });   // end 5pi/4 outer

    // if ix <= 0x401c463b  (|x| ~<= 9pi/4)
    wasm!(f, { local_get(RP_IX); i32_const(0x401c463b); i32_le_u; if_empty; });
        // if ix <= 0x4015fdbc (|x| ~<= 7pi/4)
        wasm!(f, { local_get(RP_IX); i32_const(0x4015fdbc); i32_le_u; if_empty; });
            // if ix == 0x4012d97c { medium }
            wasm!(f, { local_get(RP_IX); i32_const(0x4012d97c); i32_eq; if_empty; });
            emit_medium(&mut f);
            wasm!(f, { local_get(RP_N); return_; });
            wasm!(f, { end; });
            emit_small(&mut f, 3);
            wasm!(f, { local_get(RP_N); return_; });
        wasm!(f, { else_; });
            // if ix == 0x401921fb { medium }
            wasm!(f, { local_get(RP_IX); i32_const(0x401921fb); i32_eq; if_empty; });
            emit_medium(&mut f);
            wasm!(f, { local_get(RP_N); return_; });
            wasm!(f, { end; });
            emit_small(&mut f, 4);
            wasm!(f, { local_get(RP_N); return_; });
        wasm!(f, { end; }); // end 7pi/4 split
    wasm!(f, { end; }); // end 9pi/4 outer

    // if ix < 0x413921fb { medium }
    wasm!(f, { local_get(RP_IX); i32_const(0x413921fb); i32_lt_u; if_empty; });
    emit_medium(&mut f);
    wasm!(f, { local_get(RP_N); return_; });
    wasm!(f, { end; });

    // if ix >= 0x7ff00000 { y0 = x - x; y1 = y0; return 0 }
    wasm!(f, { local_get(RP_IX); i32_const(0x7ff00000); i32_ge_u; if_empty;
        local_get(RP_X); local_get(RP_X); f64_sub; local_set(RP_Y0);
        local_get(RP_Y0); local_set(RP_Y1);
    });
    emit_store_y(&mut f);
    wasm!(f, { i32_const(0); return_; end; });

    // ── large path ──
    // alloc tx(24) + ty(24). RP_TXP = tx, RP_TYP = ty.
    wasm!(f, {
        i32_const(TX_BYTES + TY_BYTES); call(alloc); local_set(RP_TXP);
        local_get(RP_TXP); i32_const(TX_BYTES); i32_add; local_set(RP_TYP);
        // ui = to_bits(x); ui &= (!1) >> 12; ui |= (0x3ff+23) << 52; z = from_bits(ui)
        local_get(RP_X); i64_reinterpret_f64;
        i64_const(-1); i64_const(1); i64_const(-1); i64_xor; i64_and; // (!1)
        i64_const(12); i64_shr_u;
        i64_and;
        i64_const(0x3ff + 23); i64_const(52); i64_shl; i64_or;
        local_set(RP_UI);
        local_get(RP_UI); f64_reinterpret_i64; local_set(RP_Z);
        // for i in 0..2 { tx[i] = z as i32 as f64; z = (z - tx[i]) * 2^24 }
        i32_const(0); local_set(RP_I);
        block_empty; loop_empty;
            local_get(RP_I); i32_const(2); i32_ge_s; br_if(1);
            // tx[i] = (z as i32) as f64
            local_get(RP_TXP); local_get(RP_I); i32_const(8); i32_mul; i32_add;
            local_get(RP_Z); i64_trunc_f64_s; i32_wrap_i64; f64_convert_i32_s;
            f64_store(0);
            // z = (z - tx[i]) * 2^24
            local_get(RP_Z);
            local_get(RP_TXP); local_get(RP_I); i32_const(8); i32_mul; i32_add; f64_load(0);
            f64_sub;
            i64_const(x1p24_bits); f64_reinterpret_i64; f64_mul; local_set(RP_Z);
            local_get(RP_I); i32_const(1); i32_add; local_set(RP_I);
            br(0);
        end; end;
        // tx[2] = z
        local_get(RP_TXP); i32_const(16); i32_add; local_get(RP_Z); f64_store(0);
        // i = 2; while i != 0 && tx[i] == 0 { i -= 1 }
        i32_const(2); local_set(RP_I);
        block_empty; loop_empty;
            local_get(RP_I); i32_eqz; br_if(1);
            local_get(RP_TXP); local_get(RP_I); i32_const(8); i32_mul; i32_add; f64_load(0);
            f64_const(0.0); f64_ne; br_if(1);
            local_get(RP_I); i32_const(1); i32_sub; local_set(RP_I);
            br(0);
        end; end;
        // jx = i  (nx-1 form); n = rem_pio2_large(tx, i+1, ty, (ix>>20)-(0x3ff+23), 1)
        local_get(RP_TXP);
        local_get(RP_I); i32_const(1); i32_add;        // nx = i+1
        local_get(RP_TYP);
        local_get(RP_IX); i32_const(20); i32_shr_s; i32_const(0x3ff + 23); i32_sub; // e0
        i32_const(1);                                   // prec
        call(rpl); local_set(RP_N);
        // y0 = ty[0]; y1 = ty[1]
        local_get(RP_TYP); f64_load(0); local_set(RP_Y0);
        local_get(RP_TYP); f64_load(8); local_set(RP_Y1);
        // if sign != 0 { n = -n; y0 = -y0; y1 = -y1 }
        local_get(RP_SIGN);
        if_empty;
            i32_const(0); local_get(RP_N); i32_sub; local_set(RP_N);
            local_get(RP_Y0); f64_neg; local_set(RP_Y0);
            local_get(RP_Y1); f64_neg; local_set(RP_Y1);
        end;
    });
    emit_store_y(&mut f);
    wasm!(f, { local_get(RP_N); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.rem_pio2, type_idx, f));
}

// ───────────────────── __libm_rem_pio2_large ──────────────────
// __libm_rem_pio2_large(x_ptr, nx, y_ptr, e0, prec) -> i32(n & 7).
// Faithful port of libm rem_pio2_large. x_ptr/y_ptr are f64 arrays.
// Working arrays (iq/f/q/fq, 20 each) live in a single bump-allocated scratch
// block; IPIO2/PIO2 are read from the embedded data tables.
fn compile_rem_pio2_large(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.rem_pio2_large];
    let alloc = emitter.rt.alloc;
    let scalbn = emitter.rt.libm.scalbn;
    let floor = emitter.rt.libm.floor;
    // Table offsets are recorded by `embed_libm_tables`, which runs iff the
    // program uses trig (`program_uses_trig`). When trig is unused the tables are
    // NOT embedded and THIS body is dead (DCE removes it before the module is
    // emitted), so the offsets are never read at runtime — default to 0 rather
    // than panicking on the missing table.
    let tables = emitter.libm_tables.unwrap_or_default();
    let ipio2 = tables.ipio2_base as i32;
    let pio2 = tables.pio2_base as i32;
    let x1p24_bits: i64 = 0x4170000000000000u64 as i64;  // 2^24
    let x1p_24_bits: i64 = 0x3e70000000000000u64 as i64;  // 2^-24

    // params: 0=x_ptr, 1=nx, 2=y_ptr(used in emit_rpl), 3=e0, 4=prec
    const XP: u32 = 0; const NX: u32 = 1; const E0: u32 = 3; const PREC: u32 = 4;
    // i32 locals
    const SCR: u32 = 5;    // scratch base
    const IQ: u32 = 6;     // &iq[0]
    const FA: u32 = 7;     // &f[0]
    const QA: u32 = 8;     // &q[0]
    const FQA: u32 = 9;    // &fq[0]
    const JK: u32 = 10;
    const JP: u32 = 11;
    const JX: u32 = 12;
    const JV: u32 = 13;
    const Q0: u32 = 14;
    const JZ: u32 = 15;
    const II: u32 = 16;    // i
    const JJ: u32 = 17;    // j
    // 18..=21,23,24: k/n/ih/carry/tmp1/tmp2 — used by emit_rpl_recompute_and_finalize
    //                (which re-declares them); the setup below only needs M/JTMP.
    const M: u32 = 22;
    const JTMP: u32 = 25;
    const FW: u32 = 26;    // f64
    // index 27 (Z) is used by emit_rpl_recompute_and_finalize, not this setup.
    // index 28: i64 scratch (reserved for parity).

    let mut f = Function::new([
        (21, ValType::I32), // indices 5..=25 : the i32 working locals (SCR..JTMP)
        (2, ValType::F64),  // indices 26,27   : FW, Z
        (1, ValType::I64),  // index   28      : reserved i64 scratch
    ]);

    // Helper closures to compute element addresses are inlined as wasm sequences.
    // Allocate scratch + zero it (f/q/fq/iq start at 0 in Rust).
    wasm!(f, {
        i32_const(SCRATCH_BYTES); call(alloc); local_set(SCR);
        local_get(SCR); i32_const(IQ_OFF); i32_add; local_set(IQ);
        local_get(SCR); i32_const(F_OFF); i32_add; local_set(FA);
        local_get(SCR); i32_const(Q_OFF); i32_add; local_set(QA);
        local_get(SCR); i32_const(FQ_OFF); i32_add; local_set(FQA);
        // memory.fill(SCR, 0, SCRATCH_BYTES)
        local_get(SCR); i32_const(0); i32_const(SCRATCH_BYTES); memory_fill;
    });

    // jk = INIT_JK[prec]; jp = jk  (prec is always 1 for trig → 4; emit the table read)
    wasm!(f, {
        // INIT_JK is a compile-time const; read via select chain on prec (0..3)
        // jk = match prec {0=>3,1=>4,2=>4,_=>6}
        local_get(PREC); i32_const(0); i32_eq; if_i32; i32_const(3); else_;
          local_get(PREC); i32_const(2); i32_le_u; if_i32; i32_const(4); else_; i32_const(6); end;
        end;
        local_set(JK);
        local_get(JK); local_set(JP);
        // jx = nx - 1
        local_get(NX); i32_const(1); i32_sub; local_set(JX);
        // jv = (e0 - 3) / 24 ; if jv < 0 { jv = 0 }
        local_get(E0); i32_const(3); i32_sub; i32_const(24); i32_div_s; local_set(JV);
        local_get(JV); i32_const(0); i32_lt_s; if_empty; i32_const(0); local_set(JV); end;
        // q0 = e0 - 24*(jv+1)
        local_get(E0); i32_const(24); local_get(JV); i32_const(1); i32_add; i32_mul; i32_sub; local_set(Q0);
    });

    // set up f[0..=jx+jk]: j = jv - jx ; for i in 0..=m { f[i] = j<0?0:IPIO2[j]; j++ }
    // use JTMP as j, M as m, II as i
    wasm!(f, {
        local_get(JV); local_get(JX); i32_sub; local_set(JTMP); // j
        local_get(JX); local_get(JK); i32_add; local_set(M);
        i32_const(0); local_set(II);
        block_empty; loop_empty;
            local_get(II); local_get(M); i32_gt_s; br_if(1);
            // f[i] address
            local_get(FA); local_get(II); i32_const(8); i32_mul; i32_add;
            // value = j<0 ? 0.0 : IPIO2[j] as f64
            local_get(JTMP); i32_const(0); i32_lt_s;
            if_f64;
                f64_const(0.0);
            else_;
                // IPIO2[j] : i32 at ipio2 + j*4
                i32_const(ipio2); local_get(JTMP); i32_const(4); i32_mul; i32_add; i32_load(0);
                f64_convert_i32_s;
            end;
            f64_store(0);
            local_get(JTMP); i32_const(1); i32_add; local_set(JTMP);
            local_get(II); i32_const(1); i32_add; local_set(II);
            br(0);
        end; end;
    });

    // compute q[0..=jk]: for i { fw=0; for j in 0..=jx { fw += x[j]*f[jx+i-j] } ; q[i]=fw }
    wasm!(f, {
        i32_const(0); local_set(II);
        block_empty; loop_empty;
            local_get(II); local_get(JK); i32_gt_s; br_if(1);
            f64_const(0.0); local_set(FW);
            i32_const(0); local_set(JJ);
            block_empty; loop_empty;
                local_get(JJ); local_get(JX); i32_gt_s; br_if(1);
                local_get(FW);
                // x[j]
                local_get(XP); local_get(JJ); i32_const(8); i32_mul; i32_add; f64_load(0);
                // f[jx+i-j]
                local_get(FA); local_get(JX); local_get(II); i32_add; local_get(JJ); i32_sub; i32_const(8); i32_mul; i32_add; f64_load(0);
                f64_mul; f64_add; local_set(FW);
                local_get(JJ); i32_const(1); i32_add; local_set(JJ);
                br(0);
            end; end;
            local_get(QA); local_get(II); i32_const(8); i32_mul; i32_add; local_get(FW); f64_store(0);
            local_get(II); i32_const(1); i32_add; local_set(II);
            br(0);
        end; end;
        // jz = jk
        local_get(JK); local_set(JZ);
    });
    emit_rpl_recompute_and_finalize(
        &mut f, scalbn, floor, ipio2, pio2, x1p24_bits, x1p_24_bits,
    );
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.rem_pio2_large, type_idx, f));
}

/// Emit the 'recompute loop + finalization tail of `rem_pio2_large` into `f`.
/// Split out only to keep `compile_rem_pio2_large` readable; uses the same local
/// indices defined there (re-declared here from the module layout constants).
#[allow(clippy::too_many_arguments)]
fn emit_rpl_recompute_and_finalize(
    f: &mut Function,
    scalbn: u32,
    floor: u32,
    ipio2: i32,
    pio2: i32,
    x1p24_bits: i64,
    x1p_24_bits: i64,
) {
    let _ = floor; // floor only used in the loop body below (kept for parity)
    const XP: u32 = 0; const YP: u32 = 2;
    const IQ: u32 = 6; const FA: u32 = 7; const QA: u32 = 8; const FQA: u32 = 9;
    const JK: u32 = 10; const JP: u32 = 11; const JX: u32 = 12; const JV: u32 = 13;
    const Q0: u32 = 14; const JZ: u32 = 15; const II: u32 = 16; const JJ: u32 = 17;
    const KK: u32 = 18; const N: u32 = 19; const IH: u32 = 20; const CARRY: u32 = 21;
    const TMP1: u32 = 23; const FW: u32 = 26; const Z: u32 = 27;

    // ── 'recompute loop ── block(exit) wraps loop(restart).
    wasm!(f, {
        block_empty; loop_empty;
        i32_const(0); local_set(II);
        local_get(QA); local_get(JZ); i32_const(8); i32_mul; i32_add; f64_load(0); local_set(Z);
        local_get(JZ); local_set(JJ);
        block_empty; loop_empty;
            local_get(JJ); i32_const(1); i32_lt_s; br_if(1);
            i64_const(x1p_24_bits); f64_reinterpret_i64; local_get(Z); f64_mul; i64_trunc_f64_s; i32_wrap_i64; f64_convert_i32_s; local_set(FW);
            local_get(IQ); local_get(II); i32_const(4); i32_mul; i32_add;
            local_get(Z); i64_const(x1p24_bits); f64_reinterpret_i64; local_get(FW); f64_mul; f64_sub; i64_trunc_f64_s; i32_wrap_i64;
            i32_store(0);
            local_get(QA); local_get(JJ); i32_const(1); i32_sub; i32_const(8); i32_mul; i32_add; f64_load(0); local_get(FW); f64_add; local_set(Z);
            local_get(II); i32_const(1); i32_add; local_set(II);
            local_get(JJ); i32_const(1); i32_sub; local_set(JJ);
            br(0);
        end; end;

        // z = scalbn(z, q0); z -= 8*floor(z*0.125); n = z as i32; z -= n as f64
        local_get(Z); local_get(Q0); call(scalbn); local_set(Z);
        local_get(Z);
        f64_const(8.0); local_get(Z); f64_const(0.125); f64_mul; call(floor); f64_mul;
        f64_sub; local_set(Z);
        local_get(Z); i64_trunc_f64_s; i32_wrap_i64; local_set(N);
        local_get(Z); local_get(N); f64_convert_i32_s; f64_sub; local_set(Z);
        i32_const(0); local_set(IH);
        local_get(Q0); i32_const(0); i32_gt_s;
        if_empty;
            local_get(IQ); local_get(JZ); i32_const(1); i32_sub; i32_const(4); i32_mul; i32_add; i32_load(0);
            i32_const(24); local_get(Q0); i32_sub; i32_shr_s; local_set(II);
            local_get(N); local_get(II); i32_add; local_set(N);
            local_get(IQ); local_get(JZ); i32_const(1); i32_sub; i32_const(4); i32_mul; i32_add; local_tee(TMP1);
            local_get(TMP1); i32_load(0);
            local_get(II); i32_const(24); local_get(Q0); i32_sub; i32_shl; i32_sub;
            i32_store(0);
            local_get(IQ); local_get(JZ); i32_const(1); i32_sub; i32_const(4); i32_mul; i32_add; i32_load(0);
            i32_const(23); local_get(Q0); i32_sub; i32_shr_s; local_set(IH);
        else_;
            local_get(Q0); i32_eqz;
            if_empty;
                local_get(IQ); local_get(JZ); i32_const(1); i32_sub; i32_const(4); i32_mul; i32_add; i32_load(0);
                i32_const(23); i32_shr_s; local_set(IH);
            else_;
                local_get(Z); f64_const(0.5); f64_ge; if_empty; i32_const(2); local_set(IH); end;
            end;
        end;

        local_get(IH); i32_const(0); i32_gt_s;
        if_empty;
            local_get(N); i32_const(1); i32_add; local_set(N);
            i32_const(0); local_set(CARRY);
            i32_const(0); local_set(II);
            block_empty; loop_empty;
                local_get(II); local_get(JZ); i32_ge_s; br_if(1);
                local_get(IQ); local_get(II); i32_const(4); i32_mul; i32_add; i32_load(0); local_set(JJ);
                local_get(CARRY); i32_eqz;
                if_empty;
                    local_get(JJ); i32_eqz;
                    if_empty; else_;
                        i32_const(1); local_set(CARRY);
                        local_get(IQ); local_get(II); i32_const(4); i32_mul; i32_add; i32_const(0x1000000); local_get(JJ); i32_sub; i32_store(0);
                    end;
                else_;
                    local_get(IQ); local_get(II); i32_const(4); i32_mul; i32_add; i32_const(0xffffff); local_get(JJ); i32_sub; i32_store(0);
                end;
                local_get(II); i32_const(1); i32_add; local_set(II);
                br(0);
            end; end;
            local_get(Q0); i32_const(0); i32_gt_s;
            if_empty;
                local_get(Q0); i32_const(1); i32_eq;
                if_empty;
                    local_get(IQ); local_get(JZ); i32_const(1); i32_sub; i32_const(4); i32_mul; i32_add; local_tee(TMP1);
                    local_get(TMP1); i32_load(0); i32_const(0x7fffff); i32_and; i32_store(0);
                else_;
                    local_get(Q0); i32_const(2); i32_eq;
                    if_empty;
                        local_get(IQ); local_get(JZ); i32_const(1); i32_sub; i32_const(4); i32_mul; i32_add; local_tee(TMP1);
                        local_get(TMP1); i32_load(0); i32_const(0x3fffff); i32_and; i32_store(0);
                    end;
                end;
            end;
            local_get(IH); i32_const(2); i32_eq;
            if_empty;
                f64_const(1.0); local_get(Z); f64_sub; local_set(Z);
                local_get(CARRY); i32_eqz;
                if_empty; else_;
                    local_get(Z); f64_const(1.0); local_get(Q0); call(scalbn); f64_sub; local_set(Z);
                end;
            end;
        end;

        local_get(Z); f64_const(0.0); f64_eq;
        if_empty;
            i32_const(0); local_set(JJ);
            local_get(JZ); i32_const(1); i32_sub; local_set(II);
            block_empty; loop_empty;
                local_get(II); local_get(JK); i32_lt_s; br_if(1);
                local_get(JJ); local_get(IQ); local_get(II); i32_const(4); i32_mul; i32_add; i32_load(0); i32_or; local_set(JJ);
                local_get(II); i32_const(1); i32_sub; local_set(II);
                br(0);
            end; end;
            local_get(JJ); i32_eqz;
            if_empty;
                i32_const(1); local_set(KK);
                block_empty; loop_empty;
                    local_get(IQ); local_get(JK); local_get(KK); i32_sub; i32_const(4); i32_mul; i32_add; i32_load(0);
                    br_if(1);
                    local_get(KK); i32_const(1); i32_add; local_set(KK);
                    br(0);
                end; end;
                local_get(JZ); i32_const(1); i32_add; local_set(II);
                block_empty; loop_empty;
                    local_get(II); local_get(JZ); local_get(KK); i32_add; i32_gt_s; br_if(1);
                    local_get(FA); local_get(JX); local_get(II); i32_add; i32_const(8); i32_mul; i32_add;
                    i32_const(ipio2); local_get(JV); local_get(II); i32_add; i32_const(4); i32_mul; i32_add; i32_load(0); f64_convert_i32_s;
                    f64_store(0);
                    f64_const(0.0); local_set(FW);
                    i32_const(0); local_set(JJ);
                    block_empty; loop_empty;
                        local_get(JJ); local_get(JX); i32_gt_s; br_if(1);
                        local_get(FW);
                        local_get(XP); local_get(JJ); i32_const(8); i32_mul; i32_add; f64_load(0);
                        local_get(FA); local_get(JX); local_get(II); i32_add; local_get(JJ); i32_sub; i32_const(8); i32_mul; i32_add; f64_load(0);
                        f64_mul; f64_add; local_set(FW);
                        local_get(JJ); i32_const(1); i32_add; local_set(JJ);
                        br(0);
                    end; end;
                    local_get(QA); local_get(II); i32_const(8); i32_mul; i32_add; local_get(FW); f64_store(0);
                    local_get(II); i32_const(1); i32_add; local_set(II);
                    br(0);
                end; end;
                local_get(JZ); local_get(KK); i32_add; local_set(JZ);
                // continue 'recompute: branch to the main loop header. Depth 0 = the
                // inner `if j==0`, 1 = the outer `if z==0`, 2 = the 'recompute loop.
                // (`if` blocks DO count as branch-target labels — a `br(1)` here would
                // only exit the `if z==0` and fall through to the chop without
                // re-distilling, which silently truncates argument reduction.)
                br(2);
            end;
        end;
        br(1);
        end; end;
    });

    // ── chop off zero terms / break z into 24-bit ──
    wasm!(f, {
        local_get(Z); f64_const(0.0); f64_eq;
        if_empty;
            local_get(JZ); i32_const(1); i32_sub; local_set(JZ);
            local_get(Q0); i32_const(24); i32_sub; local_set(Q0);
            block_empty; loop_empty;
                local_get(IQ); local_get(JZ); i32_const(4); i32_mul; i32_add; i32_load(0); br_if(1);
                local_get(JZ); i32_const(1); i32_sub; local_set(JZ);
                local_get(Q0); i32_const(24); i32_sub; local_set(Q0);
                br(0);
            end; end;
        else_;
            local_get(Z); i32_const(0); local_get(Q0); i32_sub; call(scalbn); local_set(Z);
            local_get(Z); i64_const(x1p24_bits); f64_reinterpret_i64; f64_ge;
            if_empty;
                i64_const(x1p_24_bits); f64_reinterpret_i64; local_get(Z); f64_mul; i64_trunc_f64_s; i32_wrap_i64; f64_convert_i32_s; local_set(FW);
                local_get(IQ); local_get(JZ); i32_const(4); i32_mul; i32_add;
                local_get(Z); i64_const(x1p24_bits); f64_reinterpret_i64; local_get(FW); f64_mul; f64_sub; i64_trunc_f64_s; i32_wrap_i64; i32_store(0);
                local_get(JZ); i32_const(1); i32_add; local_set(JZ);
                local_get(Q0); i32_const(24); i32_add; local_set(Q0);
                local_get(IQ); local_get(JZ); i32_const(4); i32_mul; i32_add; local_get(FW); i64_trunc_f64_s; i32_wrap_i64; i32_store(0);
            else_;
                local_get(IQ); local_get(JZ); i32_const(4); i32_mul; i32_add; local_get(Z); i64_trunc_f64_s; i32_wrap_i64; i32_store(0);
            end;
        end;
    });

    // ── convert integer "bit" chunk to f64 ──
    wasm!(f, {
        f64_const(1.0); local_get(Q0); call(scalbn); local_set(FW);
        local_get(JZ); local_set(II);
        block_empty; loop_empty;
            local_get(II); i32_const(0); i32_lt_s; br_if(1);
            local_get(QA); local_get(II); i32_const(8); i32_mul; i32_add;
            local_get(FW); local_get(IQ); local_get(II); i32_const(4); i32_mul; i32_add; i32_load(0); f64_convert_i32_s; f64_mul;
            f64_store(0);
            local_get(FW); i64_const(x1p_24_bits); f64_reinterpret_i64; f64_mul; local_set(FW);
            local_get(II); i32_const(1); i32_sub; local_set(II);
            br(0);
        end; end;
    });

    // ── PIO2[0..=jp]*q[jz..0] → fq ──
    wasm!(f, {
        local_get(JZ); local_set(II);
        block_empty; loop_empty;
            local_get(II); i32_const(0); i32_lt_s; br_if(1);
            f64_const(0.0); local_set(FW);
            i32_const(0); local_set(KK);
            block_empty; loop_empty;
                local_get(KK); local_get(JP); i32_gt_s; br_if(1);
                local_get(KK); local_get(JZ); local_get(II); i32_sub; i32_gt_s; br_if(1);
                local_get(FW);
                i32_const(pio2); local_get(KK); i32_const(8); i32_mul; i32_add; f64_load(0);
                local_get(QA); local_get(II); local_get(KK); i32_add; i32_const(8); i32_mul; i32_add; f64_load(0);
                f64_mul; f64_add; local_set(FW);
                local_get(KK); i32_const(1); i32_add; local_set(KK);
                br(0);
            end; end;
            local_get(FQA); local_get(JZ); local_get(II); i32_sub; i32_const(8); i32_mul; i32_add; local_get(FW); f64_store(0);
            local_get(II); i32_const(1); i32_sub; local_set(II);
            br(0);
        end; end;
    });

    // ── compress fq[] into y[] (prec 1: y[0],y[1]) ──
    wasm!(f, {
        f64_const(0.0); local_set(FW);
        local_get(JZ); local_set(II);
        block_empty; loop_empty;
            local_get(II); i32_const(0); i32_lt_s; br_if(1);
            local_get(FW); local_get(FQA); local_get(II); i32_const(8); i32_mul; i32_add; f64_load(0); f64_add; local_set(FW);
            local_get(II); i32_const(1); i32_sub; local_set(II);
            br(0);
        end; end;
        local_get(YP);
        local_get(IH); i32_eqz; if_f64; local_get(FW); else_; local_get(FW); f64_neg; end;
        f64_store(0);
        local_get(FQA); f64_load(0); local_get(FW); f64_sub; local_set(FW);
        i32_const(1); local_set(II);
        block_empty; loop_empty;
            local_get(II); local_get(JZ); i32_gt_s; br_if(1);
            local_get(FW); local_get(FQA); local_get(II); i32_const(8); i32_mul; i32_add; f64_load(0); f64_add; local_set(FW);
            local_get(II); i32_const(1); i32_add; local_set(II);
            br(0);
        end; end;
        local_get(YP);
        local_get(IH); i32_eqz; if_f64; local_get(FW); else_; local_get(FW); f64_neg; end;
        f64_store(8);
        local_get(N); i32_const(7); i32_and;
        end;
    });
}

// ════════════════════ exp / log / log2 / log10 / pow ════════════════════
// WASM twins of the vendored native exp/log/log2/log10/pow in
// runtime/rs/src/libm.rs (themselves libm 0.2.16 / FreeBSD msun). Same
// coefficients (named consts with provenance), same branch structure, same bit
// manipulations → bit-identical native↔wasm. Unlike the trig path these are
// straight-line (no scratch arrays, no embedded tables, no loops), so they read
// almost 1:1 against the Rust source.
//
// Word-manipulation helpers (libm `support` module) are emitted inline:
//   get_high_word(x)         = (to_bits(x) >> 32) as u32
//   with_set_low_word(x, 0)  = from_bits(to_bits(x) & 0xFFFFFFFF_00000000)
//   with_set_high_word(0, h) = from_bits((h as u64) << 32)
// `fabs`/`sqrt` are the native `f64.abs`/`f64.sqrt` opcodes (IEEE-754 correctly
// rounded, so they match Rust `f64::abs`/`f64::sqrt`). `scalbn` reuses the
// already-emitted `__libm_scalbn`.

// Bit mask that zeroes the low 32-bit word of an f64 (with_set_low_word(_, 0)).
const LOW_WORD_MASK: i64 = 0xFFFF_FFFF_0000_0000_u64 as i64;

// ── exp constants (e_exp.c) ──
const EXP_LN2HI: f64 = 6.93147180369123816490e-01; /* 0x3fe62e42, 0xfee00000 */
const EXP_LN2LO: f64 = 1.90821492927058770002e-10; /* 0x3dea39ef, 0x35793c76 */
const EXP_INVLN2: f64 = 1.44269504088896338700e+00; /* 0x3ff71547, 0x652b82fe */
const EXP_P1: f64 = 1.66666666666666019037e-01; /* 0x3FC55555, 0x5555553E */
const EXP_P2: f64 = -2.77777777770155933842e-03; /* 0xBF66C16C, 0x16BEBD93 */
const EXP_P3: f64 = 6.61375632143793436117e-05; /* 0x3F11566A, 0xAF25DE2C */
const EXP_P4: f64 = -1.65339022054652515390e-06; /* 0xBEBBBD41, 0xC5D26BF1 */
const EXP_P5: f64 = 4.13813679705723846039e-08; /* 0x3E663769, 0x72BEA4D0 */

// ───────────────────────────── __libm_exp ──────────────────────────────
// Faithful port of libm `exp` (e_exp.c). See runtime/rs/src/libm.rs::exp.
fn compile_exp(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.exp];
    let scalbn = emitter.rt.libm.scalbn;
    // 2^1023 and 2^-149 (the two scaling/underflow-flag constants).
    let x1p1023 = f64::from_bits(0x7fe0000000000000);
    let x1p_149 = f64::from_bits(0x36a0000000000000);
    // params: 0=x. locals:
    //   1=i32 hx, 2=i32 sign, 3=i32 k,
    //   4=f64 hi, 5=f64 lo, 6=f64 c, 7=f64 xx, 8=f64 y
    const X: u32 = 0;
    const HX: u32 = 1; const SIGN: u32 = 2; const K: u32 = 3;
    const HI: u32 = 4; const LO: u32 = 5; const C: u32 = 6; const XX: u32 = 7; const Y: u32 = 8;
    let mut f = Function::new([(3, ValType::I32), (5, ValType::F64)]);
    wasm!(f, {
        // hx = (to_bits(x) >> 32); sign = hx >> 31; hx &= 0x7fffffff
        local_get(X); i64_reinterpret_f64; i64_const(HI_SHIFT); i64_shr_u; i32_wrap_i64; local_set(HX);
        local_get(HX); i32_const(31); i32_shr_u; local_set(SIGN);
        local_get(HX); i32_const(0x7fffffff); i32_and; local_set(HX);

        // special cases: if hx >= 0x4086232b
        local_get(HX); i32_const(0x4086232b); i32_ge_u;
        if_empty;
            // if x.is_nan() return x  (x != x)
            local_get(X); local_get(X); f64_ne;
            if_empty; local_get(X); return_; end;
            // if x > 709.782712893383973096 { x *= 2^1023; return x }  (overflow)
            local_get(X); f64_const(709.782712893383973096); f64_gt;
            if_empty;
                local_get(X); f64_const(x1p1023); f64_mul; return_;
            end;
            // if x < -708.39641853226410622 { force_eval(-2^-149/x); if x < -745.13321910194110842 return 0 }
            local_get(X); f64_const(-708.39641853226410622); f64_lt;
            if_empty;
                // unobservable underflow-flag computation; drop the f32 result.
                f64_const(x1p_149); f64_neg; local_get(X); f64_div; f32_demote_f64; drop;
                local_get(X); f64_const(-745.13321910194110842); f64_lt;
                if_empty; f64_const(0.0); return_; end;
            end;
        end;

        // argument reduction
        local_get(HX); i32_const(0x3fd62e42); i32_gt_u;
        if_empty;
            // |x| > 0.5 ln2
            local_get(HX); i32_const(0x3ff0a2b2); i32_ge_u;
            if_empty;
                // k = (INVLN2*x + HALF[sign]) as i32   (HALF = [0.5, -0.5])
                local_get(X); f64_const(EXP_INVLN2); f64_mul;
                local_get(SIGN); if_f64; f64_const(-0.5); else_; f64_const(0.5); end;
                f64_add; i32_trunc_f64_s; local_set(K);
            else_;
                // k = 1 - sign - sign
                i32_const(1); local_get(SIGN); i32_sub; local_get(SIGN); i32_sub; local_set(K);
            end;
            // hi = x - k*LN2HI; lo = k*LN2LO; x = hi - lo
            local_get(X); local_get(K); f64_convert_i32_s; f64_const(EXP_LN2HI); f64_mul; f64_sub; local_set(HI);
            local_get(K); f64_convert_i32_s; f64_const(EXP_LN2LO); f64_mul; local_set(LO);
            local_get(HI); local_get(LO); f64_sub; local_set(X);
        else_;
            local_get(HX); i32_const(0x3e300000); i32_gt_u;
            if_empty;
                // |x| > 2^-28: k=0; hi=x; lo=0
                i32_const(0); local_set(K);
                local_get(X); local_set(HI);
                f64_const(0.0); local_set(LO);
            else_;
                // inexact if x!=0; return 1 + x
                f64_const(x1p1023); local_get(X); f64_add; drop;
                f64_const(1.0); local_get(X); f64_add; return_;
            end;
        end;

        // primary range: xx = x*x;
        local_get(X); local_get(X); f64_mul; local_set(XX);
        // c = x - xx*(P1 + xx*(P2 + xx*(P3 + xx*(P4 + xx*P5))))
        local_get(X);
        local_get(XX);
        f64_const(EXP_P1);
        local_get(XX); f64_const(EXP_P2);
        local_get(XX); f64_const(EXP_P3);
        local_get(XX); f64_const(EXP_P4);
        local_get(XX); f64_const(EXP_P5); f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul;
        f64_sub; local_set(C);
        // y = 1 + (x*c/(2-c) - lo + hi)
        f64_const(1.0);
        local_get(X); local_get(C); f64_mul; f64_const(2.0); local_get(C); f64_sub; f64_div;
        local_get(LO); f64_sub;
        local_get(HI); f64_add;
        f64_add; local_set(Y);
        // if k==0 { y } else { scalbn(y, k) }
        local_get(K); i32_eqz;
        if_f64;
            local_get(Y);
        else_;
            local_get(Y); local_get(K); call(scalbn);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.exp, type_idx, f));
}

// ── shared log reduction (e_log.c kernel; log2/log10 reuse it) ──
const LOG_LN2_HI: f64 = 6.93147180369123816490e-01; /* 3fe62e42 fee00000 */
const LOG_LN2_LO: f64 = 1.90821492927058770002e-10; /* 3dea39ef 35793c76 */
const LG1: f64 = 6.666666666666735130e-01; /* 3FE55555 55555593 */
const LG2: f64 = 3.999999999940941908e-01; /* 3FD99999 9997FA04 */
const LG3: f64 = 2.857142874366239149e-01; /* 3FD24924 94229359 */
const LG4: f64 = 2.222219843214978396e-01; /* 3FCC71C5 1D8E78AF */
const LG5: f64 = 1.818357216161805012e-01; /* 3FC74664 96CB03DE */
const LG6: f64 = 1.531383769920937332e-01; /* 3FC39A09 D078C69F */
const LG7: f64 = 1.479819860511658591e-01; /* 3FC2F112 DF3E5244 */

// Local layout shared by the log family. The natural-log specific tail uses
// only F/HFSQ/S/R/K (and X). The log2/log10 tails additionally use the HI/LO
// extra-precision split locals (declared by each compile_* below).
const LG_X: u32 = 0;       // f64 param x
const LG_UI: u32 = 1;      // i64 bits
const LG_HX: u32 = 2;      // i32 high word
const LG_K: u32 = 3;       // i32 exponent k
const LG_F: u32 = 4;       // f64 f = x-1
const LG_HFSQ: u32 = 5;    // f64 hfsq
const LG_S: u32 = 6;       // f64 s
const LG_Z: u32 = 7;       // f64 z
const LG_W: u32 = 8;       // f64 w
const LG_R: u32 = 9;       // f64 r (= t2 + t1)

/// Emit the libm `log` special-case dispatch + 2^k(1+f) reduction + the
/// degree-14 Remez polynomial `r = t2 + t1`. On entry x is param 0. On the
/// non-fast paths it RETURNS directly (log(±0)=-inf, log(neg)=NaN, log(inf)=x,
/// log(1)=0); otherwise it falls through leaving k/f/hfsq/s/r populated for the
/// caller's combine step. x1p54 = 2^54 (subnormal scale-up factor).
fn emit_log_reduce(f: &mut Function) {
    let x1p54 = f64::from_bits(0x4350000000000000); // 2^54
    wasm!(f, {
        // ui = to_bits(x); hx = ui >> 32; k = 0
        local_get(LG_X); i64_reinterpret_f64; local_set(LG_UI);
        local_get(LG_UI); i64_const(HI_SHIFT); i64_shr_u; i32_wrap_i64; local_set(LG_HX);
        i32_const(0); local_set(LG_K);
        // if hx < 0x00100000 || (hx >> 31) != 0
        local_get(LG_HX); i32_const(0x00100000); i32_lt_u;
        local_get(LG_HX); i32_const(31); i32_shr_u; i32_const(0); i32_ne;
        i32_or;
        if_empty;
            // if (ui << 1) == 0 { return -1/(x*x) }   log(+-0) = -inf
            local_get(LG_UI); i64_const(1); i64_shl; i64_eqz;
            if_empty;
                f64_const(-1.0); local_get(LG_X); local_get(LG_X); f64_mul; f64_div; return_;
            end;
            // if (hx >> 31) != 0 { return (x-x)/0 }   log(-#) = NaN
            local_get(LG_HX); i32_const(31); i32_shr_u; i32_const(0); i32_ne;
            if_empty;
                local_get(LG_X); local_get(LG_X); f64_sub; f64_const(0.0); f64_div; return_;
            end;
            // subnormal: k -= 54; x *= 2^54; ui = to_bits(x); hx = ui >> 32
            local_get(LG_K); i32_const(54); i32_sub; local_set(LG_K);
            local_get(LG_X); f64_const(x1p54); f64_mul; local_set(LG_X);
            local_get(LG_X); i64_reinterpret_f64; local_set(LG_UI);
            local_get(LG_UI); i64_const(HI_SHIFT); i64_shr_u; i32_wrap_i64; local_set(LG_HX);
        else_;
            // else if hx >= 0x7ff00000 { return x }
            local_get(LG_HX); i32_const(0x7ff00000); i32_ge_u;
            if_empty; local_get(LG_X); return_; end;
            // else if hx == 0x3ff00000 && (ui << 32) == 0 { return 0 }
            local_get(LG_HX); i32_const(0x3ff00000); i32_eq;
            local_get(LG_UI); i64_const(32); i64_shl; i64_eqz;
            i32_and;
            if_empty; f64_const(0.0); return_; end;
        end;

        // reduce x into [sqrt(2)/2, sqrt(2)]
        // hx += 0x3ff00000 - 0x3fe6a09e
        local_get(LG_HX); i32_const(0x3ff00000 - 0x3fe6a09e); i32_add; local_set(LG_HX);
        // k += (hx >> 20) - 0x3ff
        local_get(LG_K); local_get(LG_HX); i32_const(20); i32_shr_u; i32_const(0x3ff); i32_sub; i32_add; local_set(LG_K);
        // hx = (hx & 0x000fffff) + 0x3fe6a09e
        local_get(LG_HX); i32_const(0x000fffff); i32_and; i32_const(0x3fe6a09e); i32_add; local_set(LG_HX);
        // ui = (hx << 32) | (ui & 0xffffffff); x = from_bits(ui)
        local_get(LG_HX); i64_extend_i32_u; i64_const(HI_SHIFT); i64_shl;
        local_get(LG_UI); i64_const(0xffffffff); i64_and;
        i64_or; local_set(LG_UI);
        local_get(LG_UI); f64_reinterpret_i64; local_set(LG_X);

        // f = x - 1; hfsq = 0.5*f*f; s = f/(2+f); z = s*s; w = z*z
        local_get(LG_X); f64_const(1.0); f64_sub; local_set(LG_F);
        f64_const(0.5); local_get(LG_F); f64_mul; local_get(LG_F); f64_mul; local_set(LG_HFSQ);
        local_get(LG_F); f64_const(2.0); local_get(LG_F); f64_add; f64_div; local_set(LG_S);
        local_get(LG_S); local_get(LG_S); f64_mul; local_set(LG_Z);
        local_get(LG_Z); local_get(LG_Z); f64_mul; local_set(LG_W);
        // t1 = w*(LG2 + w*(LG4 + w*LG6)); t2 = z*(LG1 + w*(LG3 + w*(LG5 + w*LG7))); r = t2 + t1
        // compute r directly: r = t2 + t1
        local_get(LG_Z); f64_const(LG1);
        local_get(LG_W); f64_const(LG3);
        local_get(LG_W); f64_const(LG5);
        local_get(LG_W); f64_const(LG7); f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul;                                  // t2
        local_get(LG_W); f64_const(LG2);
        local_get(LG_W); f64_const(LG4);
        local_get(LG_W); f64_const(LG6); f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul;                                  // t1
        f64_add; local_set(LG_R);
    });
}

// ───────────────────────────── __libm_log ──────────────────────────────
// Faithful port of libm `log` (e_log.c). See runtime/rs/src/libm.rs::log.
fn compile_log(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.log];
    // locals: 1=i64 ui, 2..3 i32 (hx,k), 4..9 f64 (f,hfsq,s,z,w,r)
    let mut f = Function::new([(1, ValType::I64), (2, ValType::I32), (6, ValType::F64)]);
    emit_log_reduce(&mut f);
    wasm!(f, {
        // dk = k; return s*(hfsq+r) + dk*LN2_LO - hfsq + f + dk*LN2_HI
        local_get(LG_S); local_get(LG_HFSQ); local_get(LG_R); f64_add; f64_mul;
        local_get(LG_K); f64_convert_i32_s; f64_const(LOG_LN2_LO); f64_mul; f64_add;
        local_get(LG_HFSQ); f64_sub;
        local_get(LG_F); f64_add;
        local_get(LG_K); f64_convert_i32_s; f64_const(LOG_LN2_HI); f64_mul; f64_add;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.log, type_idx, f));
}

// log2/log10 extra-precision tail locals (after r at index 9).
const LG_HI: u32 = 10;   // f64 hi
const LG_LO: u32 = 11;   // f64 lo
const LG_VH: u32 = 12;   // f64 val_hi
const LG_VL: u32 = 13;   // f64 val_lo
const LG_Y: u32 = 14;    // f64 y (= k as f64, or dk*LOG10_2HI)

const IVLN2HI: f64 = 1.44269504072144627571e+00; /* 0x3ff71547, 0x65200000 */
const IVLN2LO: f64 = 1.67517131648865118353e-10; /* 0x3de705fc, 0x2eefa200 */

// ───────────────────────────── __libm_log2 ─────────────────────────────
// Faithful port of libm `log2` (e_log2.c). See runtime/rs/src/libm.rs::log2.
fn compile_log2(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.log2];
    // locals: 1=i64 ui, 2..3 i32, 4..14 f64 (f,hfsq,s,z,w,r,hi,lo,val_hi,val_lo,y)
    let mut f = Function::new([(1, ValType::I64), (2, ValType::I32), (11, ValType::F64)]);
    emit_log_reduce(&mut f);
    wasm!(f, {
        // hi = f - hfsq; ui = to_bits(hi) & 0xFFFFFFFF00000000; hi = from_bits(ui)
        local_get(LG_F); local_get(LG_HFSQ); f64_sub; local_set(LG_HI);
        local_get(LG_HI); i64_reinterpret_f64; i64_const(LOW_WORD_MASK); i64_and; f64_reinterpret_i64; local_set(LG_HI);
        // lo = f - hi - hfsq + s*(hfsq + r)
        local_get(LG_F); local_get(LG_HI); f64_sub; local_get(LG_HFSQ); f64_sub;
        local_get(LG_S); local_get(LG_HFSQ); local_get(LG_R); f64_add; f64_mul;
        f64_add; local_set(LG_LO);
        // val_hi = hi*IVLN2HI; val_lo = (lo+hi)*IVLN2LO + lo*IVLN2HI
        local_get(LG_HI); f64_const(IVLN2HI); f64_mul; local_set(LG_VH);
        local_get(LG_LO); local_get(LG_HI); f64_add; f64_const(IVLN2LO); f64_mul;
        local_get(LG_LO); f64_const(IVLN2HI); f64_mul; f64_add; local_set(LG_VL);
        // y = k; w = y + val_hi; val_lo += (y - w) + val_hi; val_hi = w
        local_get(LG_K); f64_convert_i32_s; local_set(LG_Y);
        local_get(LG_Y); local_get(LG_VH); f64_add; local_set(LG_W);
        local_get(LG_VL); local_get(LG_Y); local_get(LG_W); f64_sub; local_get(LG_VH); f64_add; f64_add; local_set(LG_VL);
        local_get(LG_W); local_set(LG_VH);
        // return val_lo + val_hi
        local_get(LG_VL); local_get(LG_VH); f64_add;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.log2, type_idx, f));
}

const IVLN10HI: f64 = 4.34294481878168880939e-01; /* 0x3fdbcb7b, 0x15200000 */
const IVLN10LO: f64 = 2.50829467116452752298e-11; /* 0x3dbb9438, 0xca9aadd5 */
const LOG10_2HI: f64 = 3.01029995663611771306e-01; /* 0x3FD34413, 0x509F6000 */
const LOG10_2LO: f64 = 3.69423907715893078616e-13; /* 0x3D59FEF3, 0x11F12B36 */

// ───────────────────────────── __libm_log10 ────────────────────────────
// Faithful port of libm `log10` (e_log10.c). See runtime/rs/src/libm.rs::log10.
fn compile_log10(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.log10];
    let mut f = Function::new([(1, ValType::I64), (2, ValType::I32), (11, ValType::F64)]);
    emit_log_reduce(&mut f);
    wasm!(f, {
        // hi = f - hfsq; zero low word
        local_get(LG_F); local_get(LG_HFSQ); f64_sub; local_set(LG_HI);
        local_get(LG_HI); i64_reinterpret_f64; i64_const(LOW_WORD_MASK); i64_and; f64_reinterpret_i64; local_set(LG_HI);
        // lo = f - hi - hfsq + s*(hfsq + r)
        local_get(LG_F); local_get(LG_HI); f64_sub; local_get(LG_HFSQ); f64_sub;
        local_get(LG_S); local_get(LG_HFSQ); local_get(LG_R); f64_add; f64_mul;
        f64_add; local_set(LG_LO);
        // val_hi = hi*IVLN10HI; dk = k; y = dk*LOG10_2HI
        local_get(LG_HI); f64_const(IVLN10HI); f64_mul; local_set(LG_VH);
        local_get(LG_K); f64_convert_i32_s; f64_const(LOG10_2HI); f64_mul; local_set(LG_Y);
        // val_lo = dk*LOG10_2LO + (lo+hi)*IVLN10LO + lo*IVLN10HI
        local_get(LG_K); f64_convert_i32_s; f64_const(LOG10_2LO); f64_mul;
        local_get(LG_LO); local_get(LG_HI); f64_add; f64_const(IVLN10LO); f64_mul; f64_add;
        local_get(LG_LO); f64_const(IVLN10HI); f64_mul; f64_add; local_set(LG_VL);
        // w = y + val_hi; val_lo += (y - w) + val_hi; val_hi = w
        local_get(LG_Y); local_get(LG_VH); f64_add; local_set(LG_W);
        local_get(LG_VL); local_get(LG_Y); local_get(LG_W); f64_sub; local_get(LG_VH); f64_add; f64_add; local_set(LG_VL);
        local_get(LG_W); local_set(LG_VH);
        local_get(LG_VL); local_get(LG_VH); f64_add;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.log10, type_idx, f));
}

// ── pow constants (e_pow.c) ──
const POW_DP_H1: f64 = 5.84962487220764160156e-01; /* dp_h[1], dp_h[0]=0 */
const POW_DP_L1: f64 = 1.35003920212974897128e-08; /* dp_l[1], dp_l[0]=0 */
const POW_TWO53: f64 = 9007199254740992.0;
const POW_HUGE: f64 = 1.0e300;
const POW_TINY: f64 = 1.0e-300;
const POW_L1: f64 = 5.99999999999994648725e-01;
const POW_L2: f64 = 4.28571428578550184252e-01;
const POW_L3: f64 = 3.33333329818377432918e-01;
const POW_L4: f64 = 2.72728123808534006489e-01;
const POW_L5: f64 = 2.30660745775561754067e-01;
const POW_L6: f64 = 2.06975017800338417784e-01;
const POW_P1: f64 = 1.66666666666666019037e-01;
const POW_P2: f64 = -2.77777777770155933842e-03;
const POW_P3: f64 = 6.61375632143793436117e-05;
const POW_P4: f64 = -1.65339022054652515390e-06;
const POW_P5: f64 = 4.13813679705723846039e-08;
const POW_LG2: f64 = 6.93147180559945286227e-01;
const POW_LG2_H: f64 = 6.93147182464599609375e-01;
const POW_LG2_L: f64 = -1.90465429995776804525e-09;
const POW_OVT: f64 = 8.0085662595372944372e-017;
const POW_CP: f64 = 9.61796693925975554329e-01;
const POW_CP_H: f64 = 9.61796700954437255859e-01;
const POW_CP_L: f64 = -7.02846165095275826516e-09;
const POW_IVLN2: f64 = 1.44269504088896338700e+00;
const POW_IVLN2_H: f64 = 1.44269502162933349609e+00;
const POW_IVLN2_L: f64 = 1.92596299112661746887e-08;
// small-|1-x| cubic-log coefficient (musl writes 1/3 as this literal).
const POW_ONE_THIRD: f64 = 0.3333333333333333333333;

// __libm_pow local map. params 0=x, 1=y.
const PW_X: u32 = 0; const PW_Y: u32 = 1;
// i32 ints from the bit decomposition + working integers.
const PW_HX: u32 = 2; const PW_LX: u32 = 3; const PW_HY: u32 = 4; const PW_LY: u32 = 5;
const PW_IX: u32 = 6; const PW_IY: u32 = 7;
const PW_YISINT: u32 = 8; const PW_K: u32 = 9; const PW_J: u32 = 10;
const PW_N: u32 = 11; const PW_KK: u32 = 12; const PW_I: u32 = 13;
// f64 working set.
const PW_AX: u32 = 14; const PW_S: u32 = 15; const PW_T1: u32 = 16; const PW_T2: u32 = 17;
const PW_Z: u32 = 18; const PW_U: u32 = 19; const PW_V: u32 = 20; const PW_W: u32 = 21;
const PW_SS: u32 = 22; const PW_S_H: u32 = 23; const PW_S_L: u32 = 24; const PW_T_H: u32 = 25;
const PW_T_L: u32 = 26; const PW_S2: u32 = 27; const PW_R: u32 = 28; const PW_P_H: u32 = 29;
const PW_P_L: u32 = 30; const PW_Z_H: u32 = 31; const PW_Z_L: u32 = 32; const PW_Y1: u32 = 33;
const PW_T: u32 = 34;

/// Push `with_set_low_word(<f64 on stack>, 0)` (zero the low 32-bit word).
fn emit_zero_low_word(f: &mut Function) {
    wasm!(f, { i64_reinterpret_f64; i64_const(LOW_WORD_MASK); i64_and; f64_reinterpret_i64; });
}
/// Push `get_high_word(<f64 on stack>)` as i32.
fn emit_high_word(f: &mut Function) {
    wasm!(f, { i64_reinterpret_f64; i64_const(HI_SHIFT); i64_shr_u; i32_wrap_i64; });
}
/// Push `with_set_high_word(0.0, <i32 hi on stack>)`  = from_bits((hi as u64)<<32).
fn emit_from_high_word(f: &mut Function) {
    wasm!(f, { i64_extend_i32_u; i64_const(HI_SHIFT); i64_shl; f64_reinterpret_i64; });
}

// ───────────────────────────── __libm_pow ──────────────────────────────
// Faithful port of libm `pow` (e_pow.c). See runtime/rs/src/libm.rs::pow.
// Exhaustive special-case handling (0/inf/nan/neg-base/odd-even integer
// exponent) → log2(x) extra precision → y*log2(x) → 2**(...).
fn compile_pow(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.pow];
    let scalbn = emitter.rt.libm.scalbn;
    let mut f = Function::new([
        (12, ValType::I32), // 2..=13 : hx,lx,hy,ly,ix,iy,yisint,k,j,n,kk,i
        (21, ValType::F64), // 14..=34: ax,s,t1,t2,z,u,v,w,ss,s_h,s_l,t_h,t_l,s2,r,p_h,p_l,z_h,z_l,y1,t
    ]);

    wasm!(f, {
        // hx = (to_bits(x) >> 32) as i32; lx = to_bits(x) as u32
        local_get(PW_X); i64_reinterpret_f64; i64_const(HI_SHIFT); i64_shr_u; i32_wrap_i64; local_set(PW_HX);
        local_get(PW_X); i64_reinterpret_f64; i32_wrap_i64; local_set(PW_LX);
        local_get(PW_Y); i64_reinterpret_f64; i64_const(HI_SHIFT); i64_shr_u; i32_wrap_i64; local_set(PW_HY);
        local_get(PW_Y); i64_reinterpret_f64; i32_wrap_i64; local_set(PW_LY);
        // ix = hx & 0x7fffffff; iy = hy & 0x7fffffff
        local_get(PW_HX); i32_const(0x7fffffff); i32_and; local_set(PW_IX);
        local_get(PW_HY); i32_const(0x7fffffff); i32_and; local_set(PW_IY);

        // x**0 = 1 (even if x is NaN): if (iy | ly) == 0 return 1
        local_get(PW_IY); local_get(PW_LY); i32_or; i32_eqz;
        if_empty; f64_const(1.0); return_; end;
        // 1**y = 1: if hx == 0x3ff00000 && lx == 0 return 1
        local_get(PW_HX); i32_const(0x3ff00000); i32_eq; local_get(PW_LX); i32_eqz; i32_and;
        if_empty; f64_const(1.0); return_; end;
        // NaN if either arg is NaN:
        //   ix > 0x7ff00000 || (ix==0x7ff00000 && lx!=0) || iy > 0x7ff00000 || (iy==0x7ff00000 && ly!=0)
        local_get(PW_IX); i32_const(0x7ff00000); i32_gt_s;
        local_get(PW_IX); i32_const(0x7ff00000); i32_eq; local_get(PW_LX); i32_const(0); i32_ne; i32_and; i32_or;
        local_get(PW_IY); i32_const(0x7ff00000); i32_gt_s; i32_or;
        local_get(PW_IY); i32_const(0x7ff00000); i32_eq; local_get(PW_LY); i32_const(0); i32_ne; i32_and; i32_or;
        if_empty; local_get(PW_X); local_get(PW_Y); f64_add; return_; end;

        // yisint: 0 not int, 1 odd int, 2 even int (only matters when x < 0)
        i32_const(0); local_set(PW_YISINT);
        local_get(PW_HX); i32_const(0); i32_lt_s;
        if_empty;
            local_get(PW_IY); i32_const(0x43400000); i32_ge_s;
            if_empty;
                i32_const(2); local_set(PW_YISINT); // even integer y
            else_;
                local_get(PW_IY); i32_const(0x3ff00000); i32_ge_s;
                if_empty;
                    // k = (iy >> 20) - 0x3ff
                    local_get(PW_IY); i32_const(20); i32_shr_s; i32_const(0x3ff); i32_sub; local_set(PW_K);
                    local_get(PW_K); i32_const(20); i32_gt_s;
                    if_empty;
                        // j = (ly >> (52 - k)) ; if (j << (52-k)) == ly { yisint = 2 - (j&1) }
                        local_get(PW_LY); i32_const(52); local_get(PW_K); i32_sub; i32_shr_u; local_set(PW_J);
                        local_get(PW_J); i32_const(52); local_get(PW_K); i32_sub; i32_shl; local_get(PW_LY); i32_eq;
                        if_empty;
                            i32_const(2); local_get(PW_J); i32_const(1); i32_and; i32_sub; local_set(PW_YISINT);
                        end;
                    else_;
                        local_get(PW_LY); i32_eqz;
                        if_empty;
                            // j = iy >> (20 - k); if (j << (20-k)) == iy { yisint = 2 - (j&1) }
                            local_get(PW_IY); i32_const(20); local_get(PW_K); i32_sub; i32_shr_s; local_set(PW_J);
                            local_get(PW_J); i32_const(20); local_get(PW_K); i32_sub; i32_shl; local_get(PW_IY); i32_eq;
                            if_empty;
                                i32_const(2); local_get(PW_J); i32_const(1); i32_and; i32_sub; local_set(PW_YISINT);
                            end;
                        end;
                    end;
                end;
            end;
        end;

        // special value of y: if ly == 0
        local_get(PW_LY); i32_eqz;
        if_empty;
            // y is +-inf
            local_get(PW_IY); i32_const(0x7ff00000); i32_eq;
            if_empty;
                // if ((ix - 0x3ff00000) | lx) == 0  -> (-1)**+-inf = 1
                local_get(PW_IX); i32_const(0x3ff00000); i32_sub; local_get(PW_LX); i32_or; i32_eqz;
                if_f64;
                    f64_const(1.0);
                else_;
                    // elif ix >= 0x3ff00000  -> (|x|>1)**+-inf = inf,0
                    local_get(PW_IX); i32_const(0x3ff00000); i32_ge_s;
                    if_f64;
                        local_get(PW_HY); i32_const(0); i32_ge_s; if_f64; local_get(PW_Y); else_; f64_const(0.0); end;
                    else_;
                        // (|x|<1)**+-inf = 0,inf
                        local_get(PW_HY); i32_const(0); i32_ge_s; if_f64; f64_const(0.0); else_; local_get(PW_Y); f64_neg; end;
                    end;
                end;
                return_;
            end;
            // y is +-1
            local_get(PW_IY); i32_const(0x3ff00000); i32_eq;
            if_empty;
                local_get(PW_HY); i32_const(0); i32_ge_s;
                if_f64; local_get(PW_X); else_; f64_const(1.0); local_get(PW_X); f64_div; end;
                return_;
            end;
            // y is 2
            local_get(PW_HY); i32_const(0x40000000); i32_eq;
            if_empty; local_get(PW_X); local_get(PW_X); f64_mul; return_; end;
            // y is 0.5 and x >= +0
            local_get(PW_HY); i32_const(0x3fe00000); i32_eq;
            if_empty;
                local_get(PW_HX); i32_const(0); i32_ge_s;
                if_empty; local_get(PW_X); f64_sqrt; return_; end;
            end;
        end;

        // ax = |x|
        local_get(PW_X); f64_abs; local_set(PW_AX);
        // special value of x: if lx == 0
        local_get(PW_LX); i32_eqz;
        if_empty;
            // x is +-0,+-inf,+-1
            local_get(PW_IX); i32_const(0x7ff00000); i32_eq;
            local_get(PW_IX); i32_eqz; i32_or;
            local_get(PW_IX); i32_const(0x3ff00000); i32_eq; i32_or;
            if_empty;
                // z = ax
                local_get(PW_AX); local_set(PW_Z);
                // if hy < 0 { z = 1/z }
                local_get(PW_HY); i32_const(0); i32_lt_s;
                if_empty; f64_const(1.0); local_get(PW_Z); f64_div; local_set(PW_Z); end;
                // if hx < 0
                local_get(PW_HX); i32_const(0); i32_lt_s;
                if_empty;
                    // if ((ix-0x3ff00000)|yisint)==0 { z = (z-z)/(z-z) }  (-1)**non-int = NaN
                    local_get(PW_IX); i32_const(0x3ff00000); i32_sub; local_get(PW_YISINT); i32_or; i32_eqz;
                    if_empty;
                        local_get(PW_Z); local_get(PW_Z); f64_sub; local_get(PW_Z); local_get(PW_Z); f64_sub; f64_div; local_set(PW_Z);
                    else_;
                        // elif yisint == 1 { z = -z }
                        local_get(PW_YISINT); i32_const(1); i32_eq;
                        if_empty; local_get(PW_Z); f64_neg; local_set(PW_Z); end;
                    end;
                end;
                local_get(PW_Z); return_;
            end;
        end;

        // s = sign of result
        f64_const(1.0); local_set(PW_S);
        local_get(PW_HX); i32_const(0); i32_lt_s;
        if_empty;
            local_get(PW_YISINT); i32_eqz;
            if_empty;
                // (x<0)**(non-int) = NaN
                local_get(PW_X); local_get(PW_X); f64_sub; local_get(PW_X); local_get(PW_X); f64_sub; f64_div; return_;
            end;
            local_get(PW_YISINT); i32_const(1); i32_eq;
            if_empty; f64_const(-1.0); local_set(PW_S); end;
        end;

        // |y| is HUGE: if iy > 0x41e00000
        local_get(PW_IY); i32_const(0x41e00000); i32_gt_s;
        if_empty;
            // if iy > 0x43f00000  (|y| > 2^64, must o/uflow)
            local_get(PW_IY); i32_const(0x43f00000); i32_gt_s;
            if_empty;
                // if ix <= 0x3fefffff { return hy<0 ? HUGE*HUGE : TINY*TINY }
                local_get(PW_IX); i32_const(0x3fefffff); i32_le_s;
                if_empty;
                    local_get(PW_HY); i32_const(0); i32_lt_s;
                    if_f64; f64_const(POW_HUGE); f64_const(POW_HUGE); f64_mul; else_; f64_const(POW_TINY); f64_const(POW_TINY); f64_mul; end;
                    return_;
                end;
                // if ix >= 0x3ff00000 { return hy>0 ? HUGE*HUGE : TINY*TINY }
                local_get(PW_IX); i32_const(0x3ff00000); i32_ge_s;
                if_empty;
                    local_get(PW_HY); i32_const(0); i32_gt_s;
                    if_f64; f64_const(POW_HUGE); f64_const(POW_HUGE); f64_mul; else_; f64_const(POW_TINY); f64_const(POW_TINY); f64_mul; end;
                    return_;
                end;
            end;
            // over/underflow if x not close to one
            local_get(PW_IX); i32_const(0x3fefffff); i32_lt_s;
            if_empty;
                local_get(PW_HY); i32_const(0); i32_lt_s;
                if_f64; local_get(PW_S); f64_const(POW_HUGE); f64_mul; f64_const(POW_HUGE); f64_mul; else_; local_get(PW_S); f64_const(POW_TINY); f64_mul; f64_const(POW_TINY); f64_mul; end;
                return_;
            end;
            local_get(PW_IX); i32_const(0x3ff00000); i32_gt_s;
            if_empty;
                local_get(PW_HY); i32_const(0); i32_gt_s;
                if_f64; local_get(PW_S); f64_const(POW_HUGE); f64_mul; f64_const(POW_HUGE); f64_mul; else_; local_get(PW_S); f64_const(POW_TINY); f64_mul; f64_const(POW_TINY); f64_mul; end;
                return_;
            end;
            // |1-x| is TINY <= 2^-20: log(x) by x-x^2/2+x^3/3-x^4/4
            // t = ax - 1
            local_get(PW_AX); f64_const(1.0); f64_sub; local_set(PW_T);
            // w = (t*t)*(0.5 - t*(1/3 - t*0.25))
            local_get(PW_T); local_get(PW_T); f64_mul;
            f64_const(0.5); local_get(PW_T); f64_const(POW_ONE_THIRD); local_get(PW_T); f64_const(0.25); f64_mul; f64_sub; f64_mul; f64_sub;
            f64_mul; local_set(PW_W);
            // u = IVLN2_H * t
            local_get(PW_T); f64_const(POW_IVLN2_H); f64_mul; local_set(PW_U);
            // v = t*IVLN2_L - w*IVLN2
            local_get(PW_T); f64_const(POW_IVLN2_L); f64_mul; local_get(PW_W); f64_const(POW_IVLN2); f64_mul; f64_sub; local_set(PW_V);
            // t1 = with_set_low_word(u+v, 0); t2 = v - (t1 - u)
            local_get(PW_U); local_get(PW_V); f64_add;
        });
        emit_zero_low_word(&mut f);
        wasm!(f, {
            local_set(PW_T1);
            local_get(PW_V); local_get(PW_T1); local_get(PW_U); f64_sub; f64_sub; local_set(PW_T2);
        });

    // ── else: main log path (|y| not HUGE) ──
    wasm!(f, {
        else_;
            // n = 0
            i32_const(0); local_set(PW_N);
            // if ix < 0x00100000 { ax *= 2^53; n -= 53; ix = get_high_word(ax) }
            local_get(PW_IX); i32_const(0x00100000); i32_lt_s;
            if_empty;
                local_get(PW_AX); f64_const(POW_TWO53); f64_mul; local_set(PW_AX);
                local_get(PW_N); i32_const(53); i32_sub; local_set(PW_N);
                local_get(PW_AX);
    });
    emit_high_word(&mut f);
    wasm!(f, {
                local_set(PW_IX);
            end;
            // n += (ix >> 20) - 0x3ff; j = ix & 0x000fffff
            local_get(PW_N); local_get(PW_IX); i32_const(20); i32_shr_s; i32_const(0x3ff); i32_sub; i32_add; local_set(PW_N);
            local_get(PW_IX); i32_const(0x000fffff); i32_and; local_set(PW_J);
            // ix = j | 0x3ff00000
            local_get(PW_J); i32_const(0x3ff00000); i32_or; local_set(PW_IX);
            // determine interval kk:  j<=0x3988E -> 0; j<0xBB67A -> 1; else { 0; n++; ix-=0x00100000 }
            local_get(PW_J); i32_const(0x3988E); i32_le_s;
            if_empty;
                i32_const(0); local_set(PW_KK);
            else_;
                local_get(PW_J); i32_const(0xBB67A); i32_lt_s;
                if_empty;
                    i32_const(1); local_set(PW_KK);
                else_;
                    i32_const(0); local_set(PW_KK);
                    local_get(PW_N); i32_const(1); i32_add; local_set(PW_N);
                    local_get(PW_IX); i32_const(0x00100000); i32_sub; local_set(PW_IX);
                end;
            end;
            // ax = with_set_high_word(ax, ix)
            local_get(PW_AX); i64_reinterpret_f64; i64_const(0xffffffff); i64_and;
            local_get(PW_IX); i64_extend_i32_u; i64_const(HI_SHIFT); i64_shl;
            i64_or; f64_reinterpret_i64; local_set(PW_AX);

            // bp[kk]: kk==0 -> 1.0 ; kk==1 -> 1.5
            // u = ax - bp[kk]; v = 1/(ax + bp[kk]); ss = u*v; s_h = with_set_low_word(ss,0)
            local_get(PW_AX); local_get(PW_KK); if_f64; f64_const(1.5); else_; f64_const(1.0); end; f64_sub; local_set(PW_U);
            f64_const(1.0); local_get(PW_AX); local_get(PW_KK); if_f64; f64_const(1.5); else_; f64_const(1.0); end; f64_add; f64_div; local_set(PW_V);
            local_get(PW_U); local_get(PW_V); f64_mul; local_set(PW_SS);
            local_get(PW_SS);
    });
    emit_zero_low_word(&mut f);
    wasm!(f, {
            local_set(PW_S_H);
            // t_h = with_set_high_word(0.0, ((ix>>1)|0x20000000) + 0x00080000 + (kk<<18))
            local_get(PW_IX); i32_const(1); i32_shr_u; i32_const(0x20000000); i32_or;
            i32_const(0x00080000); i32_add;
            local_get(PW_KK); i32_const(18); i32_shl; i32_add;
    });
    emit_from_high_word(&mut f);
    wasm!(f, {
            local_set(PW_T_H);
            // t_l = ax - (t_h - bp[kk])
            local_get(PW_AX); local_get(PW_T_H); local_get(PW_KK); if_f64; f64_const(1.5); else_; f64_const(1.0); end; f64_sub; f64_sub; local_set(PW_T_L);
            // s_l = v*((u - s_h*t_h) - s_h*t_l)
            local_get(PW_V);
            local_get(PW_U); local_get(PW_S_H); local_get(PW_T_H); f64_mul; f64_sub;
            local_get(PW_S_H); local_get(PW_T_L); f64_mul; f64_sub;
            f64_mul; local_set(PW_S_L);

            // s2 = ss*ss; r = s2*s2*(L1+s2*(L2+s2*(L3+s2*(L4+s2*(L5+s2*L6)))))
            local_get(PW_SS); local_get(PW_SS); f64_mul; local_set(PW_S2);
            local_get(PW_S2); local_get(PW_S2); f64_mul;
            f64_const(POW_L1);
            local_get(PW_S2); f64_const(POW_L2);
            local_get(PW_S2); f64_const(POW_L3);
            local_get(PW_S2); f64_const(POW_L4);
            local_get(PW_S2); f64_const(POW_L5);
            local_get(PW_S2); f64_const(POW_L6); f64_mul; f64_add;
            f64_mul; f64_add;
            f64_mul; f64_add;
            f64_mul; f64_add;
            f64_mul; f64_add;
            f64_mul; local_set(PW_R);
            // r += s_l*(s_h + ss)
            local_get(PW_R); local_get(PW_S_L); local_get(PW_S_H); local_get(PW_SS); f64_add; f64_mul; f64_add; local_set(PW_R);
            // s2 = s_h*s_h
            local_get(PW_S_H); local_get(PW_S_H); f64_mul; local_set(PW_S2);
            // t_h = with_set_low_word(3 + s2 + r, 0)
            f64_const(3.0); local_get(PW_S2); f64_add; local_get(PW_R); f64_add;
    });
    emit_zero_low_word(&mut f);
    wasm!(f, {
            local_set(PW_T_H);
            // t_l = r - ((t_h - 3) - s2)
            local_get(PW_R); local_get(PW_T_H); f64_const(3.0); f64_sub; local_get(PW_S2); f64_sub; f64_sub; local_set(PW_T_L);
            // u = s_h*t_h; v = s_l*t_h + t_l*ss
            local_get(PW_S_H); local_get(PW_T_H); f64_mul; local_set(PW_U);
            local_get(PW_S_L); local_get(PW_T_H); f64_mul; local_get(PW_T_L); local_get(PW_SS); f64_mul; f64_add; local_set(PW_V);
            // p_h = with_set_low_word(u+v, 0); p_l = v - (p_h - u)
            local_get(PW_U); local_get(PW_V); f64_add;
    });
    emit_zero_low_word(&mut f);
    wasm!(f, {
            local_set(PW_P_H);
            local_get(PW_V); local_get(PW_P_H); local_get(PW_U); f64_sub; f64_sub; local_set(PW_P_L);
            // z_h = CP_H*p_h; z_l = CP_L*p_h + p_l*CP + dp_l[kk]
            f64_const(POW_CP_H); local_get(PW_P_H); f64_mul; local_set(PW_Z_H);
            f64_const(POW_CP_L); local_get(PW_P_H); f64_mul;
            local_get(PW_P_L); f64_const(POW_CP); f64_mul; f64_add;
            local_get(PW_KK); if_f64; f64_const(POW_DP_L1); else_; f64_const(0.0); end; f64_add;
            local_set(PW_Z_L);
            // t = n; t1 = with_set_low_word((z_h+z_l)+dp_h[kk]+t, 0)
            local_get(PW_N); f64_convert_i32_s; local_set(PW_T);
            local_get(PW_Z_H); local_get(PW_Z_L); f64_add;
            local_get(PW_KK); if_f64; f64_const(POW_DP_H1); else_; f64_const(0.0); end; f64_add;
            local_get(PW_T); f64_add;
    });
    emit_zero_low_word(&mut f);
    wasm!(f, {
            local_set(PW_T1);
            // t2 = z_l - (((t1 - t) - dp_h[kk]) - z_h)
            local_get(PW_Z_L);
            local_get(PW_T1); local_get(PW_T); f64_sub;
            local_get(PW_KK); if_f64; f64_const(POW_DP_H1); else_; f64_const(0.0); end; f64_sub;
            local_get(PW_Z_H); f64_sub;
            f64_sub; local_set(PW_T2);
        end; // end of HUGE if/else
    });

    // ── combine: (y1+y2)*(t1+t2), overflow/underflow, 2**(p_h+p_l) ──
    wasm!(f, {
        // y1 = with_set_low_word(y, 0)
        local_get(PW_Y);
    });
    emit_zero_low_word(&mut f);
    wasm!(f, {
        local_set(PW_Y1);
        // p_l = (y - y1)*t1 + y*t2 ; p_h = y1*t1 ; z = p_l + p_h
        local_get(PW_Y); local_get(PW_Y1); f64_sub; local_get(PW_T1); f64_mul;
        local_get(PW_Y); local_get(PW_T2); f64_mul; f64_add; local_set(PW_P_L);
        local_get(PW_Y1); local_get(PW_T1); f64_mul; local_set(PW_P_H);
        local_get(PW_P_L); local_get(PW_P_H); f64_add; local_set(PW_Z);
        // j = (z.bits >> 32) as i32 ; i = z.bits as i32
        local_get(PW_Z); i64_reinterpret_f64; i64_const(HI_SHIFT); i64_shr_u; i32_wrap_i64; local_set(PW_J);
        local_get(PW_Z); i64_reinterpret_f64; i32_wrap_i64; local_set(PW_I);

        // if j >= 0x40900000  (z >= 1024)
        local_get(PW_J); i32_const(0x40900000); i32_ge_s;
        if_empty;
            // if (j - 0x40900000) | i != 0  (z > 1024) -> overflow
            local_get(PW_J); i32_const(0x40900000); i32_sub; local_get(PW_I); i32_or; i32_const(0); i32_ne;
            if_empty; local_get(PW_S); f64_const(POW_HUGE); f64_mul; f64_const(POW_HUGE); f64_mul; return_; end;
            // if p_l + OVT > z - p_h -> overflow
            local_get(PW_P_L); f64_const(POW_OVT); f64_add; local_get(PW_Z); local_get(PW_P_H); f64_sub; f64_gt;
            if_empty; local_get(PW_S); f64_const(POW_HUGE); f64_mul; f64_const(POW_HUGE); f64_mul; return_; end;
        else_;
            // else if (j & 0x7fffffff) >= 0x4090cc00  (z <= -1075)
            local_get(PW_J); i32_const(0x7fffffff); i32_and; i32_const(0x4090cc00); i32_ge_s;
            if_empty;
                // if (((j as u32) - 0xc090cc00) | (i as u32)) != 0  (z < -1075) -> underflow
                local_get(PW_J); i32_const(0xc090cc00_u32 as i32); i32_sub; local_get(PW_I); i32_or; i32_const(0); i32_ne;
                if_empty; local_get(PW_S); f64_const(POW_TINY); f64_mul; f64_const(POW_TINY); f64_mul; return_; end;
                // if p_l <= z - p_h -> underflow
                local_get(PW_P_L); local_get(PW_Z); local_get(PW_P_H); f64_sub; f64_le;
                if_empty; local_get(PW_S); f64_const(POW_TINY); f64_mul; f64_const(POW_TINY); f64_mul; return_; end;
            end;
        end;

        // compute 2**(p_h+p_l): i = j & 0x7fffffff; k = (i>>20) - 0x3ff; n = 0
        local_get(PW_J); i32_const(0x7fffffff); i32_and; local_set(PW_I);
        local_get(PW_I); i32_const(20); i32_shr_s; i32_const(0x3ff); i32_sub; local_set(PW_K);
        i32_const(0); local_set(PW_N);
        // if i > 0x3fe00000  (|z| > 0.5)
        local_get(PW_I); i32_const(0x3fe00000); i32_gt_s;
        if_empty;
            // n = j + (0x00100000 >> (k+1))
            local_get(PW_J); i32_const(0x00100000); local_get(PW_K); i32_const(1); i32_add; i32_shr_s; i32_add; local_set(PW_N);
            // k = ((n & 0x7fffffff) >> 20) - 0x3ff
            local_get(PW_N); i32_const(0x7fffffff); i32_and; i32_const(20); i32_shr_s; i32_const(0x3ff); i32_sub; local_set(PW_K);
            // t = with_set_high_word(0.0, (n & !(0x000fffff >> k)))
            local_get(PW_N); i32_const(0x000fffff); local_get(PW_K); i32_shr_s; i32_const(-1); i32_xor; i32_and;
    });
    emit_from_high_word(&mut f);
    wasm!(f, {
            local_set(PW_T);
            // n = ((n & 0x000fffff) | 0x00100000) >> (20 - k)
            local_get(PW_N); i32_const(0x000fffff); i32_and; i32_const(0x00100000); i32_or; i32_const(20); local_get(PW_K); i32_sub; i32_shr_s; local_set(PW_N);
            // if j < 0 { n = -n }
            local_get(PW_J); i32_const(0); i32_lt_s;
            if_empty; i32_const(0); local_get(PW_N); i32_sub; local_set(PW_N); end;
            // p_h -= t
            local_get(PW_P_H); local_get(PW_T); f64_sub; local_set(PW_P_H);
        end;

        // t = with_set_low_word(p_l + p_h, 0)
        local_get(PW_P_L); local_get(PW_P_H); f64_add;
    });
    emit_zero_low_word(&mut f);
    wasm!(f, {
        local_set(PW_T);
        // u = t*LG2_H; v = (p_l - (t - p_h))*LG2 + t*LG2_L
        local_get(PW_T); f64_const(POW_LG2_H); f64_mul; local_set(PW_U);
        local_get(PW_P_L); local_get(PW_T); local_get(PW_P_H); f64_sub; f64_sub; f64_const(POW_LG2); f64_mul;
        local_get(PW_T); f64_const(POW_LG2_L); f64_mul; f64_add; local_set(PW_V);
        // z = u + v; w = v - (z - u); t = z*z
        local_get(PW_U); local_get(PW_V); f64_add; local_set(PW_Z);
        local_get(PW_V); local_get(PW_Z); local_get(PW_U); f64_sub; f64_sub; local_set(PW_W);
        local_get(PW_Z); local_get(PW_Z); f64_mul; local_set(PW_T);
        // t1 = z - t*(P1 + t*(P2 + t*(P3 + t*(P4 + t*P5))))
        local_get(PW_Z);
        local_get(PW_T);
        f64_const(POW_P1);
        local_get(PW_T); f64_const(POW_P2);
        local_get(PW_T); f64_const(POW_P3);
        local_get(PW_T); f64_const(POW_P4);
        local_get(PW_T); f64_const(POW_P5); f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul;
        f64_sub; local_set(PW_T1);
        // r = (z*t1)/(t1 - 2) - (w + z*w)
        local_get(PW_Z); local_get(PW_T1); f64_mul; local_get(PW_T1); f64_const(2.0); f64_sub; f64_div;
        local_get(PW_W); local_get(PW_Z); local_get(PW_W); f64_mul; f64_add;
        f64_sub; local_set(PW_R);
        // z = 1 - (r - z)
        f64_const(1.0); local_get(PW_R); local_get(PW_Z); f64_sub; f64_sub; local_set(PW_Z);
        // j = get_high_word(z); j += n << 20
        local_get(PW_Z);
    });
    emit_high_word(&mut f);
    wasm!(f, {
        local_get(PW_N); i32_const(20); i32_shl; i32_add; local_set(PW_J);
        // if (j >> 20) <= 0 { z = scalbn(z, n) } else { z = with_set_high_word(z, j) }
        local_get(PW_J); i32_const(20); i32_shr_s; i32_const(0); i32_le_s;
        if_empty;
            local_get(PW_Z); local_get(PW_N); call(scalbn); local_set(PW_Z);
        else_;
            local_get(PW_Z); i64_reinterpret_f64; i64_const(0xffffffff); i64_and;
            local_get(PW_J); i64_extend_i32_u; i64_const(HI_SHIFT); i64_shl;
            i64_or; f64_reinterpret_i64; local_set(PW_Z);
        end;
        // return s * z
        local_get(PW_S); local_get(PW_Z); f64_mul;
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.pow, type_idx, f));
}
