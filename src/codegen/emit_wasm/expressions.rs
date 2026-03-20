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
                    wasm!(self.func, { local_get(local_idx); });
                } else if let Some(&(global_idx, _)) = self.emitter.top_let_globals.get(&id.0) {
                    wasm!(self.func, { global_get(global_idx); });
                } else {
                    // VarId not in var_map — try name-based lookup as fallback
                    // (handles VarId mismatch between lowering passes)
                    let name = if (id.0 as usize) < self.var_table.len() { &self.var_table.get(*id).name } else { "" };
                    let found = if !name.is_empty() {
                        self.var_map.iter().find_map(|(&vid, &lidx)| {
                            if (vid as usize) < self.var_table.len() && self.var_table.get(crate::ir::VarId(vid)).name == name {
                                Some(lidx)
                            } else { None }
                        })
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
                self.depth += 1;
                self.emit_expr(then);
                wasm!(self.func, { else_; });
                self.emit_expr(else_);
                self.depth -= 1;
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
                // do block with guards: block { loop { stmts; br 0 (continue) } }
                // Guard breaks out of the outer block
                let break_depth = self.depth;
                wasm!(self.func, { block_empty; });
                self.depth += 1;

                let continue_depth = self.depth;
                wasm!(self.func, { loop_empty; });
                self.depth += 1;

                self.loop_stack.push(super::LoopLabels { break_depth, continue_depth });

                for stmt in stmts {
                    self.emit_stmt(stmt);
                }
                if let Some(e) = tail {
                    self.emit_expr(e);
                    // Drop tail value if non-Unit (do blocks in stmt position)
                    if values::ty_to_valtype(&e.ty).is_some() {
                        wasm!(self.func, { drop; });
                    }
                }

                // Continue (loop back)
                wasm!(self.func, { br(self.depth - continue_depth - 1); });

                self.loop_stack.pop();
                self.depth -= 1;
                wasm!(self.func, { end; }); // end loop
                self.depth -= 1;
                wasm!(self.func, { end; }); // end block
            }

            // ── While loop ──
            IrExprKind::While { cond, body } => {
                let break_depth = self.depth;
                wasm!(self.func, { block_empty; });
                self.depth += 1;

                let continue_depth = self.depth;
                wasm!(self.func, { loop_empty; });
                self.depth += 1;

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
                self.depth -= 1;
                wasm!(self.func, { end; }); // end loop
                self.depth -= 1;
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
                let scratch = self.match_i32_base + self.match_depth;
                wasm!(self.func, {
                    i32_const(4);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    local_get(scratch);
                    i32_const(0);
                    i32_store(0);
                    local_get(scratch);
                });
            }
            IrExprKind::MapLiteral { entries } => {
                // Map literal: [len:i32][key0][val0][key1][val1]...
                // For now, just allocate and store entries sequentially
                let n = entries.len() as u32;
                let entry_size = if let Some((k, v)) = entries.first() {
                    values::byte_size(&k.ty) + values::byte_size(&v.ty)
                } else { 8 };
                let total = 4 + n * entry_size;
                let scratch = self.match_i32_base + self.match_depth;
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
            }

            // ── Option/Result ──
            IrExprKind::OptionSome { expr: inner } => {
                // Allocate space for the inner value, store it, return pointer
                let inner_size = values::byte_size(&inner.ty);
                let scratch = self.match_i32_base + self.match_depth;
                wasm!(self.func, {
                    i32_const(inner_size as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    local_get(scratch);
                });
                self.emit_expr(inner);
                self.emit_store_at(&inner.ty, 0);
                wasm!(self.func, { local_get(scratch); });
            }
            IrExprKind::OptionNone => {
                wasm!(self.func, { i32_const(0); });
            }

            // ── Result ok/err ──
            IrExprKind::ResultOk { expr: inner } => {
                // ok(x) = [tag:0, value]
                let inner_size = values::byte_size(&inner.ty);
                let scratch = self.match_i32_base + self.match_depth;
                wasm!(self.func, {
                    i32_const((4 + inner_size) as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    // tag = 0
                    local_get(scratch);
                    i32_const(0);
                    i32_store(0);
                    // value
                    local_get(scratch);
                });
                self.emit_expr(inner);
                self.emit_store_at(&inner.ty, 4);
                wasm!(self.func, { local_get(scratch); });
            }
            IrExprKind::ResultErr { expr: inner } => {
                // err(e) = [tag:1, value]
                let inner_size = values::byte_size(&inner.ty);
                let scratch = self.match_i32_base + self.match_depth;
                wasm!(self.func, {
                    i32_const((4 + inner_size) as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    // tag = 1
                    local_get(scratch);
                    i32_const(1);
                    i32_store(0);
                    // value
                    local_get(scratch);
                });
                self.emit_expr(inner);
                self.emit_store_at(&inner.ty, 4);
                wasm!(self.func, { local_get(scratch); });
            }

            // ── Fan block (sequential fallback — no parallelism in WASM) ──
            IrExprKind::Fan { exprs } => {
                if exprs.len() == 1 {
                    self.emit_expr(&exprs[0]);
                } else {
                    // Fan with multiple exprs → Tuple of results
                    self.emit_tuple(exprs);
                }
            }

            // ── Try (auto-unwrap Result in effect fn) ──
            IrExprKind::Try { expr: inner } => {
                // Evaluate inner (returns Result ptr: [tag:i32][value])
                // If tag == 0 (ok): unwrap → push value
                // If tag != 0 (err): return the Result as-is
                self.emit_expr(inner);
                let scratch = self.match_i32_base + self.match_depth;
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
                let base_s = self.match_i64_base + self.match_depth;
                let result_s = base_s + 1;
                let counter_s = self.match_i32_base + self.match_depth;
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
            }
            BinOp::PowFloat => {
                // Float power: check if exp == 0.5 → sqrt, else integer loop
                let base_s = self.match_i64_base + self.match_depth;
                let result_s = base_s + 1;
                let counter_s = self.match_i32_base + self.match_depth;
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
            }
            BinOp::XorInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::I64Xor);
            }
        }
    }

    pub(super) fn emit_eq(&mut self, left: &IrExpr, right: &IrExpr, negate: bool) {
        self.emit_expr(left);
        self.emit_expr(right);
        // Use the more concrete type for comparison dispatch
        let cmp_ty = if matches!(&left.ty, Ty::Unknown | Ty::TypeVar(_)) { &right.ty } else { &left.ty };
        match cmp_ty {
            Ty::Int => { wasm!(self.func, { i64_eq; }); }
            Ty::Float => { wasm!(self.func, { f64_eq; }); }
            Ty::Bool => { wasm!(self.func, { i32_eq; }); }
            Ty::String => { wasm!(self.func, { call(self.emitter.rt.str_eq); }); }
            Ty::Applied(crate::types::constructor::TypeConstructorId::List, args) => {
                let elem_size = args.first().map(|t| values::byte_size(t)).unwrap_or(8);
                wasm!(self.func, {
                    i32_const(elem_size as i32);
                    call(self.emitter.rt.list_eq);
                });
            }
            // Record: byte-compare the entire struct
            Ty::Record { fields } => {
                let size = values::record_size(fields);
                wasm!(self.func, {
                    i32_const(size as i32);
                    call(self.emitter.rt.mem_eq);
                });
            }
            // Named types (records/variants): compute size from registered fields
            Ty::Named(name, _) => {
                if let Some(cases) = self.emitter.variant_info.get(name.as_str()) {
                    // Variant: tag(4) + max payload size across all constructors
                    let max_payload = cases.iter()
                        .map(|c| values::record_size(&c.fields))
                        .max().unwrap_or(0);
                    let size = 4 + max_payload;
                    wasm!(self.func, {
                        i32_const(size as i32);
                        call(self.emitter.rt.mem_eq);
                    });
                } else {
                    let fields = self.emitter.record_fields.get(name.as_str()).cloned().unwrap_or_default();
                    let size = values::record_size(&fields);
                    if size > 0 {
                        wasm!(self.func, {
                            i32_const(size as i32);
                            call(self.emitter.rt.mem_eq);
                        });
                    } else {
                        wasm!(self.func, { i32_eq; });
                    }
                }
            }
            // Tuple: byte-compare all elements
            Ty::Tuple(elems) => {
                let size: u32 = elems.iter().map(|t| values::byte_size(t)).sum();
                wasm!(self.func, {
                    i32_const(size as i32);
                    call(self.emitter.rt.mem_eq);
                });
            }
            // Option: deep equality via runtime
            Ty::Applied(crate::types::constructor::TypeConstructorId::Option, args) => {
                match args.first() {
                    Some(Ty::String) => { wasm!(self.func, { call(self.emitter.rt.option_eq_str); }); }
                    _ => { wasm!(self.func, { call(self.emitter.rt.option_eq_i64); }); }
                }
            }
            // Result: deep equality via runtime
            Ty::Applied(crate::types::constructor::TypeConstructorId::Result, _) => {
                wasm!(self.func, { call(self.emitter.rt.result_eq_i64_str); });
            }
            _ => { wasm!(self.func, { i32_eq; }); }
        }
        if negate {
            wasm!(self.func, { i32_eqz; });
        }
    }

    pub(super) fn emit_cmp_instruction(&mut self, ty: &Ty, kind: CmpKind) {
        match (ty, kind) {
            (Ty::Int, CmpKind::Lt) => { wasm!(self.func, { i64_lt_s; }); }
            (Ty::Int, CmpKind::Gt) => { wasm!(self.func, { i64_gt_s; }); }
            (Ty::Int, CmpKind::Lte) => { wasm!(self.func, { i64_le_s; }); }
            (Ty::Int, CmpKind::Gte) => { wasm!(self.func, { i64_ge_s; }); }
            (Ty::Float, CmpKind::Lt) => { wasm!(self.func, { f64_lt; }); }
            (Ty::Float, CmpKind::Gt) => { wasm!(self.func, { f64_gt; }); }
            (Ty::Float, CmpKind::Lte) => { wasm!(self.func, { f64_le; }); }
            (Ty::Float, CmpKind::Gte) => { wasm!(self.func, { f64_ge; }); }
            _ => { wasm!(self.func, { unreachable; }); }
        }
    }

    /// Emit a store instruction for a value at base_ptr + offset.
    /// Assumes base_ptr is already on stack, followed by the value.
    pub fn emit_store_at(&mut self, ty: &Ty, offset: u32) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => {
                wasm!(self.func, { i64_store(offset); });
            }
            Some(ValType::F64) => {
                wasm!(self.func, { f64_store(offset); });
            }
            Some(ValType::I32) => {
                wasm!(self.func, { i32_store(offset); });
            }
            _ => {}
        }
    }

    /// Emit a load instruction from base_ptr (on stack) + offset.
    pub fn emit_load_at(&mut self, ty: &Ty, offset: u32) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => {
                wasm!(self.func, { i64_load(offset); });
            }
            Some(ValType::F64) => {
                wasm!(self.func, { f64_load(offset); });
            }
            Some(ValType::I32) => {
                wasm!(self.func, { i32_load(offset); });
            }
            _ => {}
        }
    }

    /// Returns 4 if the type is a variant (fields start after tag), 0 otherwise.
    pub(super) fn variant_tag_offset(&self, ty: &Ty) -> u32 {
        if let Ty::Named(name, _) = ty {
            if self.emitter.variant_info.contains_key(name.as_str()) {
                return 4;
            }
        }
        // Also check Variant type directly
        if let Ty::Variant { .. } = ty {
            return 4;
        }
        0
    }

    /// Extract field names and types from a record/named type.
    /// For generic types like Box[Int], substitutes type parameters.
    pub(super) fn extract_record_fields(&self, ty: &Ty) -> Vec<(String, Ty)> {
        match ty {
            Ty::Record { fields } => fields.clone(),
            Ty::Named(name, type_args) => {
                if let Some(fields) = self.emitter.record_fields.get(name.as_str()) {
                    if type_args.is_empty() {
                        fields.clone()
                    } else {
                        // Extract generic param names from field types (in order of first appearance)
                        let mut generic_names: Vec<&str> = Vec::new();
                        for (_, fty) in fields {
                            collect_type_param_names(fty, &mut generic_names);
                        }
                        fields.iter().map(|(fname, fty)| {
                            let resolved = substitute_type_params(fty, &generic_names, type_args);
                            (fname.clone(), resolved)
                        }).collect()
                    }
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }

    /// Find local index for a pattern field binding by name.
    pub(super) fn find_var_by_field(&self, field_name: &str, _case_fields: &[(String, Ty)]) -> Option<&u32> {
        // Search var_map for VarIds whose name in var_table matches field_name
        for (&var_id, local_idx) in &self.var_map {
            if (var_id as usize) < self.var_table.len() {
                let info = self.var_table.get(crate::ir::VarId(var_id));
                if info.name == field_name {
                    return Some(local_idx);
                }
            }
        }
        None
    }

}

/// Collect type parameter names from a type (Named("X", []) where X is a single-letter or TypeVar).
fn collect_type_param_names<'a>(ty: &'a Ty, names: &mut Vec<&'a str>) {
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
fn substitute_type_params(ty: &Ty, generic_names: &[&str], type_args: &[Ty]) -> Ty {
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
        _ => ty.clone(),
    }
}

impl FuncCompiler<'_> {
    /// Find variant tag for a unit constructor called as a function (e.g., `Red`).
    pub(super) fn find_unit_variant_tag(&self, name: &str) -> Option<u32> {
        for cases in self.emitter.variant_info.values() {
            for case in cases {
                if case.name == name && case.fields.is_empty() {
                    return Some(case.tag);
                }
            }
        }
        None
    }

    /// Find variant constructor tag. Returns (tag, is_unit).
    pub(super) fn find_variant_ctor_tag(&self, name: &str) -> Option<(u32, bool)> {
        for cases in self.emitter.variant_info.values() {
            for case in cases {
                if case.name == name {
                    return Some((case.tag, case.fields.is_empty()));
                }
            }
        }
        None
    }

    /// Find the variant tag for a constructor name, searching variant_info by subject type.
    pub(super) fn find_variant_tag_by_ctor(&self, ctor_name: &str, subject_ty: &Ty) -> Option<u32> {
        let type_name = match subject_ty {
            Ty::Named(name, _) => name.as_str(),
            Ty::Variant { name, .. } => name.as_str(),
            _ => return None,
        };
        let cases = self.emitter.variant_info.get(type_name)?;
        cases.iter().find(|c| c.name == ctor_name).map(|c| c.tag)
    }
}
