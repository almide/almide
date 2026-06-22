// ───────────────────────── driver ─────────────────────────

/// __float_to_string(f: f64) -> i32 (String ptr).
///
/// Mirrors the validated Rust prototype: decompose to f·2^e, run Dragon4
/// to get the shortest decimal digits + decimal exponent k, then render in
/// fixed notation with the `.0` integer-suffix rule.
fn compile_float_to_string(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.float_to_string];
    let dr = DragonRefs::new(emitter);

    // Locals (after the f64 param 0):
    //  1  base        i32  scratch block base ptr
    //  2  bits        i64  raw bits of |x|
    //  3  raw_exp     i32
    //  4  raw_mant    i64
    //  5  f           i64  mantissa
    //  6  e           i32  binary exponent
    //  7  k           i32  decimal exponent
    //  8  even        i32  mantissa parity (1 = even)
    //  9  low_bnd     i32  asymmetric-low-boundary flag
    // 10  dlen        i32  number of generated digits
    // 11  neg         i32  sign flag
    // 12  d           i32  current digit value
    // 13  low_t       i32
    // 14  high_t      i32
    // 15  round_up    i32
    // 16  i           i32  general loop counter
    // 17  (reserved)  f64  spare f64 scratch (k is computed on the value stack)
    // 18  result      i32  string ptr
    // 19  out_len     i32
    // 20  dp          i32  digit-buffer ptr (base + OFF_DIGITS)
    // 21  m           i32  digit count (alias of dlen for render)
    // 22  carry_i     i32  carry index for round-up
    // 23  cmp_tmp     i32  scratch for a bignum comparison result
    let mut f = Function::new([
        (1, ValType::I32), // 1 base
        (1, ValType::I64), // 2 bits
        (1, ValType::I32), // 3 raw_exp
        (1, ValType::I64), // 4 raw_mant
        (1, ValType::I64), // 5 f
        (1, ValType::I32), // 6 e
        (1, ValType::I32), // 7 k
        (1, ValType::I32), // 8 even
        (1, ValType::I32), // 9 low_bnd
        (1, ValType::I32), // 10 dlen
        (1, ValType::I32), // 11 neg
        (1, ValType::I32), // 12 d
        (1, ValType::I32), // 13 low_t
        (1, ValType::I32), // 14 high_t
        (1, ValType::I32), // 15 round_up
        (1, ValType::I32), // 16 i
        (1, ValType::F64), // 17 approx
        (1, ValType::I32), // 18 result
        (1, ValType::I32), // 19 out_len
        (1, ValType::I32), // 20 dp
        (1, ValType::I32), // 21 m (digit count, alias of dlen for render)
        (1, ValType::I32), // 22 carry_i / render index
        (1, ValType::I32), // 23 cmp_tmp
    ]);

    // ── Special cases: NaN / inf / zero ──
    // bits = reinterpret(x); exp field; mant field.
    wasm!(f, {
        local_get(0); i64_reinterpret_f64; local_set(2);
    });
    // NaN: exp==0x7FF && mant!=0  → "NaN"
    let s_nan = emitter.intern_string("NaN");
    let s_inf = emitter.intern_string("inf");
    let s_ninf = emitter.intern_string("-inf");
    let s_zero = emitter.intern_string("0.0");
    let s_nzero = emitter.intern_string("-0.0");
    wasm!(f, {
        // exp = (bits >> 52) & 0x7FF
        local_get(2); i64_const(52); i64_shr_u; i32_wrap_i64; i32_const(0x7FF); i32_and; local_set(3);
        // raw_mant = bits & 0xF_FFFF_FFFF_FFFF
        local_get(2); i64_const(0x000F_FFFF_FFFF_FFFF); i64_and; local_set(4);
        // if exp == 0x7FF: NaN or inf
        local_get(3); i32_const(0x7FF); i32_eq;
        if_empty;
          local_get(4); i64_eqz; i32_eqz;
          if_empty;
            i32_const(s_nan as i32); return_;
          end;
          // inf: sign bit
          local_get(2); i64_const(0); i64_lt_s;
          if_i32; i32_const(s_ninf as i32); else_; i32_const(s_inf as i32); end;
          return_;
        end;
    });
    // zero: x == 0.0 (covers +0 and -0). Sign from bit 63.
    wasm!(f, {
        local_get(0); f64_const(0.0); f64_eq;
        if_empty;
          local_get(2); i64_const(0); i64_lt_s;
          if_i32; i32_const(s_nzero as i32); else_; i32_const(s_zero as i32); end;
          return_;
        end;
    });

    // neg = sign bit
    wasm!(f, {
        local_get(2); i64_const(0); i64_lt_s; local_set(11);
    });

    // Work with |x|: clear sign bit.
    wasm!(f, {
        local_get(2); i64_const(0x7FFF_FFFF_FFFF_FFFF); i64_and; local_set(2);
        // re-extract exp/mant from |bits| (exp/mant already sign-independent, but recompute mant cleanly)
        local_get(2); i64_const(52); i64_shr_u; i32_wrap_i64; i32_const(0x7FF); i32_and; local_set(3);
        local_get(2); i64_const(0x000F_FFFF_FFFF_FFFF); i64_and; local_set(4);
    });

    // Decompose: if exp==0 (subnormal): f=raw_mant, e=-1074
    //            else:                  f=raw_mant + 2^52, e=exp-1075
    wasm!(f, {
        local_get(3); i32_eqz;
        if_empty;
          local_get(4); local_set(5);
          i32_const(-1074); local_set(6);
        else_;
          local_get(4); i64_const(0x10_0000_0000_0000); i64_add; local_set(5);
          local_get(3); i32_const(1075); i32_sub; local_set(6);
        end;
        // even = (f & 1) == 0
        local_get(5); i64_const(1); i64_and; i64_eqz; local_set(8);
        // low_bnd = raw_mant == 0 && exp > 1
        local_get(4); i64_eqz; local_get(3); i32_const(1); i32_gt_s; i32_and; local_set(9);
    });

    // Allocate scratch block.
    wasm!(f, {
        i32_const(SCRATCH_BYTES as i32); call(emitter.rt.alloc); local_set(1);
        local_get(1); i32_const(OFF_DIGITS as i32); i32_add; local_set(20);
    });

    // ── Initialize R, S, MP, MM as bignums set to f and 1 ──
    // Helper: set bignum at (base+off) to the i64 value on stack-top is awkward;
    // we inline "set to f" by storing low/high limbs, and "set to 1".
    // R = f (then shifted). S/MP/MM start as small ints.
    dr.set_u64(&mut f, OFF_R, 5);   // R = f
    if true {
        // e >= 0 branch vs e < 0 branch
        wasm!(f, {
            local_get(6); i32_const(0); i32_ge_s;
            if_empty;
        });
        // R = f << (e+1); S = 2; MP = 2^e; MM = 2^e
        dr.shl_imm_local(&mut f, OFF_R, 6, 1); // R <<= e+1
        dr.set_small(&mut f, OFF_S, 2);
        dr.set_small(&mut f, OFF_MP, 1);
        dr.shl_local(&mut f, OFF_MP, 6);        // MP <<= e
        dr.set_small(&mut f, OFF_MM, 1);
        dr.shl_local(&mut f, OFF_MM, 6);        // MM <<= e
        wasm!(f, {
            else_;
        });
        // R = f << 1; S = 1 << (1 - e); MP = 1; MM = 1
        dr.shl_const(&mut f, OFF_R, 1);
        dr.set_small(&mut f, OFF_S, 1);
        dr.shl_one_minus_e(&mut f, OFF_S, 6);   // S <<= (1 - e)
        dr.set_small(&mut f, OFF_MP, 1);
        dr.set_small(&mut f, OFF_MM, 1);
        wasm!(f, {
            end;
        });
    }
    // low boundary: MP <<= 1; R <<= 1; S <<= 1
    wasm!(f, {
        local_get(9);
        if_empty;
    });
    dr.shl_const(&mut f, OFF_MP, 1);
    dr.shl_const(&mut f, OFF_R, 1);
    dr.shl_const(&mut f, OFF_S, 1);
    wasm!(f, { end; });

    // ── Estimate k = ceil(log10(f) + e*log10(2)) ──
    // approx = log10((f64)f) + e * log10(2)
    // The fixup loops below correct any off-by-one in this estimate, so its
    // only requirement is to be close — exactness is not needed.
    wasm!(f, {
        local_get(5); f64_convert_i64_u;
        call(emitter.rt.math_log10);
        local_get(6); f64_convert_i32_s; f64_const(LOG10_2); f64_mul;
        f64_add;
        f64_ceil;
        // k = (i32) ceil(approx). k is in [-324, 309]; trunc-toward-zero is safe.
    });
    f.instruction(&wasm_encoder::Instruction::I32TruncF64S);
    wasm!(f, { local_set(7); });

    // ── Scale: if k>=0 S *= 10^k else R,MP,MM *= 10^(-k) ──
    // loop k times
    wasm!(f, {
        local_get(7); i32_const(0); i32_ge_s;
        if_empty;
          i32_const(0); local_set(16);
          block_empty; loop_empty;
            local_get(16); local_get(7); i32_ge_s; br_if(1);
    });
    dr.mul10(&mut f, OFF_S);
    wasm!(f, {
            local_get(16); i32_const(1); i32_add; local_set(16);
            br(0);
          end; end;
        else_;
          i32_const(0); local_set(16);
          block_empty; loop_empty;
            local_get(16); i32_const(0); local_get(7); i32_sub; i32_ge_s; br_if(1);
    });
    dr.mul10(&mut f, OFF_R);
    dr.mul10(&mut f, OFF_MP);
    dr.mul10(&mut f, OFF_MM);
    wasm!(f, {
            local_get(16); i32_const(1); i32_add; local_set(16);
            br(0);
          end; end;
        end;
    });

    // ── Fixup loop 1: while (R+MP) cmp S too_big: S*=10; k++ ──
    // too_big = even ? (R+MP >= S) : (R+MP > S)
    wasm!(f, {
        block_empty; loop_empty;
    });
    dr.copy(&mut f, OFF_TMP, OFF_R);
    dr.add(&mut f, OFF_TMP, OFF_MP);
    dr.cmp_set(&mut f, OFF_TMP, OFF_S); // cmp result -> local 23
    // too_big = even ? (c >= 0) : (c > 0)
    dr.pred_ge_gt(&mut f);
    wasm!(f, {
        i32_eqz; br_if(1); // not too_big -> break
    });
    dr.mul10(&mut f, OFF_S);
    wasm!(f, {
        local_get(7); i32_const(1); i32_add; local_set(7);
        br(0);
        end; end;
    });

    // ── Fixup loop 2: while (R+MP)*10 cmp S too_small: R,MP,MM*=10; k-- ──
    // too_small = even ? ((R+MP)*10 <= S) : ((R+MP)*10 < S)
    wasm!(f, {
        block_empty; loop_empty;
    });
    dr.copy(&mut f, OFF_TMP, OFF_R);
    dr.add(&mut f, OFF_TMP, OFF_MP);
    dr.mul10(&mut f, OFF_TMP);
    dr.cmp_set(&mut f, OFF_TMP, OFF_S);
    // too_small = even ? (c <= 0) : (c < 0)
    dr.pred_le_lt(&mut f);
    wasm!(f, {
        i32_eqz; br_if(1);
    });
    dr.mul10(&mut f, OFF_R);
    dr.mul10(&mut f, OFF_MP);
    dr.mul10(&mut f, OFF_MM);
    wasm!(f, {
        local_get(7); i32_const(1); i32_sub; local_set(7);
        br(0);
        end; end;
    });

    // ── Digit generation loop ──
    wasm!(f, {
        i32_const(0); local_set(10); // dlen = 0
        block_empty; loop_empty;     // [outer block (1)] [loop (0)]
    });
    dr.mul10(&mut f, OFF_R);
    dr.mul10(&mut f, OFF_MP);
    dr.mul10(&mut f, OFF_MM);
    // d = 0; while cmp(R,S) >= 0 { R -= S; d++ }
    wasm!(f, {
        i32_const(0); local_set(12);
        block_empty; loop_empty;
    });
    dr.cmp(&mut f, OFF_R, OFF_S);
    wasm!(f, {
          i32_const(0); i32_lt_s; br_if(1); // c < 0 -> stop
    });
    dr.sub(&mut f, OFF_R, OFF_S);
    wasm!(f, {
          local_get(12); i32_const(1); i32_add; local_set(12);
          br(0);
        end; end;
    });
    // low_t = even ? cmp(R,MM)<=0 : cmp(R,MM)<0
    dr.cmp_set(&mut f, OFF_R, OFF_MM);
    dr.pred_le_lt(&mut f);
    wasm!(f, { local_set(13); });
    // high_t = even ? cmp(R+MP,S)>=0 : cmp(R+MP,S)>0
    dr.copy(&mut f, OFF_TMP, OFF_R);
    dr.add(&mut f, OFF_TMP, OFF_MP);
    dr.cmp_set(&mut f, OFF_TMP, OFF_S);
    dr.pred_ge_gt(&mut f);
    wasm!(f, { local_set(14); });
    // if !low_t && !high_t: emit d, continue
    wasm!(f, {
        local_get(13); i32_eqz; local_get(14); i32_eqz; i32_and;
        if_empty;
          // digits[dlen] = '0'+d ; dlen++
          local_get(20); local_get(10); i32_add;
          local_get(12); i32_const(48); i32_add; i32_store8(0);
          local_get(10); i32_const(1); i32_add; local_set(10);
          br(1); // continue outer loop
        end;
    });
    // terminate: round_up = ?
    // if low_t && !high_t: round_up=0
    // elif high_t && !low_t: round_up=1
    // else: round_up = (2R cmp S) >= 0
    wasm!(f, {
        local_get(13); local_get(14); i32_eqz; i32_and;
        if_empty;
          i32_const(0); local_set(15);
        else_;
          local_get(14); local_get(13); i32_eqz; i32_and;
          if_empty;
            i32_const(1); local_set(15);
          else_;
    });
    // 2R cmp S
    dr.copy(&mut f, OFF_TMP, OFF_R);
    dr.shl_const(&mut f, OFF_TMP, 1);
    dr.cmp(&mut f, OFF_TMP, OFF_S);
    wasm!(f, {
            i32_const(0); i32_ge_s; local_set(15);
          end;
        end;
    });
    // digits[dlen] = '0'+d ; dlen++
    wasm!(f, {
        local_get(20); local_get(10); i32_add;
        local_get(12); i32_const(48); i32_add; i32_store8(0);
        local_get(10); i32_const(1); i32_add; local_set(10);
    });
    // if round_up: propagate carry from the last digit
    wasm!(f, {
        local_get(15);
        if_empty;
          // i = dlen
          local_get(10); local_set(22);
          block_empty; loop_empty;
            // if i == 0: prepend '1', k++, break
            local_get(22); i32_eqz;
            if_empty;
              // shift digits right by 1: memmove digits[0..dlen] -> digits[1..dlen+1]
    });
    // memory.copy(dst=dp+1, src=dp, len=dlen)
    wasm!(f, {
              local_get(20); i32_const(1); i32_add;
              local_get(20);
              local_get(10);
              memory_copy;
              local_get(20); i32_const(49); i32_store8(0); // '1'
              local_get(10); i32_const(1); i32_add; local_set(10);
              local_get(7); i32_const(1); i32_add; local_set(7);
              br(2); // break carry loop
            end;
            // i--
            local_get(22); i32_const(1); i32_sub; local_set(22);
            // if digits[i] == '9': set '0', continue loop; else digits[i]++, break.
            // Depths from inside this if/else: if-block=0, loop=1, block=2.
            local_get(20); local_get(22); i32_add; i32_load8_u(0);
            i32_const(57); i32_eq; // '9'
            if_empty;
              local_get(20); local_get(22); i32_add; i32_const(48); i32_store8(0);
              br(1); // continue carry loop
            else_;
              local_get(20); local_get(22); i32_add;
              local_get(20); local_get(22); i32_add; i32_load8_u(0); i32_const(1); i32_add;
              i32_store8(0);
              br(2); // break carry loop
            end;
          end; end;
        end;
    });
    // break outer digit loop
    wasm!(f, {
        br(1);
        end; end; // end loop, end outer block
    });

    // ── Render: dlen=m digits in buffer (dp), exponent k, sign neg ──
    // Compute out_len, alloc string, fill.
    // Cases:
    //   k <= 0:  "[-]0." + (-k) zeros + digits        len = neg + 2 + (-k) + m
    //   k >= m:  "[-]" + digits + (k-m) zeros + ".0"   len = neg + m + (k-m) + 2
    //   else:    "[-]" + digits[0..k] + "." + digits[k..m]  len = neg + m + 1
    //
    // result buffer: string header then data. We compute total data len, alloc,
    // then write characters sequentially using a cursor.
    wasm!(f, {
        local_get(10); local_set(21); // m = dlen (alias)
    });

    // Compute out_len into local 19.
    wasm!(f, {
        // start with neg
        local_get(11);
        // + branch
        local_get(7); i32_const(0); i32_le_s;
        if_i32;
          // 2 + (-k) + m
          i32_const(2); i32_const(0); local_get(7); i32_sub; i32_add; local_get(21); i32_add;
        else_;
          local_get(7); local_get(21); i32_ge_s;
          if_i32;
            // m + (k-m) + 2  == k + 2
            local_get(7); i32_const(2); i32_add;
          else_;
            // m + 1
            local_get(21); i32_const(1); i32_add;
          end;
        end;
        i32_add;
        local_set(19);
    });

    // Alloc string: header + out_len, set len & cap.
    wasm!(f, {
        local_get(19); i32_const(string_hdr() as i32); i32_add;
        call(emitter.rt.alloc); local_set(18);
        local_get(18); local_get(19); i32_store(0);
        local_get(18); local_get(19); i32_store(string_cap_off() as u32, 0);
    });

    // Write characters. Use local 16 as write cursor (byte offset from data start),
    // local 22 as a scratch index.
    wasm!(f, {
        i32_const(0); local_set(16); // cursor
        // sign
        local_get(11);
        if_empty;
          local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
          i32_const(45); i32_store8(0); // '-'
          local_get(16); i32_const(1); i32_add; local_set(16);
        end;
    });

    // Branch on k.
    wasm!(f, {
        local_get(7); i32_const(0); i32_le_s;
        if_empty;
          // "0." then (-k) zeros then digits
          local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
          i32_const(48); i32_store8(0); // '0'
          local_get(16); i32_const(1); i32_add; local_set(16);
          local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
          i32_const(46); i32_store8(0); // '.'
          local_get(16); i32_const(1); i32_add; local_set(16);
          // (-k) zeros
          i32_const(0); local_set(22);
          block_empty; loop_empty;
            local_get(22); i32_const(0); local_get(7); i32_sub; i32_ge_s; br_if(1);
            local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
            i32_const(48); i32_store8(0);
            local_get(16); i32_const(1); i32_add; local_set(16);
            local_get(22); i32_const(1); i32_add; local_set(22);
            br(0);
          end; end;
          // digits
          i32_const(0); local_set(22);
          block_empty; loop_empty;
            local_get(22); local_get(21); i32_ge_s; br_if(1);
            local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
            local_get(20); local_get(22); i32_add; i32_load8_u(0);
            i32_store8(0);
            local_get(16); i32_const(1); i32_add; local_set(16);
            local_get(22); i32_const(1); i32_add; local_set(22);
            br(0);
          end; end;
        else_;
          local_get(7); local_get(21); i32_ge_s;
          if_empty;
            // digits then (k-m) zeros then ".0"
            i32_const(0); local_set(22);
            block_empty; loop_empty;
              local_get(22); local_get(21); i32_ge_s; br_if(1);
              local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
              local_get(20); local_get(22); i32_add; i32_load8_u(0);
              i32_store8(0);
              local_get(16); i32_const(1); i32_add; local_set(16);
              local_get(22); i32_const(1); i32_add; local_set(22);
              br(0);
            end; end;
            // (k - m) zeros
            i32_const(0); local_set(22);
            block_empty; loop_empty;
              local_get(22); local_get(7); local_get(21); i32_sub; i32_ge_s; br_if(1);
              local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
              i32_const(48); i32_store8(0);
              local_get(16); i32_const(1); i32_add; local_set(16);
              local_get(22); i32_const(1); i32_add; local_set(22);
              br(0);
            end; end;
            // ".0"
            local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
            i32_const(46); i32_store8(0);
            local_get(16); i32_const(1); i32_add; local_set(16);
            local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
            i32_const(48); i32_store8(0);
            local_get(16); i32_const(1); i32_add; local_set(16);
          else_;
            // digits[0..k] "." digits[k..m]
            i32_const(0); local_set(22);
            block_empty; loop_empty;
              local_get(22); local_get(7); i32_ge_s; br_if(1);
              local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
              local_get(20); local_get(22); i32_add; i32_load8_u(0);
              i32_store8(0);
              local_get(16); i32_const(1); i32_add; local_set(16);
              local_get(22); i32_const(1); i32_add; local_set(22);
              br(0);
            end; end;
            local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
            i32_const(46); i32_store8(0);
            local_get(16); i32_const(1); i32_add; local_set(16);
            // digits[k..m]  (continue from local 22 = k)
            block_empty; loop_empty;
              local_get(22); local_get(21); i32_ge_s; br_if(1);
              local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
              local_get(20); local_get(22); i32_add; i32_load8_u(0);
              i32_store8(0);
              local_get(16); i32_const(1); i32_add; local_set(16);
              local_get(22); i32_const(1); i32_add; local_set(22);
              br(0);
            end; end;
          end;
        end;
    });

    wasm!(f, { local_get(18); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.float_to_string, type_idx, f));
}

