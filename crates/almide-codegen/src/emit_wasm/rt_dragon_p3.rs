// ───────────────────────── float.to_fixed ─────────────────────────
//
// The EXACT binary expansion of any finite f64 terminates: |x| = m·2^e with
// e >= -1074, so there are at most 1074 nonzero fractional digits. The digit
// generator detects R == 0 and stops doing bignum work past that point, padding
// the remaining `N` positions with '0' — so the work is bounded by the real
// expansion regardless of how large `decimals` is.

/// __float_to_fixed(f: f64, decimals: i64) -> i32 (String ptr).
///
/// Reproduces native `format!("{:.N}", f)` EXACTLY: the decimal is the f64's
/// exact binary value `m·2^e` rounded to N fractional places, round-half-to-EVEN
/// on the exact value (so 2.5@0 -> "2", 3.5@0 -> "4", 2.675@2 -> "2.67" because
/// 2.675 is really 2.67499...). It rides the Dragon4 big-integer machinery: the
/// value is the exact rational R/S (S a power of two), digits are generated
/// MSD-first by `R*=10; d=floor(R/S); R-=d*S`, and the cutoff is rounded by the
/// half-even `2R vs S` test — identical exact arithmetic to Rust's flt2dec, so
/// there is no `10^N` i64 overflow and no multiply-then-round error.
///
/// Special cases match Rust: NaN -> "NaN", +inf -> "inf", -inf -> "-inf";
/// the sign bit is honored (-0.0@2 -> "-0.00"). N<0 is clamped to 0.
pub(super) fn compile_float_to_fixed(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.float_to_fixed];
    let dr = DragonRefs::new(emitter);

    // Locals (after f64 param 0, i64 param 1 = decimals):
    //   2  base    i32  scratch block base (DragonRefs.ptr reads this)
    //   3  bits    i64  raw bits of |f|
    //   4  raw_exp i32
    //   5  raw_mant i64
    //   6  mant    i64  significand m (with implicit bit)
    //   7  e       i32  binary exponent
    //   8  neg     i32  sign flag
    //   9  n       i32  decimals (clamped >= 0)
    //  10  k       i32  decimal exponent (count of integer digits; <=0 ⇒ |x|<1)
    //  11  cnt     i32  number of meaningful generated digits = k + n (>=0)
    //  12  i       i32  general loop counter
    //  13  d       i32  current digit
    //  14  dp      i32  digit-buffer ptr
    //  15  dlen    i32  digits written into dp
    //  16  result  i32  string ptr
    //  17  out_len i32
    //  18  cursor  i32  write cursor (byte offset from data start)
    //  19  round   i32  round-up flag
    //  20  (reserved/unused)
    //  21  total   i32  total digit slots = max(k,0) + n
    //  22  start   i32  leading-zero slots before the first significant digit
    let mut f = Function::new([
        (1, ValType::I32),  // 2 base
        (1, ValType::I64),  // 3 bits
        (1, ValType::I32),  // 4 raw_exp
        (1, ValType::I64),  // 5 raw_mant
        (1, ValType::I64),  // 6 mant
        (1, ValType::I32),  // 7 e
        (1, ValType::I32),  // 8 neg
        (1, ValType::I32),  // 9 n
        (1, ValType::I32),  // 10 k
        (1, ValType::I32),  // 11 cnt
        (1, ValType::I32),  // 12 i
        (1, ValType::I32),  // 13 d
        (1, ValType::I32),  // 14 dp
        (1, ValType::I32),  // 15 dlen
        (1, ValType::I32),  // 16 result
        (1, ValType::I32),  // 17 out_len
        (1, ValType::I32),  // 18 cursor
        (1, ValType::I32),  // 19 round
        (1, ValType::I32),  // 20 (reserved/unused)
        (1, ValType::I32),  // 21 total (digit slots = max(k,0)+n)
        (1, ValType::I32),  // 22 start (leading-zero slots before the first real digit)
    ]);
    const BASE: u32 = 2; const BITS: u32 = 3; const RAW_EXP: u32 = 4; const RAW_MANT: u32 = 5;
    const MANT: u32 = 6; const E: u32 = 7; const NEG: u32 = 8; const N: u32 = 9;
    const K: u32 = 10; const CNT: u32 = 11; const I: u32 = 12; const D: u32 = 13;
    const DP: u32 = 14; const DLEN: u32 = 15; const RESULT: u32 = 16; const OUT_LEN: u32 = 17;
    const CURSOR: u32 = 18; const ROUND: u32 = 19;
    const TOTAL: u32 = 21; const START: u32 = 22;

    let s_nan = emitter.intern_string("NaN");
    let s_inf = emitter.intern_string("inf");
    let s_ninf = emitter.intern_string("-inf");

    // bits = reinterpret(f); n = clamp(decimals, 0, ..)
    wasm!(f, {
        local_get(0); i64_reinterpret_f64; local_set(BITS);
        local_get(1); i32_wrap_i64; local_set(N);
        local_get(N); i32_const(0); i32_lt_s; if_empty; i32_const(0); local_set(N); end;
    });

    // NaN / inf special cases (raw exp == 0x7FF).
    wasm!(f, {
        local_get(BITS); i64_const(52); i64_shr_u; i32_wrap_i64; i32_const(0x7FF); i32_and; local_set(RAW_EXP);
        local_get(BITS); i64_const(0x000F_FFFF_FFFF_FFFF); i64_and; local_set(RAW_MANT);
        local_get(RAW_EXP); i32_const(0x7FF); i32_eq;
        if_empty;
            local_get(RAW_MANT); i64_eqz; i32_eqz;
            if_empty; i32_const(s_nan as i32); return_; end;
            local_get(BITS); i64_const(0); i64_lt_s;
            if_i32; i32_const(s_ninf as i32); else_; i32_const(s_inf as i32); end;
            return_;
        end;
    });

    // neg = sign bit; work with |f| bits.
    wasm!(f, {
        local_get(BITS); i64_const(0); i64_lt_s; local_set(NEG);
        local_get(BITS); i64_const(0x7FFF_FFFF_FFFF_FFFF); i64_and; local_set(BITS);
        local_get(BITS); i64_const(52); i64_shr_u; i32_wrap_i64; i32_const(0x7FF); i32_and; local_set(RAW_EXP);
        local_get(BITS); i64_const(0x000F_FFFF_FFFF_FFFF); i64_and; local_set(RAW_MANT);
    });

    // Decompose: subnormal (raw_exp==0): mant=raw_mant, e=-1074; else mant=raw_mant+2^52, e=raw_exp-1075.
    wasm!(f, {
        local_get(RAW_EXP); i32_eqz;
        if_empty;
            local_get(RAW_MANT); local_set(MANT);
            i32_const(-1074); local_set(E);
        else_;
            local_get(RAW_MANT); i64_const(0x10_0000_0000_0000); i64_add; local_set(MANT);
            local_get(RAW_EXP); i32_const(1075); i32_sub; local_set(E);
        end;
    });

    // Allocate the Dragon4 scratch block (we use only R, S, TMP).
    wasm!(f, {
        i32_const(SCRATCH_BYTES as i32); call(emitter.rt.alloc); local_set(BASE);
        // Digit buffer: up to ~310 integer digits + n fraction digits + slack (for a
        // round-up carry that prepends one leading digit). The generation loop stops
        // producing significant digits once R hits 0, so n only sizes the buffer.
        i32_const(340); local_get(N); i32_add; i32_const(8); i32_add; call(emitter.rt.alloc); local_set(DP);
        i32_const(0); local_set(DLEN);
        i32_const(0); local_set(K);
    });
    // base must be in local 1 for DragonRefs.ptr; alias via a copy is impossible
    // (ptr() hardcodes local_get(1)). So we keep base in BASE and patch ptr via a
    // dedicated helper below that reads BASE instead. To keep DragonRefs usable we
    // simply move base into local 1 here is NOT possible (1 is the decimals param).
    // Instead this routine uses the bignum offsets with an explicit base in BASE.

    // ── Setup R/S exactly: value = R/S = mant·2^e ──
    // R bignum = mant (two limbs).
    set_bn_u64(&mut f, BASE, OFF_R, MANT);
    set_bn_small(&mut f, BASE, OFF_S, 1);
    wasm!(f, {
        local_get(E); i32_const(0); i32_ge_s;
        if_empty;
            // e >= 0: R <<= e ; S = 1
            local_get(BASE); i32_const(OFF_R as i32); i32_add; local_get(E); call(dr.shl);
        else_;
            // e < 0: S <<= (-e)
            local_get(BASE); i32_const(OFF_S as i32); i32_add; i32_const(0); local_get(E); i32_sub; call(dr.shl);
        end;
    });

    // ── Position: scale so value/10^k ∈ [0.1, 1), tracking k ──
    // if value == 0 (mant == 0): k stays 0, R stays 0 → all digits zero.
    wasm!(f, {
        local_get(MANT); i64_eqz; i32_eqz;
        if_empty;
            // value >= 1 ?  cmp(R, S) >= 0
            local_get(BASE); i32_const(OFF_R as i32); i32_add; local_get(BASE); i32_const(OFF_S as i32); i32_add; call(dr.cmp);
            i32_const(0); i32_ge_s;
            if_empty;
                // while cmp(R, S) >= 0 { S *= 10; k++ }
                block_empty; loop_empty;
                    local_get(BASE); i32_const(OFF_R as i32); i32_add; local_get(BASE); i32_const(OFF_S as i32); i32_add; call(dr.cmp);
                    i32_const(0); i32_lt_s; br_if(1);
                    local_get(BASE); i32_const(OFF_S as i32); i32_add; i32_const(10); call(dr.mul_small);
                    local_get(K); i32_const(1); i32_add; local_set(K);
                    br(0);
                end; end;
            else_;
                // while cmp(R*10, S) < 0 { R *= 10; k-- }
                block_empty; loop_empty;
                    // TMP = R; TMP *= 10; cmp(TMP, S) >= 0 → stop
                    local_get(BASE); i32_const(OFF_TMP as i32); i32_add; local_get(BASE); i32_const(OFF_R as i32); i32_add; call(dr.copy);
                    local_get(BASE); i32_const(OFF_TMP as i32); i32_add; i32_const(10); call(dr.mul_small);
                    local_get(BASE); i32_const(OFF_TMP as i32); i32_add; local_get(BASE); i32_const(OFF_S as i32); i32_add; call(dr.cmp);
                    i32_const(0); i32_ge_s; br_if(1);
                    local_get(BASE); i32_const(OFF_R as i32); i32_add; i32_const(10); call(dr.mul_small);
                    local_get(K); i32_const(1); i32_sub; local_set(K);
                    br(0);
                end; end;
            end;
        end;
    });

    // Digit-slot accounting. We materialize EVERY rendered digit into the buffer
    // (integer digits when k>0, plus all N fraction digits incl. leading zeros), so
    // the round-half-even carry can propagate uniformly and the render is a copy.
    //   cnt   = k + n        real (significant) digits from position k-1 down to -n.
    //   total = max(k,0) + n total digit slots (k integer digits when k>0, then n frac).
    //   start = max(-k,0)    leading-zero slots before the first significant digit
    //                        (only when k<=0; = total - max(cnt,0)).
    wasm!(f, {
        local_get(K); local_get(N); i32_add; local_set(CNT);
        local_get(K); i32_const(0); i32_gt_s; if_i32; local_get(K); else_; i32_const(0); end;
        local_get(N); i32_add; local_set(TOTAL);
        i32_const(0); local_get(K); i32_sub; i32_const(0); i32_gt_s;
        if_i32; i32_const(0); local_get(K); i32_sub; else_; i32_const(0); end;
        local_set(START);
    });

    // ── Generate `total` digit slots ──
    //   slot < start  : leading zero (only when k <= 0; no bignum work)
    //   R != 0        : R*=10; d = floor(R/S) via repeated subtraction; R -= d*S
    //   R == 0        : the exact expansion has ended → digit 0 (no bignum work)
    wasm!(f, {
        i32_const(0); local_set(DLEN);
        i32_const(0); local_set(I);
        block_empty; loop_empty;
            local_get(I); local_get(TOTAL); i32_ge_s; br_if(1);
            local_get(I); local_get(START); i32_lt_s;
            if_empty;
                // leading-zero slot (only when k <= 0): digit is 0, no bignum work.
                i32_const(0); local_set(D);
            else_;
                // Once R has been driven to 0, the EXACT expansion has ended and every
                // remaining digit is 0 — skip the bignum work. R is zero iff its len is
                // 1 and limb0 is 0. (This replaces a digit-count cap: it is exact and
                // bounds the work to the real expansion regardless of how large N is.)
                local_get(BASE); i32_const(OFF_R as i32); i32_add; i32_load(0); i32_const(1); i32_eq;
                local_get(BASE); i32_const((OFF_R + BN_HDR) as i32); i32_add; i32_load(0); i32_eqz;
                i32_and;
                if_empty;
                    i32_const(0); local_set(D);
                else_;
                    local_get(BASE); i32_const(OFF_R as i32); i32_add; i32_const(10); call(dr.mul_small);
                    i32_const(0); local_set(D);
                    block_empty; loop_empty;
                        local_get(BASE); i32_const(OFF_R as i32); i32_add; local_get(BASE); i32_const(OFF_S as i32); i32_add; call(dr.cmp);
                        i32_const(0); i32_lt_s; br_if(1);
                        local_get(BASE); i32_const(OFF_R as i32); i32_add; local_get(BASE); i32_const(OFF_S as i32); i32_add; call(dr.sub);
                        local_get(D); i32_const(1); i32_add; local_set(D);
                        br(0);
                    end; end;
                end;
            end;
            local_get(DP); local_get(DLEN); i32_add; local_get(D); i32_const(48); i32_add; i32_store8(0);
            local_get(DLEN); i32_const(1); i32_add; local_set(DLEN);
            local_get(I); i32_const(1); i32_add; local_set(I);
            br(0);
        end; end;
    });

    // ── Round half-to-even at position -n using `2R vs S` ──
    // When cnt <= 0 the digit loop ran 0 real steps, so R still holds the WHOLE value
    // and the cutoff -n is `-cnt` decades ABOVE it; scale S up by 10^(-cnt) so the
    // comparison is taken at position -n. (When cnt > 0, R is already the residue
    // below -n and -cnt <= 0, so no scaling.) Tie breaks to even via the last slot.
    wasm!(f, {
        local_get(CNT); i32_const(0); i32_lt_s;
        if_empty;
            // S *= 10, (-cnt) times.
            i32_const(0); local_set(I);
            block_empty; loop_empty;
                local_get(I); i32_const(0); local_get(CNT); i32_sub; i32_ge_s; br_if(1);
                local_get(BASE); i32_const(OFF_S as i32); i32_add; i32_const(10); call(dr.mul_small);
                local_get(I); i32_const(1); i32_add; local_set(I);
                br(0);
            end; end;
        end;
        // TMP = 2R; cmp(TMP, S).
        local_get(BASE); i32_const(OFF_TMP as i32); i32_add; local_get(BASE); i32_const(OFF_R as i32); i32_add; call(dr.copy);
        local_get(BASE); i32_const(OFF_TMP as i32); i32_add; i32_const(1); call(dr.shl);
        local_get(BASE); i32_const(OFF_TMP as i32); i32_add; local_get(BASE); i32_const(OFF_S as i32); i32_add; call(dr.cmp);
        local_set(D);
        i32_const(0); local_set(ROUND);
        local_get(D); i32_const(0); i32_gt_s;
        if_empty;
            i32_const(1); local_set(ROUND);                  // 2R > S → up
        else_;
            local_get(D); i32_eqz;
            if_empty;
                // exact half: round to even — up iff the last slot's digit is odd.
                // (total >= n >= 0; when total == 0, n == 0 and k <= 0, the units digit
                // is the implicit '0' → even → keep.)
                local_get(TOTAL); i32_eqz;
                if_empty;
                    i32_const(0); local_set(ROUND);
                else_;
                    local_get(DP); local_get(TOTAL); i32_const(1); i32_sub; i32_add; i32_load8_u(0);
                    i32_const(1); i32_and; local_set(ROUND);
                end;
            end;
        end;
    });

    // ── Apply round-up carry over digits[0..total]; overflow prepends '1', k++. ──
    wasm!(f, {
        local_get(ROUND);
        if_empty;
            local_get(TOTAL); local_set(I);
            block_empty; loop_empty;
                local_get(I); i32_eqz;
                if_empty;
                    // carry out of the most-significant slot: shift right by 1, set
                    // digits[0]='1', total++, k++ (a new leading integer digit).
                    local_get(DP); i32_const(1); i32_add; local_get(DP); local_get(TOTAL); memory_copy;
                    local_get(DP); i32_const(49); i32_store8(0);
                    local_get(TOTAL); i32_const(1); i32_add; local_set(TOTAL);
                    local_get(K); i32_const(1); i32_add; local_set(K);
                    br(2);
                end;
                local_get(I); i32_const(1); i32_sub; local_set(I);
                local_get(DP); local_get(I); i32_add; i32_load8_u(0); i32_const(57); i32_eq;
                if_empty;
                    local_get(DP); local_get(I); i32_add; i32_const(48); i32_store8(0);
                    br(1);
                else_;
                    local_get(DP); local_get(I); i32_add;
                    local_get(DP); local_get(I); i32_add; i32_load8_u(0); i32_const(1); i32_add;
                    i32_store8(0);
                    br(2);
                end;
            end; end;
        end;
    });
    // digits[0..total] now hold the rounded result: when k>0 the first k are integer
    // digits and the next n are the fraction; when k<=0 all `total`(=n) are fraction.

    // ── Compute out_len & render `[-]int.frac` ──
    // Layout cases mirror Rust format!("{:.n}"):
    //   k <= 0:  "[-]0." + (-k zeros) + (k+n digits)            (frac total = n)
    //   k >= 1, n == 0: "[-]" + (k int digits)                  (no point)
    //   k >= 1, n >= 1: "[-]" + (k int digits) + "." + (n frac digits)
    // Note dlen == (k>0 ? k : 0) + n after carry handling.
    wasm!(f, {
        // out_len = neg + body
        local_get(NEG);
        local_get(K); i32_const(0); i32_le_s;
        if_i32;
            // 2 ("0.") + (-k) + (k+n) == 2 + n ; but if n==0 then k<=0 means value<1 rounded:
            // Rust prints "0" with no point when n==0 and value<1 (e.g. 0.5@0="0").
            local_get(N); i32_eqz;
            if_i32;
                i32_const(1);                 // just "0"
            else_;
                i32_const(2); local_get(N); i32_add;   // "0." + n frac
            end;
        else_;
            local_get(N); i32_eqz;
            if_i32;
                local_get(K);                 // k integer digits
            else_;
                local_get(K); i32_const(1); i32_add; local_get(N); i32_add;  // k + "." + n
            end;
        end;
        i32_add; local_set(OUT_LEN);
    });

    // alloc string [len][cap][data...]
    wasm!(f, {
        local_get(OUT_LEN); i32_const(string_hdr() as i32); i32_add;
        call(emitter.rt.alloc); local_set(RESULT);
        local_get(RESULT); local_get(OUT_LEN); i32_store(0);
        local_get(RESULT); local_get(OUT_LEN); i32_store(string_cap_off() as u32, 0);
        i32_const(0); local_set(CURSOR);
        // sign
        local_get(NEG);
        if_empty;
            local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add; i32_const(45); i32_store8(0);
            local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
        end;
    });

    // branch on k
    wasm!(f, {
        local_get(K); i32_const(0); i32_le_s;
        if_empty;
            // value < 1 (after rounding). If n == 0 → just "0".
            local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add; i32_const(48); i32_store8(0);
            local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
            local_get(N); i32_eqz;
            if_empty;
                // done: just "0"
            else_;
                // '.' then all `total`(==n) fraction digits — the leading zeros are
                // already materialized in the buffer, so this is a straight copy.
                local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add; i32_const(46); i32_store8(0);
                local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
                i32_const(0); local_set(I);
                block_empty; loop_empty;
                    local_get(I); local_get(TOTAL); i32_ge_s; br_if(1);
                    local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add;
                    local_get(DP); local_get(I); i32_add; i32_load8_u(0); i32_store8(0);
                    local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
                    local_get(I); i32_const(1); i32_add; local_set(I);
                    br(0);
                end; end;
            end;
        else_;
            // value >= 1: k integer digits = digits[0..k], then (n>0) "." + digits[k..k+n]
            i32_const(0); local_set(I);
            block_empty; loop_empty;
                local_get(I); local_get(K); i32_ge_s; br_if(1);
                local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add;
                local_get(DP); local_get(I); i32_add; i32_load8_u(0); i32_store8(0);
                local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
                local_get(I); i32_const(1); i32_add; local_set(I);
                br(0);
            end; end;
            local_get(N); i32_eqz;
            if_empty; else_;
                local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add; i32_const(46); i32_store8(0);
                local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
                // digits[k .. total]  (I continues from k)
                block_empty; loop_empty;
                    local_get(I); local_get(TOTAL); i32_ge_s; br_if(1);
                    local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add;
                    local_get(DP); local_get(I); i32_add; i32_load8_u(0); i32_store8(0);
                    local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
                    local_get(I); i32_const(1); i32_add; local_set(I);
                    br(0);
                end; end;
            end;
        end;
    });

    wasm!(f, { local_get(RESULT); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.float_to_fixed, type_idx, f));
}

/// Set bignum at (base+off) to the i64 value in `loc` (two u32 limbs). Standalone
/// twin of `DragonRefs::set_u64` for callers that keep the scratch base in a local
/// OTHER than 1 (to_fixed keeps it in BASE because local 1 is its `decimals` param).
fn set_bn_u64(f: &mut Function, base: u32, off: u32, loc: u32) {
    wasm!(f, {
        local_get(base); i32_const((off + BN_HDR) as i32); i32_add; local_get(loc); i32_wrap_i64; i32_store(0);
        local_get(base); i32_const((off + BN_HDR + 4) as i32); i32_add; local_get(loc); i64_const(32); i64_shr_u; i32_wrap_i64; i32_store(0);
        local_get(base); i32_const(off as i32); i32_add;
        local_get(loc); i64_const(32); i64_shr_u; i64_eqz; if_i32; i32_const(1); else_; i32_const(2); end;
        i32_store(0);
    });
}
/// Set bignum at (base+off) to a small u32 constant (1 limb).
fn set_bn_small(f: &mut Function, base: u32, off: u32, v: u32) {
    wasm!(f, {
        local_get(base); i32_const(off as i32); i32_add; i32_const(1); i32_store(0);
        local_get(base); i32_const((off + BN_HDR) as i32); i32_add; i32_const(v as i32); i32_store(0);
    });
}

/// Bundle of helper func indices + emit-time conveniences for the driver.
struct DragonRefs {
    mul_small: u32,
    cmp: u32,
    add: u32,
    sub: u32,
    shl: u32,
    copy: u32,
}

impl DragonRefs {
    fn new(emitter: &WasmEmitter) -> DragonRefs {
        DragonRefs {
            mul_small: emitter.rt.dragon.mul_small,
            cmp: emitter.rt.dragon.cmp,
            add: emitter.rt.dragon.add,
            sub: emitter.rt.dragon.sub,
            shl: emitter.rt.dragon.shl,
            copy: emitter.rt.dragon.copy,
        }
    }
    // base ptr is in local 1; absolute ptr of bignum at offset `off` = base + off.
    fn ptr(&self, f: &mut Function, off: u32) {
        wasm!(f, { local_get(1); i32_const(off as i32); i32_add; });
    }
    /// bignum[off] = the i64 in local `loc` (split into two u32 limbs).
    fn set_u64(&self, f: &mut Function, off: u32, loc: u32) {
        // len: if hi != 0 -> 2 else 1
        wasm!(f, {
            // limb0 = (val & 0xFFFFFFFF)
            local_get(1); i32_const((off + BN_HDR) as i32); i32_add;
            local_get(loc); i32_wrap_i64; i32_store(0);
            // limb1 = (val >> 32)
            local_get(1); i32_const((off + BN_HDR + 4) as i32); i32_add;
            local_get(loc); i64_const(32); i64_shr_u; i32_wrap_i64; i32_store(0);
            // len = (hi != 0) ? 2 : 1
            local_get(1); i32_const(off as i32); i32_add;
            local_get(loc); i64_const(32); i64_shr_u; i64_eqz;
            if_i32; i32_const(1); else_; i32_const(2); end;
            i32_store(0);
        });
    }
    /// bignum[off] = small constant (1 limb).
    fn set_small(&self, f: &mut Function, off: u32, v: u32) {
        wasm!(f, {
            local_get(1); i32_const(off as i32); i32_add; i32_const(1); i32_store(0); // len = 1
            local_get(1); i32_const((off + BN_HDR) as i32); i32_add; i32_const(v as i32); i32_store(0);
        });
    }
    fn copy(&self, f: &mut Function, dst: u32, src: u32) {
        self.ptr(f, dst); self.ptr(f, src);
        wasm!(f, { call(self.copy); });
    }
    fn add(&self, f: &mut Function, dst: u32, src: u32) {
        self.ptr(f, dst); self.ptr(f, src);
        wasm!(f, { call(self.add); });
    }
    fn sub(&self, f: &mut Function, dst: u32, src: u32) {
        self.ptr(f, dst); self.ptr(f, src);
        wasm!(f, { call(self.sub); });
    }
    fn cmp(&self, f: &mut Function, a: u32, b: u32) {
        self.ptr(f, a); self.ptr(f, b);
        wasm!(f, { call(self.cmp); });
    }
    /// cmp(a, b) and store the -1/0/1 result into local 23 (`cmp_tmp`).
    fn cmp_set(&self, f: &mut Function, a: u32, b: u32) {
        self.cmp(f, a, b);
        wasm!(f, { local_set(23); });
    }
    /// Push the boolean `even ? (cmp_tmp >= 0) : (cmp_tmp > 0)`.
    /// (Used where the rounding interval is closed for even mantissas: the
    /// "upper" predicate.)  = (c > 0) | (even & (c == 0)).
    fn pred_ge_gt(&self, f: &mut Function) {
        wasm!(f, {
            local_get(23); i32_const(0); i32_gt_s;
            local_get(8); local_get(23); i32_eqz; i32_and;
            i32_or;
        });
    }
    /// Push the boolean `even ? (cmp_tmp <= 0) : (cmp_tmp < 0)`.
    /// (The "lower" predicate.)  = (c < 0) | (even & (c == 0)).
    fn pred_le_lt(&self, f: &mut Function) {
        wasm!(f, {
            local_get(23); i32_const(0); i32_lt_s;
            local_get(8); local_get(23); i32_eqz; i32_and;
            i32_or;
        });
    }
    fn mul10(&self, f: &mut Function, off: u32) {
        self.ptr(f, off);
        wasm!(f, { i32_const(10); call(self.mul_small); });
    }
    /// shl by a constant bit count.
    fn shl_const(&self, f: &mut Function, off: u32, bits: u32) {
        self.ptr(f, off);
        wasm!(f, { i32_const(bits as i32); call(self.shl); });
    }
    /// shl by the value in local `loc` (an i32).
    fn shl_local(&self, f: &mut Function, off: u32, loc: u32) {
        self.ptr(f, off);
        wasm!(f, { local_get(loc); call(self.shl); });
    }
    /// shl by (local + imm).
    fn shl_imm_local(&self, f: &mut Function, off: u32, loc: u32, imm: i32) {
        self.ptr(f, off);
        wasm!(f, { local_get(loc); i32_const(imm); i32_add; call(self.shl); });
    }
    /// shl by (1 - local).
    fn shl_one_minus_e(&self, f: &mut Function, off: u32, loc: u32) {
        self.ptr(f, off);
        wasm!(f, { i32_const(1); local_get(loc); i32_sub; call(self.shl); });
    }
}

/// log10(2), used to estimate the decimal exponent from the binary one.
const LOG10_2: f64 = core::f64::consts::LOG10_2;
