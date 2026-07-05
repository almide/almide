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
