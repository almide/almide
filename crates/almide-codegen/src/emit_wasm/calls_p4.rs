impl FuncCompiler<'_> {
    /// Stage 3 of the sized-numeric-types arc: emit WASM conversion
    /// instructions for the `to_intN` / `to_uintN` / `to_floatN`
    /// UFCS methods on sized numeric modules. Returns `true` if the
    /// (module, func) pair was handled.
    ///
    /// The scheme mirrors Rust's `as` semantics (wrapping on narrow
    /// int downcasts, saturating trunc on float→int). WASM-native
    /// opcodes cover every combination: `i32.wrap_i64`,
    /// `i64.extend_i32_s/_u`, `f32.convert_i64_s/_u`,
    /// `i32.trunc_sat_f64_s`, `f32.demote_f64`, `f64.promote_f32`,
    /// and no-ops for same-width / same-kind conversions.
    fn emit_sized_conv_call(&mut self, module: &str, func: &str, args: &[IrExpr]) -> bool {
        use wasm_encoder::Instruction;
        // All conversion fns take one positional arg (the source value).
        if args.len() != 1 { return false; }
        // Determine source / destination kind+width purely from names so
        // this dispatcher is closed over the entire sized-type matrix
        // regardless of which .almd module hosts the fn.
        // `int.to_int32(n: Int)` / `float.to_uint16(n: Float)` style:
        //   module names the SRC; `to_<T>` names the DST.
        // `int.from_uint16(n: UInt16)` / `float.from_float32(n: Float32)` style:
        //   module names the DST; `from_<T>` names the SRC.
        let (src, dst) = if let Some(to_part) = func.strip_prefix("to_") {
            (sized_type_info(module), sized_type_info(to_part))
        } else if let Some(from_part) = func.strip_prefix("from_") {
            (sized_type_info(from_part), sized_type_info(module))
        } else {
            return false;
        };
        let (Some((src_kind, src_bits)), Some((dst_kind, dst_bits))) = (src, dst) else {
            return false;
        };
        // to_string is NOT handled here; its @inline_rust uses format!()
        // which is Rust-only. On WASM we already have string dispatch via
        // the respective int/float module, so fall through.
        if func == "to_string" { return false; }

        self.emit_expr(&args[0]);

        let src_u = matches!(src_kind, SizedKind::UInt);
        match (src_kind, dst_kind) {
            (SizedKind::Int | SizedKind::UInt, SizedKind::Int | SizedKind::UInt) => {
                // Integer → integer. WASM valtype buckets: narrow
                // (<=32 bits) → i32; 64-bit → i64. Sign behavior at
                // extend vs wrap:
                //   src i32 bucket → dst i64 bucket: i64.extend_i32_s/_u
                //   src i64 bucket → dst i32 bucket: i32.wrap_i64
                //   same bucket: no-op for width ≥ dst; mask-to-width
                //     for narrow dst so `256 as u8 == 0` matches Rust.
                //
                // Masking is applied on narrowing INTO any sized int
                // less than 32 bits so the representation in the i32
                // bucket is canonical (zero-extended for UInt, sign-
                // extended for Int via extend_8_s / extend_16_s).
                // This keeps subsequent `i64.extend_i32_{s,u}` correct.
                let src_64 = src_bits == 64;
                let dst_64 = dst_bits == 64;
                if src_64 && !dst_64 {
                    self.func.instruction(&Instruction::I32WrapI64);
                } else if !src_64 && dst_64 {
                    if src_u {
                        self.func.instruction(&Instruction::I64ExtendI32U);
                    } else {
                        self.func.instruction(&Instruction::I64ExtendI32S);
                    }
                    // narrowing happens below when dst is <= 32 bits;
                    // extend is only for reaching the i64 bucket.
                }
                // Normalize the narrow representation: UInt* zero-pads,
                // Int* sign-extends from the stored width.
                if dst_bits < 32 {
                    let dst_u = matches!(dst_kind, SizedKind::UInt);
                    if dst_u {
                        let mask = ((1u64 << dst_bits) - 1) as i32;
                        self.func.instruction(&Instruction::I32Const(mask));
                        self.func.instruction(&Instruction::I32And);
                    } else {
                        let instr = if dst_bits == 8 { Instruction::I32Extend8S }
                                    else { Instruction::I32Extend16S };
                        self.func.instruction(&instr);
                    }
                }
            }
            (SizedKind::Int | SizedKind::UInt, SizedKind::Float) => {
                let dst_f32 = dst_bits == 32;
                let src_64 = src_bits == 64;
                let instr = match (dst_f32, src_64, src_u) {
                    (true, true, true) => Instruction::F32ConvertI64U,
                    (true, true, false) => Instruction::F32ConvertI64S,
                    (true, false, true) => Instruction::F32ConvertI32U,
                    (true, false, false) => Instruction::F32ConvertI32S,
                    (false, true, true) => Instruction::F64ConvertI64U,
                    (false, true, false) => Instruction::F64ConvertI64S,
                    (false, false, true) => Instruction::F64ConvertI32U,
                    (false, false, false) => Instruction::F64ConvertI32S,
                };
                self.func.instruction(&instr);
            }
            (SizedKind::Float, SizedKind::Int | SizedKind::UInt) => {
                // Float → int. `_sat_` variants mirror Rust's `as`
                // semantics: NaN → 0, overflow saturates to the
                // target's min/max. The signed/unsigned variant is
                // picked from the DESTINATION kind because that's what
                // determines the integer encoding.
                let src_f32 = src_bits == 32;
                let dst_64 = dst_bits == 64;
                let dst_u = matches!(dst_kind, SizedKind::UInt);
                let instr = match (dst_64, src_f32, dst_u) {
                    (true, true, true) => Instruction::I64TruncSatF32U,
                    (true, true, false) => Instruction::I64TruncSatF32S,
                    (true, false, true) => Instruction::I64TruncSatF64U,
                    (true, false, false) => Instruction::I64TruncSatF64S,
                    (false, true, true) => Instruction::I32TruncSatF32U,
                    (false, true, false) => Instruction::I32TruncSatF32S,
                    (false, false, true) => Instruction::I32TruncSatF64U,
                    (false, false, false) => Instruction::I32TruncSatF64S,
                };
                self.func.instruction(&instr);
                // Narrow into the i32 bucket: saturating-trunc into a
                // sub-32-bit target leaves the full 32-bit value on
                // the stack, but the Almide semantics say the value
                // is e.g. u16. Without masking, a downstream
                // `int.from_uint16` widens 0xFFFFFFFF (saturated inf)
                // to i64 as 4_294_967_295, not 65_535 — and
                // assert-eq compares the wrong thing.
                if dst_bits < 32 {
                    if dst_u {
                        let mask = ((1u64 << dst_bits) - 1) as i32;
                        self.func.instruction(&Instruction::I32Const(mask));
                        self.func.instruction(&Instruction::I32And);
                    } else {
                        let instr = if dst_bits == 8 { Instruction::I32Extend8S }
                                    else { Instruction::I32Extend16S };
                        self.func.instruction(&instr);
                    }
                }
            }
            (SizedKind::Float, SizedKind::Float) => {
                if src_bits == 64 && dst_bits == 32 {
                    self.func.instruction(&Instruction::F32DemoteF64);
                } else if src_bits == 32 && dst_bits == 64 {
                    self.func.instruction(&Instruction::F64PromoteF32);
                }
                // Same-width float: no-op.
            }
        }
        true
    }

    pub(super) fn emit_assert_eq(&mut self, left: &IrExpr, right: &IrExpr) {
        // Use the same equality logic as BinOp::Eq
        self.emit_eq(left, right, false);
        // If not equal (result == 0), trap
        wasm!(self.func, {
            i32_eqz;
            if_empty;
            unreachable;
            end;
        });
    }

    /// Fan module: concurrent execution fallback (sequential in WASM).
    /// fan.map(xs, f) → List[T]: apply f to each element, unwrap Results
    /// fan.race(fns) → T: run all, return first result (sequential: just run first)
    /// fan.any(fns) → Result[T, String]: first success
    /// fan.settle(fns) → List[Result[T, E]]: run all, collect results
    fn emit_fan_call(&mut self, func: &str, args: &[IrExpr], result_ty: &Ty) {
        match func {
            "map" => {
                // fan.map(xs, f) → Result[List[B], String]: apply effect fn f to each
                // element (f returns Result[B, E], an i32 ptr), collecting the unwrapped
                // ok values in LIST ORDER. If any element fails, the WHOLE map produces
                // that element's Err Result (the FIRST err in list order) — a DEFINED
                // Result, not a wasm trap. The standard Try wrapper / effect-main path
                // then propagates it, byte-identical to native's collect::<Result<_>>().
                //
                // result_ty is now Result[List[B], String]; peel one layer for the list,
                // then another for the element.
                let list_ty = match result_ty {
                    Ty::Applied(_, a) => a.first().cloned().unwrap_or(Ty::Unknown),
                    _ => result_ty.clone(),
                };
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let out_elem_ty = if let Ty::Applied(_, a) = &list_ty {
                    a.first().cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };
                let out_es = values::byte_size(&out_elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let res = self.scratch.alloc_i32();
                let err_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(err_ptr);
                    local_get(xs); i32_load(0); local_set(len);
                    i32_const(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32); local_get(len); i32_const(out_es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // Call closure(elem) — closure returns Result ptr (i32)
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                // call_indirect: (env, elem) → i32 (Result ptr)
                {
                    let mut ct = vec![ValType::I32]; // env
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_set(res);
                      // First element Err: stash it and break out of loop+block (DON'T
                      // trap/return). Depths from inside this `if`: if=0, loop=1, block=2.
                      local_get(res); i32_load(0); i32_const(0); i32_ne;
                      if_empty; local_get(res); local_set(err_ptr); br(2); end;
                      // Store unwrapped ok value into dst
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(out_es); i32_mul; i32_add;
                      local_get(res);
                });
                self.emit_load_at(&out_elem_ty, 4);
                self.emit_store_at(&out_elem_ty, 0);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Produce a Result ptr: err_ptr if an element failed, else Ok(dst).
                    local_get(err_ptr); i32_eqz;
                    if_i32;
                      // Ok(dst): [tag:0][list ptr@4]
                      i32_const(8); call(self.emitter.rt.alloc); local_set(res);
                      local_get(res); i32_const(0); i32_store(0);
                      local_get(res); local_get(dst); i32_store(4);
                      local_get(res);
                    else_;
                      local_get(err_ptr);
                    end;
                });
                self.scratch.free_i32(err_ptr);
                self.scratch.free_i32(res);
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "race" => {
                // fan.race(fns: List[() -> Result[T,E]]) → Result[T, String]: the FIRST
                // thunk in LIST ORDER to SETTLE = fns[0]'s Result (Ok or Err),
                // deterministic (NOT wall-clock). Distinct from fan.any (which SKIPS
                // failures): race leaves fns[0]'s Result as-is for the standard
                // effectful auto-unwrap. An empty list yields a DEFINED Err (no OOB read).
                let list_scratch = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let out = self.scratch.alloc_i32();
                let fail_msg = self.emitter.intern_string("fan.race: no candidates") as i32;
                let data_off = self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(list_scratch);
                    local_get(list_scratch); i32_load(0); i32_eqz; // len == 0
                    if_empty;
                      // Err("fan.race: no candidates"): [tag:1][msg@4]
                      i32_const(8); call(self.emitter.rt.alloc); local_set(out);
                      local_get(out); i32_const(1); i32_store(0);
                      local_get(out); i32_const(fail_msg); i32_store(4);
                    else_;
                      // out = fns[0]()
                      local_get(list_scratch); i32_const(data_off); i32_add; i32_load(0); local_set(closure);
                      local_get(closure); i32_load(4); // env
                      local_get(closure); i32_load(0); // table_idx
                });
                {
                    let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_set(out);
                    end;
                    local_get(out);
                });
                self.scratch.free_i32(out);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(list_scratch);
            }
            "any" => {
                // fan.any(fns) → Result[T, String]: try thunks in LIST ORDER, return
                // the FIRST Ok Result (deterministic — NOT wall-clock fastest). The
                // thunk's own `Ok(value)` Result ptr IS the fan.any result, so it is
                // left on the stack as-is. If EVERY candidate fails, produce a DEFINED
                // Err("fan.any: all candidates failed") Result — never a wasm trap. The
                // standard Try wrapper / effect-main path then surfaces it, matching
                // native's Err byte-for-byte ("Error: fan.any: all candidates failed").
                let list_scratch = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let res = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let out = self.scratch.alloc_i32();
                let fail_msg = self.emitter.intern_string("fan.any: all candidates failed") as i32;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(list_scratch);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(out);
                    block_empty; loop_empty;
                      local_get(i); local_get(list_scratch); i32_load(0); i32_ge_u; br_if(1);
                      local_get(list_scratch); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(closure);
                      local_get(closure); i32_load(4); // env
                      local_get(closure); i32_load(0); // table_idx
                });
                {
                    let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_set(res);
                      // First Ok (tag==0): this thunk's Ok Result IS the result; break.
                      local_get(res); i32_load(0); i32_eqz;
                      if_empty;
                        local_get(res); local_set(out);
                        br(2); // break out of loop + block (if=0, loop=1, block=2)
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // All failed → Err("fan.any: all candidates failed"): [tag:1][msg@4].
                    local_get(out); i32_eqz;
                    if_empty;
                      i32_const(8); call(self.emitter.rt.alloc); local_set(out);
                      local_get(out); i32_const(1); i32_store(0);
                      local_get(out); i32_const(fail_msg); i32_store(4);
                    end;
                    local_get(out);
                });
                self.scratch.free_i32(out);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(res);
                self.scratch.free_i32(i);
                self.scratch.free_i32(list_scratch);
            }
            "settle" => {
                // fan.settle(fns) → List[Result[T, E]]: run all, collect results
                // Sequential: just map and collect
                let list_scratch = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let res_val = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(list_scratch);
                    // Alloc result list
                    i32_const(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32); local_get(list_scratch); i32_load(0); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(list_scratch); i32_load(0); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(list_scratch); i32_load(0); i32_ge_u; br_if(1);
                      local_get(list_scratch); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(closure);
                      local_get(closure); i32_load(4);
                      local_get(closure); i32_load(0);
                });
                {
                    let ti = self.emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                // The closure result is an aliased (non-fresh) Result pointer. The
                // settle list must own a reference, or the size-blind LIFO free-list
                // allocator reuses/zeroes that block during later thunk calls and
                // string allocations — corrupting the settled values. rc_inc it
                // (returns the same ptr), then store `result[i] = it`. call_indirect
                // left the result on the operand stack; stash it in a scratch so the
                // store gets the correct [addr, value] order (the original stored in
                // the reverse order, writing the list address INTO the Result block).
                wasm!(self.func, {
                      call(self.emitter.rt.rc_inc);
                      local_set(res_val);
                      // result[i] address
                      local_get(result); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      // value, then store
                      local_get(res_val);
                      i32_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(res_val);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(list_scratch);
            }
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `fan.{}` — \
                 add an arm in emit_fan_call or resolve upstream",
                func
            ),
        }
    }
}
