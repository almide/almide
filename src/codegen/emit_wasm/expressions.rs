//! IrExpr → WASM instruction emission.

use crate::ir::{BinOp, IrExpr, IrExprKind, UnOp};
use crate::types::Ty;
use wasm_encoder::{Instruction, MemArg, ValType};

use super::FuncCompiler;
use super::values;
use super::wasm_macro::wasm;

pub(super) fn mem(offset: u64) -> MemArg {
    MemArg { offset, align: 2, memory_index: 0 }
}

#[derive(Clone, Copy)]
pub(super) enum CmpKind {
    Lt,
    Gt,
    Lte,
    Gte,
}

impl FuncCompiler<'_> {
    /// Emit WASM instructions for an IR expression.
    /// Leaves the result value on the WASM stack (nothing for Unit).
    pub fn emit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            // ── Literals ──
            IrExprKind::LitInt { value } => {
                wasm!(self.func, { i64_const(*value); });
            }
            IrExprKind::LitFloat { value } => {
                wasm!(self.func, { f64_const(*value); });
            }
            IrExprKind::LitBool { value } => {
                wasm!(self.func, { i32_const(*value as i32); });
            }
            IrExprKind::LitStr { value } => {
                let offset = self.emitter.intern_string(value);
                wasm!(self.func, { i32_const(offset as i32); });
            }
            IrExprKind::Unit => {
                // Unit produces no value on the stack
            }

            // ── Variables ──
            IrExprKind::Var { id } => {
                if let Some(&local_idx) = self.var_map.get(&id.0) {
                    if self.emitter.mutable_captures.contains(&id.0) {
                        // Mutable capture: local holds cell ptr, deref to get value
                        wasm!(self.func, { local_get(local_idx); });
                        self.emit_load_at(&expr.ty, 0);
                    } else {
                        wasm!(self.func, { local_get(local_idx); });
                    }
                } else if let Some(&(global_idx, _)) = self.emitter.top_let_globals.get(&id.0) {
                    wasm!(self.func, { global_get(global_idx); });
                } else {
                    // VarId not in var_map — try name-based lookup as fallback
                    // (handles VarId mismatch between lowering passes)
                    let name = if (id.0 as usize) < self.var_table.len() { &self.var_table.get(*id).name } else { "" };
                    let found = if !name.is_empty() {
                        let target_vt = values::ty_to_valtype(&expr.ty);
                        // Find var_map entry with matching name, prefer matching WASM type
                        self.var_map.iter()
                            .filter(|(vid, _)| (**vid as usize) < self.var_table.len() && self.var_table.get(crate::ir::VarId(**vid)).name == name)
                            .max_by_key(|(vid, _)| {
                                let vid_vt = values::ty_to_valtype(&self.var_table.get(crate::ir::VarId(**vid)).ty);
                                if vid_vt == target_vt { 1u8 } else { 0u8 }
                            })
                            .map(|(_, lidx)| *lidx)
                    } else { None };
                    if let Some(local_idx) = found {
                        wasm!(self.func, { local_get(local_idx); });
                    } else {
                        // Truly not in scope — push typed zero
                        match values::ty_to_valtype(&expr.ty) {
                            Some(ValType::I64) => { wasm!(self.func, { i64_const(0); }); }
                            Some(ValType::F64) => { wasm!(self.func, { f64_const(0.0); }); }
                            Some(ValType::I32) => { wasm!(self.func, { i32_const(0); }); }
                            _ => {}
                        }
                    }
                }
            }

            // ── Function reference (used as value) → closure [wrapper_table_idx, 0] ──
            IrExprKind::FnRef { name } => {
                self.emit_fn_ref_closure(name);
            }

            // ── Lambda → closure [table_idx, env_ptr] ──
            IrExprKind::Lambda { params, body } => {
                self.emit_lambda_closure(params, body);
            }

            // ── Binary operators ──
            IrExprKind::BinOp { op, left, right } => {
                self.emit_binop(*op, left, right);
            }

            // ── Unary operators ──
            IrExprKind::UnOp { op, operand } => {
                match op {
                    UnOp::NegInt => {
                        wasm!(self.func, { i64_const(0); });
                        self.emit_expr(operand);
                        wasm!(self.func, { i64_sub; });
                    }
                    UnOp::NegFloat => {
                        self.emit_expr(operand);
                        wasm!(self.func, { f64_neg; });
                    }
                    UnOp::Not => {
                        self.emit_expr(operand);
                        wasm!(self.func, { i32_eqz; });
                    }
                }
            }

            // ── If/else ──
            IrExprKind::If { cond, then, else_ } => {
                self.emit_expr(cond);
                let bt = values::block_type(&expr.ty);
                self.func.instruction(&Instruction::If(bt));
                let _g0 = self.depth_push();
                self.emit_expr(then);
                wasm!(self.func, { else_; });
                self.emit_expr(else_);
                self.depth_pop(_g0);
                wasm!(self.func, { end; });
            }

            // ── Block ──
            IrExprKind::Block { stmts, expr: tail } => {
                for stmt in stmts {
                    self.emit_stmt(stmt);
                }
                if let Some(e) = tail {
                    self.emit_expr(e);
                }
            }

            // ── DoBlock (with guard → loop) ──
            IrExprKind::DoBlock { stmts, expr: tail } => {
                // do block with guards: block { loop { stmts; tail?; br 0 } }
                // Guard breaks out via br to outer block.
                // Tail expr (if any) is stored in a scratch local, then break exits the loop.
                let has_tail = tail.is_some();
                let tail_vt = tail.as_ref().and_then(|e| values::ty_to_valtype(&e.ty));
                let result_local = if has_tail && tail_vt.is_some() {
                    // Use i64 scratch for i64/f64, i32 scratch for i32
                    match tail_vt {
                        Some(ValType::I64) | Some(ValType::F64) =>
                            Some(self.scratch.alloc_i64()),
                        _ =>
                            Some(self.scratch.alloc_i32()),
                    }
                } else { None };

                wasm!(self.func, { block_empty; });
                let _g1 = self.depth_push();
                let break_depth = _g1.saved();

                wasm!(self.func, { loop_empty; });
                let _g2 = self.depth_push();
                let continue_depth = _g2.saved();

                self.loop_stack.push(super::LoopLabels { break_depth, continue_depth });

                for stmt in stmts {
                    self.emit_stmt(stmt);
                }
                if let Some(e) = tail {
                    self.emit_expr(e);
                    if let Some(rl) = result_local {
                        // Non-unit tail: save result and break out
                        wasm!(self.func, { local_set(rl); });
                        wasm!(self.func, { br(self.depth - break_depth - 1); });
                    } else {
                        // Unit tail (side-effect only): drop value if any, continue looping
                        if values::ty_to_valtype(&e.ty).is_some() {
                            wasm!(self.func, { drop; });
                        }
                        wasm!(self.func, { br(self.depth - continue_depth - 1); });
                    }
                } else {
                    // No tail: continue looping
                    wasm!(self.func, { br(self.depth - continue_depth - 1); });
                }

                self.loop_stack.pop();
                self.depth_pop(_g2);
                wasm!(self.func, { end; }); // end loop
                self.depth_pop(_g1);
                wasm!(self.func, { end; }); // end block

                // After block: load saved result (if any)
                if let Some(rl) = result_local {
                    wasm!(self.func, { local_get(rl); });
                    // Free scratch in reverse order
                    match tail_vt {
                        Some(ValType::I64) | Some(ValType::F64) => self.scratch.free_i64(rl),
                        _ => self.scratch.free_i32(rl),
                    }
                }
            }

            // ── While loop ──
            IrExprKind::While { cond, body } => {
                wasm!(self.func, { block_empty; });
                let _g3 = self.depth_push();
                let break_depth = _g3.saved();

                wasm!(self.func, { loop_empty; });
                let _g4 = self.depth_push();
                let continue_depth = _g4.saved();

                self.loop_stack.push(super::LoopLabels { break_depth, continue_depth });

                // if !cond, break
                self.emit_expr(cond);
                wasm!(self.func, {
                    i32_eqz;
                    br_if(self.depth - break_depth - 1);
                });

                // body
                for stmt in body {
                    self.emit_stmt(stmt);
                }

                // continue (jump to loop start)
                wasm!(self.func, { br(self.depth - continue_depth - 1); });

                self.loop_stack.pop();
                self.depth_pop(_g4);
                wasm!(self.func, { end; }); // end loop
                self.depth_pop(_g3);
                wasm!(self.func, { end; }); // end block
            }

            // ── For-in loop ──
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.emit_for_in(*var, var_tuple.as_deref(), iterable, body);
            }

            IrExprKind::Break => {
                if let Some(labels) = self.loop_stack.last() {
                    let relative = self.depth - labels.break_depth - 1;
                    wasm!(self.func, { br(relative); });
                }
            }

            IrExprKind::Continue => {
                if let Some(labels) = self.loop_stack.last() {
                    let relative = self.depth - labels.continue_depth - 1;
                    wasm!(self.func, { br(relative); });
                }
            }

            // ── Function calls ──
            IrExprKind::Call { target, args, .. } => {
                self.emit_call(target, args, &expr.ty);
            }

            // ── String interpolation ──
            IrExprKind::StringInterp { parts } => {
                self.emit_string_interp(parts);
            }

            // ── Match ──
            IrExprKind::Match { subject, arms } => {
                self.emit_match(subject, arms, &expr.ty);
            }

            // ── Record/Variant construction ──
            IrExprKind::Record { name, fields, .. } => {
                self.emit_record(name.as_deref(), fields, &expr.ty);
            }

            // ── Spread record ──
            IrExprKind::SpreadRecord { base, fields } => {
                self.emit_spread_record(base, fields, &expr.ty);
            }

            // ── Tuple construction ──
            IrExprKind::Tuple { elements } => {
                self.emit_tuple(elements);
            }

            // ── Field access ──
            IrExprKind::Member { object, field } => {
                self.emit_member(object, field);
            }

            // ── Tuple index access ──
            IrExprKind::TupleIndex { object, index } => {
                self.emit_tuple_index(object, *index, &expr.ty);
            }

            // ── List construction ──
            IrExprKind::List { elements } => {
                self.emit_list(elements, &expr.ty);
            }

            // ── Index access (list[i]) ──
            IrExprKind::IndexAccess { object, index } => {
                self.emit_index_access(object, index, &expr.ty);
            }

            // ── Map ──
            IrExprKind::EmptyMap => {
                // Empty map: just [len=0:i32]
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(4);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    local_get(scratch);
                    i32_const(0);
                    i32_store(0);
                    local_get(scratch);
                });
                self.scratch.free_i32(scratch);
            }
            IrExprKind::MapLiteral { entries } => {
                // Map literal: [len:i32][key0][val0][key1][val1]...
                // For now, just allocate and store entries sequentially
                let n = entries.len() as u32;
                let entry_size = if let Some((k, v)) = entries.first() {
                    values::byte_size(&k.ty) + values::byte_size(&v.ty)
                } else { 8 };
                let total = 4 + n * entry_size;
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(total as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    // Store length
                    local_get(scratch);
                    i32_const(n as i32);
                    i32_store(0);
                });
                // Store entries
                let mut offset = 4u32;
                for (key, val) in entries {
                    wasm!(self.func, { local_get(scratch); });
                    self.emit_expr(key);
                    self.emit_store_at(&key.ty, offset);
                    offset += values::byte_size(&key.ty);
                    wasm!(self.func, { local_get(scratch); });
                    self.emit_expr(val);
                    self.emit_store_at(&val.ty, offset);
                    offset += values::byte_size(&val.ty);
                }
                wasm!(self.func, { local_get(scratch); });
                self.scratch.free_i32(scratch);
            }

            // ── Option/Result ──
            IrExprKind::OptionSome { expr: inner } => {
                // Resolve inner type: if Unknown, infer from outer Option type or inner expr
                let inner_ty = if matches!(inner.ty, Ty::Unknown) {
                    if let Ty::Applied(crate::types::constructor::TypeConstructorId::Option, args) = &expr.ty {
                        let candidate = args.first().cloned().unwrap_or(Ty::Unknown);
                        if !matches!(candidate, Ty::Unknown) { candidate }
                        else { self.infer_type_from_expr(inner) }
                    } else { self.infer_type_from_expr(inner) }
                } else { inner.ty.clone() };
                let inner_size = values::byte_size(&inner_ty);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(inner_size as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    local_get(scratch);
                });
                self.emit_expr(inner);
                self.emit_store_at(&inner_ty, 0);
                wasm!(self.func, { local_get(scratch); });
                self.scratch.free_i32(scratch);
            }
            IrExprKind::OptionNone => {
                wasm!(self.func, { i32_const(0); });
            }

            // ── Result ok/err ──
            IrExprKind::ResultOk { expr: inner } => {
                // ok(x) = [tag:0, value]
                // Resolve inner type: if Unknown, try to infer from the outer Result type or expr
                let inner_ty = if matches!(inner.ty, Ty::Unknown) {
                    self.resolve_result_inner_ty(expr, true)
                } else { inner.ty.clone() };
                let inner_size = values::byte_size(&inner_ty);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const((4 + inner_size) as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    // tag = 0
                    local_get(scratch);
                    i32_const(0);
                    i32_store(0);
                });
                if values::ty_to_valtype(&inner_ty).is_some() {
                    wasm!(self.func, { local_get(scratch); });
                    self.emit_expr(inner);
                    self.emit_store_at(&inner_ty, 4);
                }
                wasm!(self.func, { local_get(scratch); });
                self.scratch.free_i32(scratch);
            }
            IrExprKind::ResultErr { expr: inner } => {
                // err(e) = [tag:1, value]
                let inner_ty = if matches!(inner.ty, Ty::Unknown) {
                    self.resolve_result_inner_ty(expr, false)
                } else { inner.ty.clone() };
                let inner_size = values::byte_size(&inner_ty);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const((4 + inner_size) as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    // tag = 1
                    local_get(scratch);
                    i32_const(1);
                    i32_store(0);
                });
                if values::ty_to_valtype(&inner_ty).is_some() {
                    wasm!(self.func, { local_get(scratch); });
                    self.emit_expr(inner);
                    self.emit_store_at(&inner_ty, 4);
                }
                wasm!(self.func, { local_get(scratch); });
                self.scratch.free_i32(scratch);
            }

            // ── Fan block (sequential fallback — no parallelism in WASM) ──
            IrExprKind::Fan { exprs } => {
                if exprs.len() == 1 {
                    // Single expr: emit with auto-unwrap if Result
                    self.emit_expr(&exprs[0]);
                    if let Ty::Applied(crate::types::constructor::TypeConstructorId::Result, _) = &exprs[0].ty {
                        let scratch = self.scratch.alloc_i32();
                        wasm!(self.func, {
                            local_set(scratch);
                            local_get(scratch); i32_load(0); i32_const(0); i32_ne;
                            if_empty; local_get(scratch); return_; end;
                            local_get(scratch);
                        });
                        self.emit_load_at(&expr.ty, 4);
                        self.scratch.free_i32(scratch);
                    }
                } else {
                    // Fan with multiple exprs → Tuple of unwrapped results
                    // Each expr returns Result[T, E]. Unwrap each, build tuple of T values.
                    let elem_types: Vec<Ty> = if let Ty::Tuple(tys) = &expr.ty {
                        tys.clone()
                    } else {
                        exprs.iter().map(|e| e.ty.clone()).collect()
                    };
                    let total_size: u32 = elem_types.iter().map(|t| values::byte_size(t)).sum();
                    let tuple_scratch = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        i32_const(total_size as i32);
                        call(self.emitter.rt.alloc);
                        local_set(tuple_scratch);
                    });
                    let mut offset = 0u32;
                    for (i, e) in exprs.iter().enumerate() {
                        let elem_ty = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                        let elem_size = values::byte_size(&elem_ty);
                        // Fan exprs are typically effect fn calls → Result[T, E]
                        // Auto-unwrap: if err, return Result early; if ok, store unwrapped value
                        let is_result = matches!(&e.ty, Ty::Applied(crate::types::constructor::TypeConstructorId::Result, _));
                        if is_result {
                            self.emit_expr(e);
                            let res_scratch = self.scratch.alloc_i32();
                            wasm!(self.func, {
                                local_set(res_scratch);
                                local_get(res_scratch); i32_load(0); i32_const(0); i32_ne;
                                if_empty; local_get(res_scratch); return_; end;
                                local_get(tuple_scratch);
                                local_get(res_scratch);
                            });
                            self.emit_load_at(&elem_ty, 4);
                            self.emit_store_at(&elem_ty, offset);
                            self.scratch.free_i32(res_scratch);
                        } else {
                            // Non-Result: push tuple_ptr, emit expr, store
                            wasm!(self.func, { local_get(tuple_scratch); });
                            self.emit_expr(e);
                            self.emit_store_at(&elem_ty, offset);
                        }
                        offset += elem_size;
                    }
                    wasm!(self.func, { local_get(tuple_scratch); });
                    self.scratch.free_i32(tuple_scratch);
                }
            }

            // ── Try (auto-unwrap Result in effect fn) ──
            IrExprKind::Try { expr: inner } => {
                // Evaluate inner (returns Result ptr: [tag:i32][value])
                // If tag == 0 (ok): unwrap → push value
                // If tag != 0 (err): return the Result as-is
                self.emit_expr(inner);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(scratch);
                    // Check tag
                    local_get(scratch);
                    i32_load(0);
                    i32_const(0);
                    i32_ne;
                    if_empty;
                    // Err: return the Result ptr
                    local_get(scratch);
                    return_;
                    end;
                    // Ok: load the unwrapped value
                    local_get(scratch);
                });
                self.emit_load_at(&expr.ty, 4);
                self.scratch.free_i32(scratch);
            }

            // ── Map index access: m[key] → Option[V] ──
            IrExprKind::MapAccess { object, key } => {
                let fake_args = vec![(**object).clone(), (**key).clone()];
                self.emit_map_call("get", &fake_args);
            }

            // ── Codegen-specific nodes (pass-through or ignore) ──
            IrExprKind::Clone { expr: inner } | IrExprKind::Deref { expr: inner } => {
                self.emit_expr(inner);
            }

            // ── Unsupported ──
            _ => {
                wasm!(self.func, { unreachable; });
            }
        }
    }

    pub(super) fn emit_binop(&mut self, op: BinOp, left: &IrExpr, right: &IrExpr) {
        match op {
            // ── Arithmetic ──
            BinOp::AddInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { i64_add; });
            }
            BinOp::SubInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { i64_sub; });
            }
            BinOp::MulInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { i64_mul; });
            }
            BinOp::DivInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { i64_div_s; });
            }
            BinOp::ModInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { i64_rem_s; });
            }
            BinOp::AddFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { f64_add; });
            }
            BinOp::SubFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { f64_sub; });
            }
            BinOp::MulFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { f64_mul; });
            }
            BinOp::DivFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { f64_div; });
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
            BinOp::Eq => self.emit_eq(left, right, false),
            BinOp::Neq => self.emit_eq(left, right, true),
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
                // Determine element size from left's type
                let elem_size = if let Ty::Applied(_, args) = &left.ty {
                    args.first().map(|t| values::byte_size(t)).unwrap_or(8)
                } else { 8 };
                wasm!(self.func, {
                    i32_const(elem_size as i32);
                    call(self.emitter.rt.concat_list);
                });
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
                // Float power: check if exp == 0.5 → sqrt, else integer loop
                let base_s = self.scratch.alloc_i64();
                let result_s = self.scratch.alloc_i64();
                let counter_s = self.scratch.alloc_i32();
                self.emit_expr(left);
                wasm!(self.func, { i64_reinterpret_f64; local_set(base_s); });
                self.emit_expr(right);
                // Check if exp == 0.5
                wasm!(self.func, {
                    f64_const(0.5);
                    f64_eq;
                    if_f64;
                    local_get(base_s);
                    f64_reinterpret_i64;
                    f64_sqrt;
                    else_;
                });
                // Integer loop for non-0.5 exponent
                self.emit_expr(right);
                wasm!(self.func, {
                    i64_trunc_f64_s;
                    i32_wrap_i64;
                    local_set(counter_s);
                    f64_const(1.0);
                    i64_reinterpret_f64;
                    local_set(result_s);
                    block_empty;
                    loop_empty;
                    local_get(counter_s);
                    i32_eqz;
                    br_if(1);
                    local_get(result_s);
                    f64_reinterpret_i64;
                    local_get(base_s);
                    f64_reinterpret_i64;
                    f64_mul;
                    i64_reinterpret_f64;
                    local_set(result_s);
                    local_get(counter_s);
                    i32_const(1);
                    i32_sub;
                    local_set(counter_s);
                    br(0);
                    end;
                    end;
                    local_get(result_s);
                    f64_reinterpret_i64;
                    end;
                });
                self.scratch.free_i32(counter_s);
                self.scratch.free_i64(result_s);
                self.scratch.free_i64(base_s);
            }
            BinOp::XorInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::I64Xor);
            }
        }
    }

}

/// Collect type parameter names from a type (Named("X", []) where X is a single-letter or TypeVar).
pub(super) fn collect_type_param_names<'a>(ty: &'a Ty, names: &mut Vec<&'a str>) {
    match ty {
        Ty::Named(name, args) if args.is_empty() && name.len() <= 2 && name.chars().next().map_or(false, |c| c.is_uppercase()) => {
            if !names.contains(&name.as_str()) {
                names.push(name.as_str());
            }
        }
        Ty::TypeVar(name) => {
            if !names.contains(&name.as_str()) {
                names.push(name.as_str());
            }
        }
        Ty::Applied(_, args) => { for a in args { collect_type_param_names(a, names); } }
        Ty::Tuple(elems) => { for e in elems { collect_type_param_names(e, names); } }
        Ty::Fn { params, ret } => {
            for p in params { collect_type_param_names(p, names); }
            collect_type_param_names(ret, names);
        }
        _ => {}
    }
}

/// Substitute type parameters in a type. Named("T", []) → type_args[index of "T"].
pub(super) fn substitute_type_params(ty: &Ty, generic_names: &[&str], type_args: &[Ty]) -> Ty {
    match ty {
        Ty::Named(name, args) if args.is_empty() => {
            // Check if this is a type parameter name
            if let Some(idx) = generic_names.iter().position(|&g| g == name.as_str()) {
                if let Some(concrete) = type_args.get(idx) {
                    return concrete.clone();
                }
            }
            // Also check TypeVar style
            ty.clone()
        }
        Ty::TypeVar(name) => {
            if let Some(idx) = generic_names.iter().position(|&g| g == name.as_str()) {
                if let Some(concrete) = type_args.get(idx) {
                    return concrete.clone();
                }
            }
            ty.clone()
        }
        // Recursively substitute in all other type constructors
        _ => ty.map_children(&|child| substitute_type_params(child, generic_names, type_args)),
    }
}

impl FuncCompiler<'_> {
    /// Resolve the inner type of a ResultOk/ResultErr when inner.ty is Unknown.
    /// Tries: 1) outer expr.ty Result[T,E] args, 2) inner expr IR kind inference.
    pub(super) fn resolve_result_inner_ty(&self, expr: &IrExpr, is_ok: bool) -> Ty {
        use crate::types::constructor::TypeConstructorId;
        // Try from outer Result type
        if let Ty::Applied(TypeConstructorId::Result, args) = &expr.ty {
            let candidate = if is_ok {
                args.first().cloned().unwrap_or(Ty::Unknown)
            } else {
                args.get(1).cloned().unwrap_or(Ty::Unknown)
            };
            if !matches!(candidate, Ty::Unknown) {
                return candidate;
            }
        }
        // Fall back to inferring from inner expr
        let inner = match &expr.kind {
            IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e } => e,
            _ => return Ty::Int,
        };
        self.infer_type_from_expr(inner)
    }

    /// Best-effort type inference from IR expression structure.
    pub(super) fn infer_type_from_expr(&self, expr: &IrExpr) -> Ty {
        if !matches!(expr.ty, Ty::Unknown) {
            return expr.ty.clone();
        }
        match &expr.kind {
            IrExprKind::LitInt { .. } => Ty::Int,
            IrExprKind::LitFloat { .. } => Ty::Float,
            IrExprKind::LitBool { .. } => Ty::Bool,
            IrExprKind::LitStr { .. } => Ty::String,
            IrExprKind::BinOp { op, left, .. } => {
                match op {
                    BinOp::AddInt | BinOp::SubInt | BinOp::MulInt | BinOp::DivInt | BinOp::ModInt
                    | BinOp::PowInt | BinOp::XorInt => Ty::Int,
                    BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat | BinOp::DivFloat
                    | BinOp::ModFloat | BinOp::PowFloat => Ty::Float,
                    BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte
                    | BinOp::And | BinOp::Or => Ty::Bool,
                    BinOp::ConcatStr => Ty::String,
                    BinOp::ConcatList => {
                        let lt = self.infer_type_from_expr(left);
                        lt
                    }
                }
            }
            IrExprKind::UnOp { op, .. } => {
                match op {
                    UnOp::NegInt => Ty::Int,
                    UnOp::NegFloat => Ty::Float,
                    UnOp::Not => Ty::Bool,
                }
            }
            IrExprKind::Var { id } => {
                self.var_table.get(*id).ty.clone()
            }
            _ => Ty::Int, // conservative fallback
        }
    }
}
