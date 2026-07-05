/// U+FFFD REPLACEMENT CHARACTER — `from_utf8_lossy` emits one per maximal invalid
/// subpart.
const REPLACEMENT_SCALAR: i32 = '\u{FFFD}' as i32;
/// Largest ASCII scalar; a byte `<=` this is a complete 1-byte sequence.
const ASCII_MAX: i32 = 0x7F;
/// A UTF-8 continuation byte has its top two bits `0b10`: `(b & CONT_MASK) == CONT_TAG`.
const CONT_MASK: i32 = 0b1100_0000;
const CONT_TAG: i32 = 0b1000_0000;
/// Any valid continuation byte (`0b10_111111`), used to probe `from_utf8` below.
const CONT_SAMPLE: u8 = 0b1011_1111;

/// Pack a `__utf8_classify` result: `consumed` bytes, `valid` flag (1 = well-formed
/// sequence to copy; 0 = maximal invalid subpart → emit one U+FFFD, resume after).
const fn classify_packed(consumed: i32, valid: i32) -> i32 {
    (consumed << 1) | valid
}

/// Derive a non-ASCII lead byte's UTF-8 classification from Rust's OWN validator
/// (no hardcoded Table 3-7 constants): returns `(width, lo2, hi2)` — the sequence
/// length and the valid 2nd-byte range — or `(0, 0, 0)` if `b0` can't start a valid
/// sequence. Probing `std::str::from_utf8` keeps this locked to std's exact UTF-8
/// rules, the same the native `from_utf8_lossy` runtime uses.
fn utf8_lead_class(b0: u8) -> (u8, u8, u8) {
    for width in 2u8..=4 {
        let (mut lo, mut hi) = (None, None);
        for b1 in 0u8..=u8::MAX {
            let mut seq = vec![b0, b1];
            seq.resize(width as usize, CONT_SAMPLE); // valid trailing continuations
            if std::str::from_utf8(&seq).is_ok_and(|s| s.chars().count() == 1) {
                lo.get_or_insert(b1);
                hi = Some(b1);
            }
        }
        if let (Some(lo), Some(hi)) = (lo, hi) {
            return (width, lo, hi);
        }
    }
    (0, 0, 0)
}

/// Lead bytes grouped into contiguous runs of identical `(width, lo2, hi2)` —
/// `(lead_lo, lead_hi, width, lo2, hi2)`. Built from [`utf8_lead_class`], so the
/// boundaries are oracle-derived, never hand-written hex. Cached: the derivation
/// probes `from_utf8` ~98k times, so compute it once per process rather than per
/// module compile.
fn utf8_lead_groups() -> &'static [(u8, u8, u8, u8, u8)] {
    static GROUPS: LazyLock<Vec<(u8, u8, u8, u8, u8)>> = LazyLock::new(|| {
        let mut groups: Vec<(u8, u8, u8, u8, u8)> = Vec::new();
        for b0 in (ASCII_MAX as u8 + 1)..=u8::MAX {
            let (w, lo2, hi2) = utf8_lead_class(b0);
            if w == 0 {
                continue; // invalid lead → handled by the width==0 default
            }
            match groups.last_mut() {
                Some(g) if g.1 + 1 == b0 && (g.2, g.3, g.4) == (w, lo2, hi2) => g.1 = b0,
                _ => groups.push((b0, b0, w, lo2, hi2)),
            }
        }
        groups
    });
    &GROUPS
}

/// `__utf8_classify(buf, i, n) -> i32`: classify the UTF-8 sequence starting at
/// `buf[i]` (within `n` bytes), returning `classify_packed(consumed, valid)`.
/// Replicates Rust `String::from_utf8_lossy`'s maximal-subpart subdivision.
fn compile_utf8_classify(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.utf8_classify];
    let groups = utf8_lead_groups();
    const BUF: u32 = 0; // params
    const I: u32 = 1;
    const N: u32 = 2;
    const B0: u32 = 3; // locals
    const WIDTH: u32 = 4; // 0 ⇒ invalid lead
    const LO2: u32 = 5;
    const HI2: u32 = 6;
    const CONSUMED: u32 = 7;
    const K: u32 = 8;
    const BK: u32 = 9;
    let mut f = Function::new([(7, ValType::I32)]);
    wasm!(f, {
        local_get(BUF); local_get(I); i32_add; i32_load8_u(0); local_set(B0);
        local_get(B0); i32_const(ASCII_MAX); i32_le_u;
        if_empty; i32_const(classify_packed(1, 1)); return_; end;   // ASCII: 1 byte, valid
        i32_const(0); local_set(WIDTH);
    });
    // Lead-byte width + 2nd-byte range, generated from the derived groups.
    for (lead_lo, lead_hi, width, lo2, hi2) in groups {
        wasm!(f, {
            local_get(B0); i32_const(*lead_lo as i32); i32_ge_u;
            local_get(B0); i32_const(*lead_hi as i32); i32_le_u; i32_and;
            if_empty;
              i32_const(*width as i32); local_set(WIDTH);
              i32_const(*lo2 as i32); local_set(LO2);
              i32_const(*hi2 as i32); local_set(HI2);
            end;
        });
    }
    wasm!(f, {
        local_get(WIDTH); i32_eqz;
        if_empty; i32_const(classify_packed(1, 0)); return_; end;   // invalid lead: 1-byte subpart
        // Validate continuation bytes: 2nd in [lo2,hi2]; 3rd/4th are plain continuations.
        // On the first failure the maximal subpart ends, so `consumed < width`.
        i32_const(1); local_set(CONSUMED);
        i32_const(1); local_set(K);
        block_empty; loop_empty;
          local_get(K); local_get(WIDTH); i32_ge_u; br_if(1);             // matched all → valid
          local_get(I); local_get(K); i32_add; local_get(N); i32_ge_u; br_if(1);   // truncated
          local_get(BUF); local_get(I); i32_add; local_get(K); i32_add; i32_load8_u(0); local_set(BK);
          // valid = (k == 1) ? lo2 <= bk <= hi2 : bk is a continuation byte
          local_get(K); i32_const(1); i32_eq;
          if_i32;
            local_get(BK); local_get(LO2); i32_ge_u; local_get(BK); local_get(HI2); i32_le_u; i32_and;
          else_;
            local_get(BK); i32_const(CONT_MASK); i32_and; i32_const(CONT_TAG); i32_eq;
          end;
          i32_eqz; br_if(1);
          local_get(CONSUMED); i32_const(1); i32_add; local_set(CONSUMED);
          local_get(K); i32_const(1); i32_add; local_set(K);
          br(0);
        end; end;
        // classify_packed(consumed, consumed == width)
        local_get(CONSUMED); i32_const(1); i32_shl;
        local_get(CONSUMED); local_get(WIDTH); i32_eq;
        i32_or;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.utf8_classify, type_idx, f));
}

/// `from_bytes(list) -> String`: UTF-8-lossy decode of the byte list (each element
/// truncated to a byte), the inverse of `to_bytes`. Two passes over a scratch byte
/// buffer: classify each sequence, copy well-formed bytes through, emit one U+FFFD
/// per maximal invalid subpart — byte-identical to native `String::from_utf8_lossy`.
fn compile_from_bytes(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.from_bytes];
    let classify = emitter.rt.string.utf8_classify;
    let emit_scalar = emitter.rt.string.utf8_emit_scalar;
    let alloc = emitter.rt.alloc;
    let do_ = string_data_off();
    const LIST: u32 = 0; // param
    const N: u32 = 1; // byte count
    const BUF: u32 = 2; // scratch byte buffer
    const I: u32 = 3; // cursor
    const TOTAL: u32 = 4; // pass-1 output byte length
    const R: u32 = 5; // packed classify result
    const CONSUMED: u32 = 6;
    const OUT: u32 = 7; // output string ptr
    const WOFF: u32 = 8; // pass-2 write offset
    // ASCII-out and FFFD-out byte counts for pass-1 sizing.
    let fffd_len = '\u{FFFD}'.len_utf8() as i32;
    let mut f = Function::new([(8, ValType::I32)]);
    wasm!(f, {
        // n = list length; copy elements (truncated to bytes) into a scratch buffer.
        local_get(LIST); i32_load(0); local_set(N);
        local_get(N); call(alloc); local_set(BUF);
        i32_const(0); local_set(I);
        block_empty; loop_empty;
          local_get(I); local_get(N); i32_ge_u; br_if(1);
          local_get(BUF); local_get(I); i32_add;
          local_get(LIST); i32_const(list_data_off()); i32_add; local_get(I); i32_const(8); i32_mul; i32_add;
          i64_load(0); i32_wrap_i64; i32_store8(0);
          local_get(I); i32_const(1); i32_add; local_set(I);
          br(0);
        end; end;
        // PASS 1: output byte length (valid run = consumed bytes; invalid = one U+FFFD).
        i32_const(0); local_set(TOTAL);
        i32_const(0); local_set(I);
        block_empty; loop_empty;
          local_get(I); local_get(N); i32_ge_u; br_if(1);
          local_get(BUF); local_get(I); local_get(N); call(classify); local_set(R);
          local_get(R); i32_const(1); i32_shr_u; local_set(CONSUMED);   // R >> 1
          local_get(R); i32_const(1); i32_and;                          // valid bit
          if_i32; local_get(CONSUMED); else_; i32_const(fffd_len); end;
          local_get(TOTAL); i32_add; local_set(TOTAL);
          local_get(I); local_get(CONSUMED); i32_add; local_set(I);
          br(0);
        end; end;
        // alloc the output string.
        i32_const(string_hdr()); local_get(TOTAL); i32_add; call(alloc); local_set(OUT);
        local_get(OUT); local_get(TOTAL); i32_store(0);
        // PASS 2: fill (copy valid bytes, emit U+FFFD for each invalid subpart).
        i32_const(0); local_set(WOFF);
        i32_const(0); local_set(I);
        block_empty; loop_empty;
          local_get(I); local_get(N); i32_ge_u; br_if(1);
          local_get(BUF); local_get(I); local_get(N); call(classify); local_set(R);
          local_get(R); i32_const(1); i32_shr_u; local_set(CONSUMED);
          local_get(R); i32_const(1); i32_and;
          if_empty;
            local_get(OUT); i32_const(do_); i32_add; local_get(WOFF); i32_add;
            local_get(BUF); local_get(I); i32_add;
            local_get(CONSUMED);
            memory_copy;
            local_get(WOFF); local_get(CONSUMED); i32_add; local_set(WOFF);
          else_;
            local_get(OUT); local_get(WOFF); i32_const(REPLACEMENT_SCALAR); call(emit_scalar); local_set(WOFF);
          end;
          local_get(I); local_get(CONSUMED); i32_add; local_set(I);
          br(0);
        end; end;
        local_get(OUT); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.from_bytes, type_idx, f));
}

fn compile_to_bytes(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.to_bytes];
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        i32_const(list_hdr()); local_get(1); i32_const(8); i32_mul; i32_add;
        call(emitter.rt.alloc); local_set(2);
        local_get(2); local_get(1); i32_store(0);
        i32_const(0); local_set(3);
        block_empty; loop_empty;
          local_get(3); local_get(1); i32_ge_u; br_if(1);
          local_get(2); i32_const(list_data_off()); i32_add; local_get(3); i32_const(8); i32_mul; i32_add;
          local_get(0); i32_const(string_data_off()); i32_add; local_get(3); i32_add;
          i32_load8_u(0); i64_extend_i32_u; i64_store(0);
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
        local_get(2); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.to_bytes, type_idx, f));
}

// ── Full-Unicode case folding ──
//
// `to_upper`/`to_lower`/`capitalize` are byte-identical to native (Rust
// `str::to_uppercase`/`to_lowercase` + char `to_uppercase`). The mapping tables
// are generated at emit time in `rt_string_case` from the SAME `std`, embedded at
// the front of the data section, and consulted here. Uppercasing is context-free;
// lowercasing is too EXCEPT Greek capital sigma U+03A3 (Final_Sigma), resolved by
// `__final_sigma`. See `rt_string_case` for the derivation + proofs.

/// `__utf8_emit_scalar(dst, byte_off, scalar) -> new_byte_off`. Encodes `scalar`
/// (a valid Unicode scalar, max U+10FFFF) as 1-4 UTF-8 bytes into `dst`'s data
/// section at `byte_off`; returns the advanced byte offset.
fn compile_utf8_emit_scalar(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.utf8_emit_scalar];
    // params: 0=dst, 1=byte_off, 2=scalar | local: 3=addr
    let mut f = Function::new([(1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_const(string_data_off()); i32_add; local_get(1); i32_add; local_set(3);
        local_get(2); i32_const(0x80); i32_lt_u;
        if_i32;
          local_get(3); local_get(2); i32_store8(0);
          local_get(1); i32_const(1); i32_add;
        else_;
          local_get(2); i32_const(0x800); i32_lt_u;
          if_i32;
            local_get(3); local_get(2); i32_const(6); i32_shr_u; i32_const(0xC0); i32_or; i32_store8(0);
            local_get(3); local_get(2); i32_const(0x3F); i32_and; i32_const(0x80); i32_or; i32_store8(1);
            local_get(1); i32_const(2); i32_add;
          else_;
            local_get(2); i32_const(0x10000); i32_lt_u;
            if_i32;
              local_get(3); local_get(2); i32_const(12); i32_shr_u; i32_const(0xE0); i32_or; i32_store8(0);
              local_get(3); local_get(2); i32_const(6); i32_shr_u; i32_const(0x3F); i32_and; i32_const(0x80); i32_or; i32_store8(1);
              local_get(3); local_get(2); i32_const(0x3F); i32_and; i32_const(0x80); i32_or; i32_store8(2);
              local_get(1); i32_const(3); i32_add;
            else_;
              local_get(3); local_get(2); i32_const(18); i32_shr_u; i32_const(0xF0); i32_or; i32_store8(0);
              local_get(3); local_get(2); i32_const(12); i32_shr_u; i32_const(0x3F); i32_and; i32_const(0x80); i32_or; i32_store8(1);
              local_get(3); local_get(2); i32_const(6); i32_shr_u; i32_const(0x3F); i32_and; i32_const(0x80); i32_or; i32_store8(2);
              local_get(3); local_get(2); i32_const(0x3F); i32_and; i32_const(0x80); i32_or; i32_store8(3);
              local_get(1); i32_const(4); i32_add;
            end;
          end;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.utf8_emit_scalar, type_idx, f));
}

/// `__case_map_lookup(map_sel, scalar) -> i32`. Binary-search the UPPER(0)/LOWER(1)
/// map; returns the absolute address of the `[len:u8][utf8 bytes]` value record,
/// or -1 on miss (caller emits the scalar unchanged). Trivial when no case op is
/// present (then DCE-stubbed anyway).
fn compile_case_map_lookup(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.case_map_lookup];
    // params: 0=map_sel, 1=scalar | locals: 2=keys, 3=n, 4=offs, 5=lo, 6=hi, 7=mid, 8=k
    let mut f = Function::new([(7, ValType::I32)]);
    if let Some(ct) = emitter.case_tables {
        wasm!(f, {
            local_get(0); i32_eqz;
            if_empty;
              i32_const(ct.upper_keys as i32); local_set(2);
              i32_const(ct.upper_n as i32); local_set(3);
              i32_const(ct.upper_offs as i32); local_set(4);
            else_;
              i32_const(ct.lower_keys as i32); local_set(2);
              i32_const(ct.lower_n as i32); local_set(3);
              i32_const(ct.lower_offs as i32); local_set(4);
            end;
            i32_const(0); local_set(5);
            local_get(3); local_set(6);
            block_empty; loop_empty;
              local_get(5); local_get(6); i32_ge_u;
              if_empty; i32_const(-1); return_; end;
              local_get(5); local_get(6); i32_add; i32_const(1); i32_shr_u; local_set(7);
              local_get(2); local_get(7); i32_const(2); i32_shl; i32_add; i32_load(0); local_set(8);
              local_get(8); local_get(1); i32_eq;
              if_empty;
                local_get(4); local_get(7); i32_const(2); i32_shl; i32_add; i32_load(0); return_;
              end;
              local_get(8); local_get(1); i32_lt_u;
              if_empty;
                local_get(7); i32_const(1); i32_add; local_set(5);
              else_;
                local_get(7); local_set(6);
              end;
              br(0);
            end; end;
            i32_const(-1);
            end;
        });
    } else {
        wasm!(f, { i32_const(-1); end; });
    }
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.case_map_lookup, type_idx, f));
}

/// `__set_member(set_sel, scalar) -> i32`. 1 iff `scalar` is in the CASED(0) /
/// CASE_IGNORABLE(1) sorted key array (binary search). Used by `__final_sigma`.
fn compile_set_member(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.set_member];
    // params: 0=set_sel, 1=scalar | locals: 2=base, 3=n, 4=lo, 5=hi, 6=mid, 7=k
    let mut f = Function::new([(6, ValType::I32)]);
    if let Some(ct) = emitter.case_tables {
        wasm!(f, {
            local_get(0); i32_eqz;
            if_empty;
              i32_const(ct.cased as i32); local_set(2);
              i32_const(ct.cased_n as i32); local_set(3);
            else_;
              i32_const(ct.ci as i32); local_set(2);
              i32_const(ct.ci_n as i32); local_set(3);
            end;
            i32_const(0); local_set(4);
            local_get(3); local_set(5);
            block_empty; loop_empty;
              local_get(4); local_get(5); i32_ge_u;
              if_empty; i32_const(0); return_; end;
              local_get(4); local_get(5); i32_add; i32_const(1); i32_shr_u; local_set(6);
              local_get(2); local_get(6); i32_const(2); i32_shl; i32_add; i32_load(0); local_set(7);
              local_get(7); local_get(1); i32_eq;
              if_empty; i32_const(1); return_; end;
              local_get(7); local_get(1); i32_lt_u;
              if_empty;
                local_get(6); i32_const(1); i32_add; local_set(4);
              else_;
                local_get(6); local_set(5);
              end;
              br(0);
            end; end;
            i32_const(0);
            end;
        });
    } else {
        wasm!(f, { i32_const(0); end; });
    }
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.set_member, type_idx, f));
}

/// `__final_sigma(s, byte_off) -> i32`. The Unicode `Final_Sigma` rule for a Σ at
/// `byte_off`: ς (U+03C2) iff it is preceded by a Cased char (skipping
/// Case_Ignorable) AND not followed by one; else σ (U+03C3).
///
/// Both context scans cost O(length of the adjacent Case_Ignorable run), NOT
/// O(position): the "Before" scan steps BACKWARD over codepoints (skipping UTF-8
/// continuation bytes) rather than re-walking from byte 0, so a Σ-dense string
/// stays O(n) overall (a forward re-walk would be O(n²)). This mirrors Rust's
/// reverse-iterator `Final_Sigma` scan in `str::to_lowercase`.
fn compile_final_sigma(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.final_sigma];
    // params: 0=s, 1=byte_off | locals: 2=blen, 3=before, 4=after, 5=p, 6=q, 7=sc, 8=done
    let mut f = Function::new([(7, ValType::I32)]);
    let uw = emitter.rt.string.utf8_width;
    let us = emitter.rt.string.utf8_scalar;
    let setm = emitter.rt.string.set_member;
    let do_ = string_data_off();
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        i32_const(0); local_set(3);   // before
        i32_const(0); local_set(4);   // after
        // Before: step BACKWARD from byte_off over codepoints, skipping
        // Case_Ignorable; the first non-ignorable char's Cased-ness is `before`.
        local_get(1); local_set(5);   // p = byte_off
        i32_const(0); local_set(8);   // done
        block_empty; loop_empty;
          local_get(8); br_if(1);            // done → break
          local_get(5); i32_eqz; br_if(1);   // p == 0 → break (before stays 0)
          // q = p-1; skip UTF-8 continuation bytes (0b10xxxxxx) back to a lead byte.
          local_get(5); i32_const(1); i32_sub; local_set(6);
          block_empty; loop_empty;
            local_get(6); i32_eqz; br_if(1);                   // q == 0 → stop
            local_get(0); i32_const(do_); i32_add; local_get(6); i32_add; i32_load8_u(0);
            i32_const(0xC0); i32_and; i32_const(0x80); i32_eq; // continuation byte?
            i32_eqz; br_if(1);                                 // not continuation → stop (lead byte)
            local_get(6); i32_const(1); i32_sub; local_set(6);
            br(0);
          end; end;
          local_get(0); local_get(6); call(us); i32_wrap_i64; local_set(7);
          i32_const(1); local_get(7); call(setm); i32_eqz;     // not Case_Ignorable
          if_empty;
            i32_const(0); local_get(7); call(setm); local_set(3);  // before = Cased(sc)
            i32_const(1); local_set(8);
          else_;
            local_get(6); local_set(5);                            // p = q (keep scanning back)
          end;
          br(0);
        end; end;
        // After: first non-CI scalar at/after byte_off + width(Σ).
        local_get(1); local_get(0); local_get(1); call(uw); i32_add; local_set(5);
        i32_const(0); local_set(8);
        block_empty; loop_empty;
          local_get(5); local_get(2); i32_ge_u; br_if(1);
          local_get(8); br_if(1);
          local_get(0); local_get(5); call(uw); local_set(6);
          local_get(0); local_get(5); call(us); i32_wrap_i64; local_set(7);
          i32_const(1); local_get(7); call(setm); i32_eqz;
          if_empty;
            i32_const(0); local_get(7); call(setm); local_set(4);
            i32_const(1); local_set(8);
          end;
          local_get(5); local_get(6); i32_add; local_set(5);
          br(0);
        end; end;
        local_get(3); local_get(4); i32_eqz; i32_and;
        if_i32; i32_const(0x03C2); else_; i32_const(0x03C3); end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.final_sigma, type_idx, f));
}

/// `__str_case_map(s, is_upper) -> i32`. The unified two-pass case driver, exact
/// for all scalars. Pass 1 sizes the output (ASCII = 1 byte; Σ-lower = 2 bytes;
/// else table out_len or identity width); ONE allocation; pass 2 fills (ASCII
/// fold inline, Σ via Final_Sigma, else `memory.copy` of the table/identity bytes).
fn compile_str_case_map(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.str_case_map];
    // params: 0=s, 1=is_upper
    // locals: 2=blen,3=total,4=i,5=b0,6=w,7=sc,8=rec,9=out,10=woff,11=outlen,12=fold,13=msel
    let mut f = Function::new([(12, ValType::I32)]);
    let uw = emitter.rt.string.utf8_width;
    let us = emitter.rt.string.utf8_scalar;
    let lk = emitter.rt.string.case_map_lookup;
    let fsig = emitter.rt.string.final_sigma;
    let em = emitter.rt.string.utf8_emit_scalar;
    let alloc = emitter.rt.alloc;
    let do_ = string_data_off();
    let hdr = string_hdr();
    let capo = string_cap_off() as u32;
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(1); i32_eqz; local_set(13);   // msel = is_upper==0 ? 1 : 0
        // PASS 1: total output bytes
        i32_const(0); local_set(3);
        i32_const(0); local_set(4);
        block_empty; loop_empty;
          local_get(4); local_get(2); i32_ge_u; br_if(1);
          local_get(0); i32_const(do_); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(5);
          local_get(5); i32_const(0x80); i32_lt_u;
          if_empty;
            local_get(3); i32_const(1); i32_add; local_set(3);
            local_get(4); i32_const(1); i32_add; local_set(4);
          else_;
            local_get(0); local_get(4); call(uw); local_set(6);
            local_get(0); local_get(4); call(us); i32_wrap_i64; local_set(7);
            local_get(1); i32_eqz; local_get(7); i32_const(0x03A3); i32_eq; i32_and;
            if_empty;
              local_get(3); i32_const(2); i32_add; local_set(3);
            else_;
              local_get(13); local_get(7); call(lk); local_set(8);
              local_get(8); i32_const(-1); i32_eq;
              if_empty;
                local_get(3); local_get(6); i32_add; local_set(3);
              else_;
                local_get(3); local_get(8); i32_load8_u(0); i32_add; local_set(3);
              end;
            end;
            local_get(4); local_get(6); i32_add; local_set(4);
          end;
          br(0);
        end; end;
        // ALLOC exact-size output
        i32_const(hdr); local_get(3); i32_add; call(alloc); local_set(9);
        local_get(9); local_get(3); i32_store(0);
        local_get(9); local_get(3); i32_store(capo, 0);
        // PASS 2: fill
        i32_const(0); local_set(10);
        i32_const(0); local_set(4);
        block_empty; loop_empty;
          local_get(4); local_get(2); i32_ge_u; br_if(1);
          local_get(0); i32_const(do_); i32_add; local_get(4); i32_add; i32_load8_u(0); local_set(5);
          local_get(5); i32_const(0x80); i32_lt_u;
          if_empty;
            local_get(1);
            if_i32;
              local_get(5); i32_const(0x61); i32_ge_u; local_get(5); i32_const(0x7A); i32_le_u; i32_and;
              if_i32; local_get(5); i32_const(32); i32_sub; else_; local_get(5); end;
            else_;
              local_get(5); i32_const(0x41); i32_ge_u; local_get(5); i32_const(0x5A); i32_le_u; i32_and;
              if_i32; local_get(5); i32_const(32); i32_add; else_; local_get(5); end;
            end;
            local_set(12);
            local_get(9); i32_const(do_); i32_add; local_get(10); i32_add; local_get(12); i32_store8(0);
            local_get(10); i32_const(1); i32_add; local_set(10);
            local_get(4); i32_const(1); i32_add; local_set(4);
          else_;
            local_get(0); local_get(4); call(uw); local_set(6);
            local_get(0); local_get(4); call(us); i32_wrap_i64; local_set(7);
            local_get(1); i32_eqz; local_get(7); i32_const(0x03A3); i32_eq; i32_and;
            if_empty;
              local_get(9); local_get(10);
              local_get(0); local_get(4); call(fsig);
              call(em); local_set(10);
            else_;
              local_get(13); local_get(7); call(lk); local_set(8);
              local_get(8); i32_const(-1); i32_eq;
              if_empty;
                local_get(9); i32_const(do_); i32_add; local_get(10); i32_add;
                local_get(0); i32_const(do_); i32_add; local_get(4); i32_add;
                local_get(6);
                memory_copy;
                local_get(10); local_get(6); i32_add; local_set(10);
              else_;
                local_get(8); i32_load8_u(0); local_set(11);
                local_get(9); i32_const(do_); i32_add; local_get(10); i32_add;
                local_get(8); i32_const(1); i32_add;
                local_get(11);
                memory_copy;
                local_get(10); local_get(11); i32_add; local_set(10);
              end;
            end;
            local_get(4); local_get(6); i32_add; local_set(4);
          end;
          br(0);
        end; end;
        local_get(9); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.str_case_map, type_idx, f));
}

/// `__str_capitalize(s) -> i32`. First scalar uppercased (`char::to_uppercase` —
/// context-free, no Σ rule), the rest of the bytes copied VERBATIM (native
/// `string.capitalize` does not recase the tail).
fn compile_str_capitalize(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.string.capitalize];
    // params: 0=s (1 param ⇒ declared locals start at index 1; index 1 is unused).
    // locals: 2=blen,3=w0,4=sc0,5=b0,6=rec,7=hlen,8=total,9=out,10=hb
    let mut f = Function::new([(10, ValType::I32)]);
    let uw = emitter.rt.string.utf8_width;
    let us = emitter.rt.string.utf8_scalar;
    let lk = emitter.rt.string.case_map_lookup;
    let alloc = emitter.rt.alloc;
    let do_ = string_data_off();
    let hdr = string_hdr();
    let capo = string_cap_off() as u32;
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        // SHARE: empty input → returns the INPUT string; own a +1 (#668 class).
        local_get(2); i32_eqz; if_empty; local_get(0); call(emitter.rt.rc_inc); return_; end;
        local_get(0); i32_const(do_); i32_add; i32_load8_u(0); local_set(5);
        local_get(0); i32_const(0); call(uw); local_set(3);
        local_get(5); i32_const(0x80); i32_lt_u;
        if_empty;
          i32_const(1); local_set(7);
          local_get(5); i32_const(0x61); i32_ge_u; local_get(5); i32_const(0x7A); i32_le_u; i32_and;
          if_i32; local_get(5); i32_const(32); i32_sub; else_; local_get(5); end;
          local_set(10);
          i32_const(-2); local_set(6);
        else_;
          local_get(0); i32_const(0); call(us); i32_wrap_i64; local_set(4);
          i32_const(0); local_get(4); call(lk); local_set(6);
          local_get(6); i32_const(-1); i32_eq;
          if_empty; local_get(3); local_set(7);
          else_; local_get(6); i32_load8_u(0); local_set(7); end;
        end;
        local_get(7); local_get(2); i32_add; local_get(3); i32_sub; local_set(8);
        i32_const(hdr); local_get(8); i32_add; call(alloc); local_set(9);
        local_get(9); local_get(8); i32_store(0);
        local_get(9); local_get(8); i32_store(capo, 0);
        // head
        local_get(6); i32_const(-2); i32_eq;
        if_empty;
          local_get(9); i32_const(do_); i32_add; local_get(10); i32_store8(0);
        else_;
          local_get(6); i32_const(-1); i32_eq;
          if_empty;
            local_get(9); i32_const(do_); i32_add;
            local_get(0); i32_const(do_); i32_add;
            local_get(3);
            memory_copy;
          else_;
            local_get(9); i32_const(do_); i32_add;
            local_get(6); i32_const(1); i32_add;
            local_get(7);
            memory_copy;
          end;
        end;
        // tail (verbatim): blen - w0 bytes from s data+w0 to out data+hlen
        local_get(9); i32_const(do_); i32_add; local_get(7); i32_add;
        local_get(0); i32_const(do_); i32_add; local_get(3); i32_add;
        local_get(2); local_get(3); i32_sub;
        memory_copy;
        local_get(9); end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.string.capitalize, type_idx, f));
}

