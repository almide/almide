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

