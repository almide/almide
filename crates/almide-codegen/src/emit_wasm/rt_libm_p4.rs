// ═══════════════════════════════ expm1 ═══════════════════════════════
// WASM twin of the vendored native expm1 in runtime/rs/src/libm_p4.rs
// (libm 0.2.16 / FreeBSD s_expm1.c). Same coefficients, same branch
// structure, same bit manipulations → bit-identical native↔wasm. Straight-line
// like exp/log (no scratch arrays, no tables); `__math_tanh` is its only
// caller today.

// ── expm1 constants (s_expm1.c) ──
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

// ──────────────────────────── __libm_expm1 ────────────────────────────
// Faithful port of libm `expm1` (s_expm1.c). See runtime/rs/src/libm_p4.rs.
fn compile_expm1(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.libm.expm1];
    let x1p1023 = f64::from_bits(0x7fe0000000000000);
    // params: 0=x (mutated, as in the Rust source). locals:
    //   1=i32 hx, 2=i32 sign, 3=i32 k,
    //   4=f64 hi, 5=f64 lo, 6=f64 c, 7=f64 t, 8=f64 y,
    //   9=f64 hfx, 10=f64 hxs, 11=f64 r1, 12=f64 e, 13=f64 twopk, 14=f64 uf
    const X: u32 = 0;
    const HX: u32 = 1; const SIGN: u32 = 2; const K: u32 = 3;
    const HI: u32 = 4; const LO: u32 = 5; const C: u32 = 6; const T: u32 = 7; const Y: u32 = 8;
    const HFX: u32 = 9; const HXS: u32 = 10; const R1: u32 = 11; const E: u32 = 12;
    const TWOPK: u32 = 13; const UF: u32 = 14;
    let mut f = Function::new([(3, ValType::I32), (11, ValType::F64)]);
    wasm!(f, {
        // hx = (to_bits(x) >> 32); sign = hx >> 31; hx &= 0x7fffffff
        local_get(X); i64_reinterpret_f64; i64_const(HI_SHIFT); i64_shr_u; i32_wrap_i64; local_set(HX);
        local_get(HX); i32_const(31); i32_shr_u; local_set(SIGN);
        local_get(HX); i32_const(0x7fffffff); i32_and; local_set(HX);

        // filter out huge and non-finite argument: if |x| >= 56*ln2
        local_get(HX); i32_const(0x4043687A); i32_ge_u;
        if_empty;
            // if x.is_nan() return x  (x != x)
            local_get(X); local_get(X); f64_ne;
            if_empty; local_get(X); return_; end;
            // if sign != 0 return -1.0
            local_get(SIGN);
            if_empty; f64_const(-1.0); return_; end;
            // if x > O_THRESHOLD { x *= 2^1023; return x }  (overflow)
            local_get(X); f64_const(EXPM1_O_THRESHOLD); f64_gt;
            if_empty; local_get(X); f64_const(x1p1023); f64_mul; return_; end;
        end;

        // argument reduction
        local_get(HX); i32_const(0x3fd62e42); i32_gt_u;
        if_empty;
            // |x| > 0.5 ln2
            local_get(HX); i32_const(0x3FF0A2B2); i32_lt_u;
            if_empty;
                // and |x| < 1.5 ln2
                local_get(SIGN); i32_eqz;
                if_empty;
                    local_get(X); f64_const(EXPM1_LN2_HI); f64_sub; local_set(HI);
                    f64_const(EXPM1_LN2_LO); local_set(LO);
                    i32_const(1); local_set(K);
                else_;
                    local_get(X); f64_const(EXPM1_LN2_HI); f64_add; local_set(HI);
                    f64_const(EXPM1_LN2_LO); f64_neg; local_set(LO);
                    i32_const(-1); local_set(K);
                end;
            else_;
                // k = (INVLN2*x + (sign? -0.5 : 0.5)) as i32
                f64_const(EXPM1_INVLN2); local_get(X); f64_mul;
                local_get(SIGN); if_f64; f64_const(-0.5); else_; f64_const(0.5); end;
                f64_add; i32_trunc_f64_s; local_set(K);
                // t = k as f64; hi = x - t*ln2_hi (exact); lo = t*ln2_lo
                local_get(K); f64_convert_i32_s; local_set(T);
                local_get(X); local_get(T); f64_const(EXPM1_LN2_HI); f64_mul; f64_sub; local_set(HI);
                local_get(T); f64_const(EXPM1_LN2_LO); f64_mul; local_set(LO);
            end;
            // x = hi - lo; c = (hi - x) - lo
            local_get(HI); local_get(LO); f64_sub; local_set(X);
            local_get(HI); local_get(X); f64_sub; local_get(LO); f64_sub; local_set(C);
        else_;
            // if |x| < 2^-54 return x (upstream force_eval only raises underflow)
            local_get(HX); i32_const(0x3c900000); i32_lt_u;
            if_empty; local_get(X); return_; end;
            f64_const(0.0); local_set(C);
            i32_const(0); local_set(K);
        end;

        // x is now in primary range: hfx = 0.5*x; hxs = x*hfx
        f64_const(0.5); local_get(X); f64_mul; local_set(HFX);
        local_get(X); local_get(HFX); f64_mul; local_set(HXS);
        // r1 = 1 + hxs*(Q1 + hxs*(Q2 + hxs*(Q3 + hxs*(Q4 + hxs*Q5))))
        f64_const(1.0);
        local_get(HXS);
        f64_const(EXPM1_Q1);
        local_get(HXS); f64_const(EXPM1_Q2);
        local_get(HXS); f64_const(EXPM1_Q3);
        local_get(HXS); f64_const(EXPM1_Q4);
        local_get(HXS); f64_const(EXPM1_Q5); f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        f64_mul; f64_add;
        local_set(R1);
        // t = 3 - r1*hfx
        f64_const(3.0); local_get(R1); local_get(HFX); f64_mul; f64_sub; local_set(T);
        // e = hxs*((r1-t)/(6 - x*t))
        local_get(HXS);
        local_get(R1); local_get(T); f64_sub;
        f64_const(6.0); local_get(X); local_get(T); f64_mul; f64_sub;
        f64_div; f64_mul; local_set(E);
        // if k == 0 return x - (x*e - hxs)   (c is 0)
        local_get(K); i32_eqz;
        if_empty;
            local_get(X); local_get(X); local_get(E); f64_mul; local_get(HXS); f64_sub; f64_sub; return_;
        end;
        // e = x*(e-c) - c; e -= hxs
        local_get(X); local_get(E); local_get(C); f64_sub; f64_mul; local_get(C); f64_sub; local_set(E);
        local_get(E); local_get(HXS); f64_sub; local_set(E);
        // if k == -1 return 0.5*(x-e) - 0.5
        local_get(K); i32_const(-1); i32_eq;
        if_empty;
            f64_const(0.5); local_get(X); local_get(E); f64_sub; f64_mul; f64_const(0.5); f64_sub; return_;
        end;
        // if k == 1 { if x < -0.25 return -2*(e-(x+0.5)); return 1 + 2*(x-e) }
        local_get(K); i32_const(1); i32_eq;
        if_empty;
            local_get(X); f64_const(-0.25); f64_lt;
            if_empty;
                f64_const(-2.0); local_get(E); local_get(X); f64_const(0.5); f64_add; f64_sub; f64_mul; return_;
            end;
            f64_const(1.0); f64_const(2.0); local_get(X); local_get(E); f64_sub; f64_mul; f64_add; return_;
        end;
        // twopk = from_bits(((0x3ff + k) as u64) << 52)   (2^k)
        i32_const(0x3ff); local_get(K); i32_add; i64_extend_i32_s; i64_const(52); i64_shl; f64_reinterpret_i64; local_set(TWOPK);
        // if k < 0 || k > 56: suffice to return exp(x)-1
        local_get(K); i32_const(0); i32_lt_s;
        local_get(K); i32_const(56); i32_gt_s;
        i32_or;
        if_empty;
            local_get(X); local_get(E); f64_sub; f64_const(1.0); f64_add; local_set(Y);
            local_get(K); i32_const(1024); i32_eq;
            if_empty;
                local_get(Y); f64_const(2.0); f64_mul; f64_const(x1p1023); f64_mul; local_set(Y);
            else_;
                local_get(Y); local_get(TWOPK); f64_mul; local_set(Y);
            end;
            local_get(Y); f64_const(1.0); f64_sub; return_;
        end;
        // uf = from_bits(((0x3ff - k) as u64) << 52)   (2^-k)
        i32_const(0x3ff); local_get(K); i32_sub; i64_extend_i32_s; i64_const(52); i64_shl; f64_reinterpret_i64; local_set(UF);
        // if k < 20 { (x - e + (1 - uf)) * twopk } else { (x - (e+uf) + 1) * twopk }
        local_get(K); i32_const(20); i32_lt_s;
        if_f64;
            local_get(X); local_get(E); f64_sub; f64_const(1.0); local_get(UF); f64_sub; f64_add; local_get(TWOPK); f64_mul;
        else_;
            local_get(X); local_get(E); local_get(UF); f64_add; f64_sub; f64_const(1.0); f64_add; local_get(TWOPK); f64_mul;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.libm.expm1, type_idx, f));
}
