// ════════════════════════════════════════════════════════════════════════
//  Matcher
// ════════════════════════════════════════════════════════════════════════

// ─── __rx_node_matches(piece_ptr, scalar) -> 0/1 ───
// Mirrors native rx_node_matches: Lit==scalar, Dot=scalar!='\n', Class=ranges.
fn compile_node_matches(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.node_matches];
    // params: 0=piece, 1=scalar
    // locals: 2=kind, 3=ranges, 4=nranges, 5=neg, 6=i, 7=hit, 8=lo, 9=hi
    let mut f = Function::new([(8, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2); // kind
        // Lit
        local_get(2); i32_const(RX_KIND_LIT); i32_eq;
        if_empty;
            local_get(1); local_get(0); i32_const(RX_PIECE_X_OFF); i32_add; i32_load(0); i32_eq;
            return_;
        end;
        // Dot
        local_get(2); i32_const(RX_KIND_DOT); i32_eq;
        if_empty;
            local_get(1); i32_const(ASCII_NEWLINE); i32_ne;
            return_;
        end;
        // Class
        local_get(2); i32_const(RX_KIND_CLASS); i32_eq;
        if_empty;
            local_get(0); i32_const(RX_PIECE_X_OFF); i32_add; i32_load(0); local_set(3);
            local_get(0); i32_const(RX_PIECE_Y_OFF); i32_add; i32_load(0); local_set(4);
            local_get(0); i32_const(RX_PIECE_Z_OFF); i32_add; i32_load(0); local_set(5);
            i32_const(0); local_set(7); // hit
            i32_const(0); local_set(6); // i
            block_empty; loop_empty;
                local_get(6); local_get(4); i32_ge_u; br_if(1);
                local_get(3); local_get(6); i32_const(RX_RANGE_WORDS * RX_WORD); i32_mul; i32_add; i32_load(RX_RANGE_LO_OFF); local_set(8);
                local_get(3); local_get(6); i32_const(RX_RANGE_WORDS * RX_WORD); i32_mul; i32_add; i32_load(RX_RANGE_HI_OFF); local_set(9);
                // lo <= scalar <= hi (signed; scalars are non-negative)
                local_get(1); local_get(8); i32_ge_s;
                local_get(1); local_get(9); i32_le_s;
                i32_and;
                if_empty; i32_const(1); local_set(7); br(2); end;
                local_get(6); i32_const(1); i32_add; local_set(6);
                br(0);
            end; end;
            // hit XOR neg
            local_get(7); local_get(5); i32_ne;
            return_;
        end;
        // anchors / group never reach node_matches
        i32_const(0);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.node_matches, type_idx, f));
}

// ─── __rx_match_one(piece_ptr, text, p, caps, ncap) -> bytes_consumed | -1 ───
// p is a BYTE offset. Returns BYTES consumed (caller advances p+ret) or -1.
// Mirrors native rx_match_one (native returns char count; we return bytes).
fn compile_match_one(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.match_one];
    // params: 0=piece, 1=text, 2=p, 3=caps, 4=ncap
    // locals: 5=kind, 6=text_len, 7=scalar, 8=width, 9=inner, 10=end, 11=ci, 12=start
    let mut f = Function::new([(8, ValType::I32)]);
    let node_matches = emitter.rt.regex.node_matches;
    let match_alts = emitter.rt.regex.match_alts;
    let utf8_scalar = emitter.rt.string.utf8_scalar;
    let utf8_width = emitter.rt.string.utf8_width;

    wasm!(f, {
        local_get(0); i32_load(0); local_set(5); // kind
        local_get(1); i32_load(0); local_set(6); // text_len bytes
    });
    // Group
    wasm!(f, {
        local_get(5); i32_const(RX_KIND_GROUP); i32_eq;
        if_empty;
            local_get(2); local_set(12); // start = p
            local_get(0); i32_const(RX_PIECE_X_OFF); i32_add; i32_load(0); local_set(9); // inner alts
            local_get(9); local_get(1); local_get(2); local_get(3); local_get(4);
            call(match_alts);
            local_set(10);
            local_get(10); i32_const(RX_NO_MATCH); i32_eq;
            if_empty; i32_const(RX_NO_MATCH); return_; end;
            // caps[ci-1] = (start,end) when ci>0
            local_get(0); i32_const(RX_PIECE_Z_OFF); i32_add; i32_load(0); local_set(11);
            local_get(11); i32_const(0); i32_gt_u;
            if_empty;
                local_get(3); local_get(11); i32_const(1); i32_sub; i32_const(RX_CAP_BYTES); i32_mul; i32_add;
                local_get(12); i32_store(RX_CAP_START_OFF);
                local_get(3); local_get(11); i32_const(1); i32_sub; i32_const(RX_CAP_BYTES); i32_mul; i32_add;
                local_get(10); i32_store(RX_CAP_END_OFF);
            end;
            local_get(10); local_get(2); i32_sub; return_; // bytes consumed
        end;
    });
    // Lit / Dot / Class: one scalar at p
    wasm!(f, {
        local_get(2); local_get(6); i32_ge_u;
        if_empty; i32_const(RX_NO_MATCH); return_; end;
        local_get(1); local_get(2); call(utf8_scalar); i32_wrap_i64; local_set(7);
        local_get(0); local_get(7); call(node_matches);
        if_empty;
            local_get(1); local_get(2); call(utf8_width); local_set(8);
            local_get(8); return_;
        else_;
            i32_const(RX_NO_MATCH); return_;
        end;
    });
    wasm!(f, { i32_const(RX_NO_MATCH); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.match_one, type_idx, f));
}

// ─── __rx_match_rep(piece, text, p, caps, ncap, count) -> end | -1 ───
// Greedy repetition over a single Piece, then its `next`. Mirrors native
// rx_match_rep with caps save/restore on the greedy branch.
fn compile_match_rep(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.match_rep];
    // params: 0=piece, 1=text, 2=p, 3=caps, 4=ncap, 5=count
    // locals: 6=max, 7=min, 8=at_max, 9=consumed, 10=res, 11=save, 12=next
    let mut f = Function::new([(7, ValType::I32)]);
    let match_one = emitter.rt.regex.match_one;
    let match_seq = emitter.rt.regex.match_seq;
    let match_rep = emitter.rt.regex.match_rep;
    let save_sp = emitter.rt.regex.save_sp_global;

    wasm!(f, {
        local_get(0); i32_const(RX_PIECE_MAX_OFF); i32_add; i32_load(0); local_set(6);
        local_get(0); i32_const(RX_PIECE_MIN_OFF); i32_add; i32_load(0); local_set(7);
        // at_max = max != UNBOUNDED && count >= max
        i32_const(0); local_set(8);
        local_get(6); i32_const(RX_MAX_UNBOUNDED); i32_ne;
        if_empty; local_get(5); local_get(6); i32_ge_u; local_set(8); end;
    });
    // greedy: if !at_max try one more
    wasm!(f, { local_get(8); i32_eqz; if_empty; });
    emit_caps_push(&mut f, save_sp, 3, 4, 11);
    wasm!(f, {
            local_get(0); local_get(1); local_get(2); local_get(3); local_get(4);
            call(match_one);
            local_set(9);
            local_get(9); i32_const(RX_NO_MATCH); i32_ne;
            if_empty;
                // zero-width guard: consumed>0 || count==0
                local_get(9); i32_const(0); i32_gt_u;
                local_get(5); i32_eqz;
                i32_or;
                if_empty;
                    local_get(0); local_get(1);
                    local_get(2); local_get(9); i32_add; // p+consumed
                    local_get(3); local_get(4);
                    local_get(5); i32_const(1); i32_add; // count+1
                    call(match_rep);
                    local_set(10);
                    local_get(10); i32_const(RX_NO_MATCH); i32_ne;
                    if_empty;
    });
    emit_caps_pop_discard(&mut f, save_sp, 11);
    wasm!(f, {
                        local_get(10); return_;
                    end;
                end;
            end;
    });
    emit_caps_restore(&mut f, save_sp, 3, 4, 11);
    wasm!(f, { end; }); // end !at_max
    // if count >= min: match rest of seq (piece.next)
    wasm!(f, {
        local_get(5); local_get(7); i32_ge_u;
        if_empty;
            local_get(0); i32_const(RX_PIECE_NEXT_OFF); i32_add; i32_load(0); local_set(12);
            local_get(12); local_get(1); local_get(2); local_get(3); local_get(4);
            call(match_seq);
            return_;
        end;
        i32_const(RX_NO_MATCH);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.match_rep, type_idx, f));
}

// ─── __rx_match_seq(piece, text, p, caps, ncap) -> end | -1 ───
// Match the linked piece list starting at `piece` (null = success). Anchors are
// handled here; everything else delegates to rep. Mirrors native rx_match_seq.
fn compile_match_seq(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.match_seq];
    // params: 0=piece, 1=text, 2=p, 3=caps, 4=ncap
    // locals: 5=kind, 6=text_len, 7=next
    let mut f = Function::new([(3, ValType::I32)]);
    let match_seq = emitter.rt.regex.match_seq;
    let match_rep = emitter.rt.regex.match_rep;

    wasm!(f, {
        // piece == null => success (return p)
        local_get(0); i32_eqz;
        if_empty; local_get(2); return_; end;
        local_get(0); i32_load(0); local_set(5); // kind
        local_get(1); i32_load(0); local_set(6); // text_len bytes
        local_get(0); i32_const(RX_PIECE_NEXT_OFF); i32_add; i32_load(0); local_set(7); // next
        // AnchorStart: p==0
        local_get(5); i32_const(RX_KIND_ANCHOR_START); i32_eq;
        if_empty;
            local_get(2); i32_eqz;
            if_empty;
                local_get(7); local_get(1); local_get(2); local_get(3); local_get(4);
                call(match_seq); return_;
            else_;
                i32_const(RX_NO_MATCH); return_;
            end;
        end;
        // AnchorEnd: p==text_len(bytes)
        local_get(5); i32_const(RX_KIND_ANCHOR_END); i32_eq;
        if_empty;
            local_get(2); local_get(6); i32_eq;
            if_empty;
                local_get(7); local_get(1); local_get(2); local_get(3); local_get(4);
                call(match_seq); return_;
            else_;
                i32_const(RX_NO_MATCH); return_;
            end;
        end;
        // else: rep over this piece, count=0
        local_get(0); local_get(1); local_get(2); local_get(3); local_get(4); i32_const(0);
        call(match_rep);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.match_seq, type_idx, f));
}

// ─── __rx_match_alts(alts, text, p, caps, ncap) -> end | -1 ───
// Try each Seq alternative; save/restore caps. Mirrors native rx_match_alts.
fn compile_match_alts(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.match_alts];
    // params: 0=alts, 1=text, 2=p, 3=caps, 4=ncap
    // locals: 5=seq, 6=res, 7=save, 8=head_piece
    let mut f = Function::new([(4, ValType::I32)]);
    let match_seq = emitter.rt.regex.match_seq;
    let save_sp = emitter.rt.regex.save_sp_global;

    wasm!(f, {
        local_get(0); i32_const(RX_ALTS_HEAD_OFF); i32_add; i32_load(0); local_set(5); // first seq
        block_empty; loop_empty;
            local_get(5); i32_eqz; br_if(1); // no more alts → fail
            local_get(5); i32_const(RX_SEQ_HEAD_OFF); i32_add; i32_load(0); local_set(8); // head piece
    });
    emit_caps_push(&mut f, save_sp, 3, 4, 7);
    wasm!(f, {
            local_get(8); local_get(1); local_get(2); local_get(3); local_get(4);
            call(match_seq);
            local_set(6);
            local_get(6); i32_const(RX_NO_MATCH); i32_ne;
            if_empty;
    });
    emit_caps_pop_discard(&mut f, save_sp, 7);
    wasm!(f, {
                local_get(6); return_;
            end;
    });
    emit_caps_restore(&mut f, save_sp, 3, 4, 7);
    wasm!(f, {
            local_get(5); i32_const(RX_SEQ_NEXT_OFF); i32_add; i32_load(0); local_set(5); // next alt
            br(0);
        end; end;
        i32_const(RX_NO_MATCH);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.match_alts, type_idx, f));
}

// ─── __rx_find_at(alts, text, start, caps, ncap) -> end | -1 ───
// Search from byte offset `start`; mirrors native rx_find_at (resets caps to
// unset on each position, sets match_start_global). `start` advances by scalar
// width so byte offsets stay codepoint-aligned. The search range is
// `start..=byte_len` (i may equal len for zero-width / anchor matches).
fn compile_find_at(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.find_at];
    // params: 0=alts, 1=text, 2=start, 3=caps, 4=ncap
    // locals: 5=text_len, 6=i, 7=res, 8=k, 9=width
    let mut f = Function::new([(5, ValType::I32)]);
    let match_alts = emitter.rt.regex.match_alts;
    let match_start = emitter.rt.regex.match_start_global;
    let utf8_width = emitter.rt.string.utf8_width;

    wasm!(f, {
        local_get(1); i32_load(0); local_set(5); // text_len bytes
        local_get(2); local_set(6); // i = start
        block_empty; loop_empty;
            local_get(6); local_get(5); i32_gt_u; br_if(1); // i > len → stop
    });
    emit_caps_reset(&mut f, 3, 4, 8);
    wasm!(f, {
            local_get(0); local_get(1); local_get(6); local_get(3); local_get(4);
            call(match_alts);
            local_set(7);
            local_get(7); i32_const(RX_NO_MATCH); i32_ne;
            if_empty;
                local_get(6); global_set(match_start);
                local_get(7); return_;
            end;
            // advance i by one scalar width; if i==len, no more positions.
            local_get(6); local_get(5); i32_ge_u;
            if_empty; i32_const(RX_NO_MATCH); return_; end;
            local_get(1); local_get(6); call(utf8_width); local_set(9);
            local_get(6); local_get(9); i32_add; local_set(6);
            br(0);
        end; end;
        i32_const(RX_NO_MATCH);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.find_at, type_idx, f));
}

// ════════════════════════════════════════════════════════════════════════
//  Capture save-stack helpers (bump push/pop of ncap × 2 words)
// ════════════════════════════════════════════════════════════════════════

/// Push a copy of `caps` (ncap slots) onto the save stack; record the saved
/// base into `save_local`. Bumps `save_sp` by ncap × RX_CAP_BYTES.
fn emit_caps_push(f: &mut Function, save_sp: u32, caps: u32, ncap: u32, save_local: u32) {
    wasm!(f, {
        global_get(save_sp); local_set(save_local);
        global_get(save_sp); local_get(ncap); i32_const(RX_CAP_BYTES); i32_mul; i32_add; global_set(save_sp);
        // memory_copy(dst=save_base, src=caps, n=ncap*RX_CAP_BYTES)
        local_get(save_local); local_get(caps); local_get(ncap); i32_const(RX_CAP_BYTES); i32_mul;
        memory_copy;
    });
}

/// Restore `caps` from the saved copy at `save_local`, then pop (rewind save_sp).
fn emit_caps_restore(f: &mut Function, save_sp: u32, caps: u32, ncap: u32, save_local: u32) {
    wasm!(f, {
        local_get(caps); local_get(save_local); local_get(ncap); i32_const(RX_CAP_BYTES); i32_mul;
        memory_copy;
        local_get(save_local); global_set(save_sp);
    });
}

/// Discard the saved copy at `save_local` (success path): rewind save_sp.
fn emit_caps_pop_discard(f: &mut Function, save_sp: u32, save_local: u32) {
    wasm!(f, { local_get(save_local); global_set(save_sp); });
}

/// Reset all caps slots to UNSET (-1). `i` is a scratch loop local.
fn emit_caps_reset(f: &mut Function, caps: u32, ncap: u32, i: u32) {
    wasm!(f, {
        i32_const(0); local_set(i);
        block_empty; loop_empty;
            local_get(i); local_get(ncap); i32_ge_u; br_if(1);
            local_get(caps); local_get(i); i32_const(RX_CAP_BYTES); i32_mul; i32_add;
            i32_const(RX_CAP_UNSET); i32_store(RX_CAP_START_OFF);
            local_get(caps); local_get(i); i32_const(RX_CAP_BYTES); i32_mul; i32_add;
            i32_const(RX_CAP_UNSET); i32_store(RX_CAP_END_OFF);
            local_get(i); i32_const(1); i32_add; local_set(i);
            br(0);
        end; end;
    });
}
