// IrExpr → WASM: binary-operator emission (split from expressions.rs).
//
// Part 2 of `expressions.rs` — `include!`d at the END of the parent, so it
// shares the parent module's imports and the `CmpKind` enum. Methods are
// moved VERBATIM; only the surrounding `impl FuncCompiler<'_>` is re-opened.

impl FuncCompiler<'_> {
    pub(super) fn emit_binop(&mut self, op: BinOp, left: &IrExpr, right: &IrExpr) {
        // BinOp is already reconciled with operand types by ConcretizeTypes pass.
        // Pick WASM arithmetic width from the operand's valtype. All
        // sized integer variants (Int8/Int16/Int32/UInt8/UInt16/UInt32)
        // lower to `i32`; `UInt64` and canonical `Int` stay `i64`. For
        // unsigned div/mod the distinction matters (div_u vs div_s),
        // tracked via `is_unsigned_int`.
        let is_i32_int = matches!(
            left.ty,
            Ty::Int8 | Ty::Int16 | Ty::Int32
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32
        );
        let is_unsigned_int = matches!(
            left.ty,
            Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
        );
        let is_f32 = matches!(left.ty, Ty::Float32);

        match op {
            // ── Arithmetic ──
            BinOp::AddInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_i32_int {
                    wasm!(self.func, { i32_add; });
                } else {
                    wasm!(self.func, { i64_add; });
                }
            }
            BinOp::SubInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_i32_int {
                    wasm!(self.func, { i32_sub; });
                } else {
                    wasm!(self.func, { i64_sub; });
                }
            }
            BinOp::MulInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_i32_int {
                    wasm!(self.func, { i32_mul; });
                } else {
                    wasm!(self.func, { i64_mul; });
                }
            }
            // Integer `/` and `%` are total: a zero divisor (or signed MIN/-1, which
            // wasm's div_s traps but rem_s silently DEFINES as 0) aborts with
            // `Error: <msg>\n` + exit 1 instead of diverging from native. See
            // `emit_checked_int_div_mod`.
            BinOp::DivInt => {
                self.emit_checked_int_div_mod(left, right, /*is_mod=*/false, is_i32_int, is_unsigned_int);
            }
            BinOp::ModInt => {
                self.emit_checked_int_div_mod(left, right, /*is_mod=*/true, is_i32_int, is_unsigned_int);
            }
            BinOp::AddFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_f32 {
                    self.func.instruction(&wasm_encoder::Instruction::F32Add);
                } else {
                    wasm!(self.func, { f64_add; });
                }
            }
            BinOp::SubFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_f32 {
                    self.func.instruction(&wasm_encoder::Instruction::F32Sub);
                } else {
                    wasm!(self.func, { f64_sub; });
                }
            }
            BinOp::MulFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_f32 {
                    self.func.instruction(&wasm_encoder::Instruction::F32Mul);
                } else {
                    wasm!(self.func, { f64_mul; });
                }
            }
            BinOp::DivFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_f32 {
                    self.func.instruction(&wasm_encoder::Instruction::F32Div);
                } else {
                    wasm!(self.func, { f64_div; });
                }
            }
            BinOp::ModFloat => {
                // WASM has no f64.rem; compute via: a - trunc(a/b) * b
                self.emit_expr(left);
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { f64_div; });
                self.func.instruction(&Instruction::F64Trunc);
                self.emit_expr(right);
                wasm!(self.func, {
                    f64_mul;
                    f64_sub;
                });
            }

            // ── Comparison (type-dispatched via operand type) ──
            BinOp::Eq => {
                // Peephole: x % (power-of-2) == 0 → (x & (n-1)) == 0
                // Safe because for any sign of x: x%n==0 ⟺ x&(n-1)==0
                let modint_zero = Self::extract_mod_pow2_eq_zero(left, right)
                    .or_else(|| Self::extract_mod_pow2_eq_zero(right, left));
                if let Some((mod_expr, mask)) = modint_zero {
                    self.emit_expr(mod_expr);
                    wasm!(self.func, { i64_const(mask); i64_and; i64_eqz; });
                } else {
                    self.emit_eq(left, right, false);
                }
            }
            BinOp::Neq => {
                // Peephole: x % (power-of-2) != 0 → (x & (n-1)) != 0
                let modint_zero = Self::extract_mod_pow2_eq_zero(left, right)
                    .or_else(|| Self::extract_mod_pow2_eq_zero(right, left));
                if let Some((mod_expr, mask)) = modint_zero {
                    self.emit_expr(mod_expr);
                    wasm!(self.func, { i64_const(mask); i64_and; i64_const(0); i64_ne; });
                } else {
                    self.emit_eq(left, right, true);
                }
            }
            BinOp::Lt => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Lt);
            }
            BinOp::Gt => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Gt);
            }
            BinOp::Lte => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Lte);
            }
            BinOp::Gte => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Gte);
            }

            // ── Logical ──
            BinOp::And => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { i32_and; });
            }
            BinOp::Or => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { i32_or; });
            }

            // ── String concatenation ──
            BinOp::ConcatStr => {
                self.emit_concat_str(left, right);
            }

            // ── List concatenation ──
            BinOp::ConcatList => {
                self.emit_expr(left);
                self.emit_expr(right);
                // Determine element size from left/right types or VarTable
                let extract_elem = |ty: &Ty| -> Option<u32> {
                    if let Ty::Applied(_, args) = ty {
                        args.first()
                            .filter(|t| !t.is_unresolved())
                            .map(|t| values::byte_size(t))
                    } else { None }
                };
                let var_elem = |expr: &IrExpr| -> Option<u32> {
                    if let almide_ir::IrExprKind::Var { id } = &expr.kind {
                        extract_elem(&self.var_table.get(*id).ty)
                    } else { None }
                };
                let elem_size = extract_elem(&left.ty)
                    .or_else(|| extract_elem(&right.ty))
                    .or_else(|| var_elem(left))
                    .or_else(|| var_elem(right))
                    .unwrap_or(8);
                wasm!(self.func, {
                    i32_const(elem_size as i32);
                    call(self.emitter.rt.concat_list);
                });
                // SHARE dup: __concat_list bulk-copies BOTH inputs' element
                // pointers into the fresh result while both inputs survive
                // and keep their own scope-end (deep) Decs — without one inc
                // per copied element, `xs = xs + [r]` frees the elements the
                // new spine shares (cross_module_spread trap). rc_inc no-ops
                // on data-section constants. Unresolved elem Ty falls through
                // un-dup'd — unreachable for live code post-AllTypesConcrete.
                let extract_elem_ty = |ty: &Ty| -> Option<Ty> {
                    if let Ty::Applied(_, args) = ty {
                        args.first().filter(|t| !t.is_unresolved()).cloned()
                    } else { None }
                };
                let var_elem_ty = |expr: &IrExpr| -> Option<Ty> {
                    if let almide_ir::IrExprKind::Var { id } = &expr.kind {
                        extract_elem_ty(&self.var_table.get(*id).ty)
                    } else { None }
                };
                let elem_ty = extract_elem_ty(&left.ty)
                    .or_else(|| extract_elem_ty(&right.ty))
                    .or_else(|| var_elem_ty(left))
                    .or_else(|| var_elem_ty(right));
                if elem_ty.map_or(false, |t| crate::pass_perceus::is_heap_type(&t)) {
                    let res = self.scratch.alloc_i32();
                    let idx = self.scratch.alloc_i32();
                    let len = self.scratch.alloc_i32();
                    let data_off = super::rt_string::list_data_off();
                    wasm!(self.func, {
                        local_set(res);
                        local_get(res); i32_load(0); local_set(len);
                        i32_const(0); local_set(idx);
                        block_empty; loop_empty;
                            local_get(idx); local_get(len); i32_ge_u; br_if(1);
                            local_get(res); i32_const(data_off); i32_add;
                            local_get(idx); i32_const(4); i32_mul; i32_add;
                            i32_load(0); call(self.emitter.rt.rc_inc); drop;
                            local_get(idx); i32_const(1); i32_add; local_set(idx);
                            br(0);
                        end; end;
                        local_get(res);
                    });
                    self.scratch.free_i32(len);
                    self.scratch.free_i32(idx);
                    self.scratch.free_i32(res);
                }
            }

            // ── Matrix operations (WASM stub — not yet optimized) ──
            BinOp::MulMatrix | BinOp::AddMatrix | BinOp::SubMatrix | BinOp::ScaleMatrix => {
                // Matrix ops in WASM: call the corresponding stdlib function via module dispatch
                let func_name = match op {
                    BinOp::MulMatrix => "mul",
                    BinOp::AddMatrix => "add",
                    BinOp::SubMatrix => "sub",
                    BinOp::ScaleMatrix => "scale",
                    _ => unreachable!(),
                };
                let target = almide_ir::CallTarget::Module {
                    module: almide_base::intern::sym("matrix"),
                    func: almide_base::intern::sym(func_name),
                    def_id: None,
                };
                self.emit_call(&target, &[left.clone(), right.clone()], &Ty::Matrix);
            }

            BinOp::PowInt => {
                // Integer power: base^exp via mem scratch (no locals needed)
                // mem[0]=base, mem[8]=result, counter on stack via block/loop
                self.emit_expr(left);
                self.emit_expr(right);
                // Use i32 scratch for counter, i64 scratch for result/base
                let base_s = self.scratch.alloc_i64();
                let result_s = self.scratch.alloc_i64();
                let counter_s = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_wrap_i64;
                    local_set(counter_s);
                    local_set(base_s);
                    i64_const(1);
                    local_set(result_s);
                    block_empty;
                    loop_empty;
                    local_get(counter_s);
                    i32_eqz;
                    br_if(1);
                    local_get(result_s);
                    local_get(base_s);
                    i64_mul;
                    local_set(result_s);
                    local_get(counter_s);
                    i32_const(1);
                    i32_sub;
                    local_set(counter_s);
                    br(0);
                    end;
                    end;
                    local_get(result_s);
                });
                self.scratch.free_i32(counter_s);
                self.scratch.free_i64(result_s);
                self.scratch.free_i64(base_s);
            }
            BinOp::PowFloat => {
                // Float `**` -> __float_pow -> vendored musl-libm __libm_pow. The old
                // inline impl (sqrt for exp==0.5, integer multiply loop otherwise) was
                // wrong for negative bases / non-integer exponents and TRAPPED on an
                // infinite exponent (i64.trunc_f64_s of inf). The vendored pow handles
                // every special case and is bit-identical to native almide_rt_math_fpow.
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { call(self.emitter.rt.float_pow); });
            }
        }
    }

    /// Emit a total integer `/` or `%`: spill both operands to scratch, guard the
    /// divisor, then run the raw div/rem. A zero divisor aborts with `division by
    /// zero`; for SIGNED div AND rem, the width's `MIN op -1` aborts with `integer
    /// overflow` (wasm `i64.rem_s`/narrow `i32.div_s` of MIN/-1 do NOT trap, so the
    /// explicit check is what keeps wasm aligned with native `checked_div`/`checked_rem`).
    /// Both abort paths call `__div_trap` with the interned `Error: <msg>\n` string.
    fn emit_checked_int_div_mod(
        &mut self,
        left: &IrExpr,
        right: &IrExpr,
        is_mod: bool,
        is_i32_int: bool,
        is_unsigned_int: bool,
    ) {
        // Divisor -1 — the second half of the signed `MIN / -1` overflow witness.
        const NEG_ONE: i64 = -1;
        let div_by_zero_msg = self.emitter.intern_string("Error: division by zero\n") as i32;
        let overflow_msg = self.emitter.intern_string("Error: integer overflow\n") as i32;
        let div_trap = self.emitter.rt.div_trap;

        // Most-negative value of the operand width — `MIN / -1` is the only signed
        // overflow. Narrow ints run as i32 arithmetic, so an i8/i16 MIN must be the
        // TRUE per-width MIN (e.g. i8 -128), not i32::MIN: `i32.div_s(-128, -1)` is
        // 128 and does NOT trap, yet native `i8::checked_div` returns None.
        let width_min: i64 = match left.ty {
            Ty::Int8 => i8::MIN as i64,
            Ty::Int16 => i16::MIN as i64,
            Ty::Int32 => i32::MIN as i64,
            _ => i64::MIN,
        };

        if is_i32_int {
            let la = self.scratch.alloc_i32();
            let rb = self.scratch.alloc_i32();
            self.emit_expr(left);
            wasm!(self.func, { local_set(la); });
            self.emit_expr(right);
            wasm!(self.func, { local_set(rb); });

            // if rb == 0 { div_trap("division by zero") }
            wasm!(self.func, {
                local_get(rb);
                i32_eqz;
                if_empty;
                i32_const(div_by_zero_msg);
                call(div_trap);
                end;
            });
            // Signed overflow: if la == width_min && rb == -1 { div_trap("integer overflow") }
            if !is_unsigned_int {
                wasm!(self.func, {
                    local_get(la);
                    i32_const(width_min as i32);
                    i32_eq;
                    local_get(rb);
                    i32_const(NEG_ONE as i32);
                    i32_eq;
                    i32_and;
                    if_empty;
                    i32_const(overflow_msg);
                    call(div_trap);
                    end;
                });
            }
            // The checked operands are now safe — run the raw op.
            wasm!(self.func, { local_get(la); local_get(rb); });
            let instr = match (is_mod, is_unsigned_int) {
                (false, true) => wasm_encoder::Instruction::I32DivU,
                (false, false) => wasm_encoder::Instruction::I32DivS,
                (true, true) => wasm_encoder::Instruction::I32RemU,
                (true, false) => wasm_encoder::Instruction::I32RemS,
            };
            self.func.instruction(&instr);
            self.scratch.free_i32(rb);
            self.scratch.free_i32(la);
        } else {
            let la = self.scratch.alloc_i64();
            let rb = self.scratch.alloc_i64();
            self.emit_expr(left);
            wasm!(self.func, { local_set(la); });
            self.emit_expr(right);
            wasm!(self.func, { local_set(rb); });

            // if rb == 0 { div_trap("division by zero") }
            wasm!(self.func, {
                local_get(rb);
                i64_eqz;
                if_empty;
                i32_const(div_by_zero_msg);
                call(div_trap);
                end;
            });
            // Signed overflow: if la == i64::MIN && rb == -1 { div_trap("integer overflow") }
            if !is_unsigned_int {
                wasm!(self.func, {
                    local_get(la);
                    i64_const(width_min);
                    i64_eq;
                    local_get(rb);
                    i64_const(NEG_ONE);
                    i64_eq;
                    i32_and;
                    if_empty;
                    i32_const(overflow_msg);
                    call(div_trap);
                    end;
                });
            }
            wasm!(self.func, { local_get(la); local_get(rb); });
            let instr = match (is_mod, is_unsigned_int) {
                (false, true) => wasm_encoder::Instruction::I64DivU,
                (false, false) => wasm_encoder::Instruction::I64DivS,
                (true, true) => wasm_encoder::Instruction::I64RemU,
                (true, false) => wasm_encoder::Instruction::I64RemS,
            };
            self.func.instruction(&instr);
            self.scratch.free_i64(rb);
            self.scratch.free_i64(la);
        }
    }
}
