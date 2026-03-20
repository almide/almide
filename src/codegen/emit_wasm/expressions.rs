//! IrExpr → WASM instruction emission.

use crate::ir::{BinOp, IrExpr, IrExprKind, UnOp};
use crate::types::Ty;
use wasm_encoder::{BlockType, Instruction, MemArg, ValType};

use super::FuncCompiler;
use super::values;

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
                self.func.instruction(&Instruction::I64Const(*value));
            }
            IrExprKind::LitFloat { value } => {
                self.func.instruction(&Instruction::F64Const(*value));
            }
            IrExprKind::LitBool { value } => {
                self.func.instruction(&Instruction::I32Const(*value as i32));
            }
            IrExprKind::LitStr { value } => {
                let offset = self.emitter.intern_string(value);
                self.func.instruction(&Instruction::I32Const(offset as i32));
            }
            IrExprKind::Unit => {
                // Unit produces no value on the stack
            }

            // ── Variables ──
            IrExprKind::Var { id } => {
                if let Some(&local_idx) = self.var_map.get(&id.0) {
                    self.func.instruction(&Instruction::LocalGet(local_idx));
                } else if let Some(&(global_idx, _)) = self.emitter.top_let_globals.get(&id.0) {
                    self.func.instruction(&Instruction::GlobalGet(global_idx));
                } else {
                    // Variable not in scope — push zero
                    match values::ty_to_valtype(&expr.ty) {
                        Some(ValType::I64) => { self.func.instruction(&Instruction::I64Const(0)); }
                        Some(ValType::F64) => { self.func.instruction(&Instruction::F64Const(0.0)); }
                        Some(ValType::I32) => { self.func.instruction(&Instruction::I32Const(0)); }
                        _ => {}
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
                        self.func.instruction(&Instruction::I64Const(0));
                        self.emit_expr(operand);
                        self.func.instruction(&Instruction::I64Sub);
                    }
                    UnOp::NegFloat => {
                        self.emit_expr(operand);
                        self.func.instruction(&Instruction::F64Neg);
                    }
                    UnOp::Not => {
                        self.emit_expr(operand);
                        self.func.instruction(&Instruction::I32Eqz);
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
                self.func.instruction(&Instruction::Else);
                self.emit_expr(else_);
                self.depth -= 1;
                self.func.instruction(&Instruction::End);
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
                self.func.instruction(&Instruction::Block(BlockType::Empty));
                self.depth += 1;

                let continue_depth = self.depth;
                self.func.instruction(&Instruction::Loop(BlockType::Empty));
                self.depth += 1;

                self.loop_stack.push(super::LoopLabels { break_depth, continue_depth });

                for stmt in stmts {
                    self.emit_stmt(stmt);
                }
                if let Some(e) = tail {
                    self.emit_expr(e);
                    // Drop tail value if non-Unit (do blocks in stmt position)
                    if values::ty_to_valtype(&e.ty).is_some() {
                        self.func.instruction(&Instruction::Drop);
                    }
                }

                // Continue (loop back)
                self.func.instruction(&Instruction::Br(self.depth - continue_depth - 1));

                self.loop_stack.pop();
                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end loop
                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end block
            }

            // ── While loop ──
            IrExprKind::While { cond, body } => {
                let break_depth = self.depth;
                self.func.instruction(&Instruction::Block(BlockType::Empty));
                self.depth += 1;

                let continue_depth = self.depth;
                self.func.instruction(&Instruction::Loop(BlockType::Empty));
                self.depth += 1;

                self.loop_stack.push(super::LoopLabels { break_depth, continue_depth });

                // if !cond, break
                self.emit_expr(cond);
                self.func.instruction(&Instruction::I32Eqz);
                self.func.instruction(&Instruction::BrIf(self.depth - break_depth - 1));

                // body
                for stmt in body {
                    self.emit_stmt(stmt);
                }

                // continue (jump to loop start)
                self.func.instruction(&Instruction::Br(self.depth - continue_depth - 1));

                self.loop_stack.pop();
                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end loop
                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end block
            }

            // ── For-in loop ──
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.emit_for_in(*var, var_tuple.as_deref(), iterable, body);
            }

            IrExprKind::Break => {
                if let Some(labels) = self.loop_stack.last() {
                    let relative = self.depth - labels.break_depth - 1;
                    self.func.instruction(&Instruction::Br(relative));
                }
            }

            IrExprKind::Continue => {
                if let Some(labels) = self.loop_stack.last() {
                    let relative = self.depth - labels.continue_depth - 1;
                    self.func.instruction(&Instruction::Br(relative));
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
                self.func.instruction(&Instruction::I32Const(4));
                self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                let scratch = self.match_i32_base + self.match_depth;
                self.func.instruction(&Instruction::LocalSet(scratch));
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Const(0));
                self.func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));
                self.func.instruction(&Instruction::LocalGet(scratch));
            }
            IrExprKind::MapLiteral { entries } => {
                // Map literal: [len:i32][key0][val0][key1][val1]...
                // For now, just allocate and store entries sequentially
                let n = entries.len() as u32;
                let entry_size = if let Some((k, v)) = entries.first() {
                    values::byte_size(&k.ty) + values::byte_size(&v.ty)
                } else { 8 };
                let total = 4 + n * entry_size;
                self.func.instruction(&Instruction::I32Const(total as i32));
                self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                let scratch = self.match_i32_base + self.match_depth;
                self.func.instruction(&Instruction::LocalSet(scratch));
                // Store length
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Const(n as i32));
                self.func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));
                // Store entries
                let mut offset = 4u32;
                for (key, val) in entries {
                    self.func.instruction(&Instruction::LocalGet(scratch));
                    self.emit_expr(key);
                    self.emit_store_at(&key.ty, offset);
                    offset += values::byte_size(&key.ty);
                    self.func.instruction(&Instruction::LocalGet(scratch));
                    self.emit_expr(val);
                    self.emit_store_at(&val.ty, offset);
                    offset += values::byte_size(&val.ty);
                }
                self.func.instruction(&Instruction::LocalGet(scratch));
            }

            // ── Option/Result ──
            IrExprKind::OptionSome { expr: inner } => {
                // Allocate space for the inner value, store it, return pointer
                let inner_size = values::byte_size(&inner.ty);
                self.func.instruction(&Instruction::I32Const(inner_size as i32));
                self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                let scratch = self.match_i32_base + self.match_depth;
                self.func.instruction(&Instruction::LocalSet(scratch));
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.emit_expr(inner);
                self.emit_store_at(&inner.ty, 0);
                self.func.instruction(&Instruction::LocalGet(scratch));
            }
            IrExprKind::OptionNone => {
                self.func.instruction(&Instruction::I32Const(0));
            }

            // ── Result ok/err ──
            IrExprKind::ResultOk { expr: inner } => {
                // ok(x) = [tag:0, value]
                let inner_size = values::byte_size(&inner.ty);
                self.func.instruction(&Instruction::I32Const((4 + inner_size) as i32));
                self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                let scratch = self.match_i32_base + self.match_depth;
                self.func.instruction(&Instruction::LocalSet(scratch));
                // tag = 0
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Const(0));
                self.func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));
                // value
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.emit_expr(inner);
                self.emit_store_at(&inner.ty, 4);
                self.func.instruction(&Instruction::LocalGet(scratch));
            }
            IrExprKind::ResultErr { expr: inner } => {
                // err(e) = [tag:1, value]
                let inner_size = values::byte_size(&inner.ty);
                self.func.instruction(&Instruction::I32Const((4 + inner_size) as i32));
                self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                let scratch = self.match_i32_base + self.match_depth;
                self.func.instruction(&Instruction::LocalSet(scratch));
                // tag = 1
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Const(1));
                self.func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));
                // value
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.emit_expr(inner);
                self.emit_store_at(&inner.ty, 4);
                self.func.instruction(&Instruction::LocalGet(scratch));
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
                self.func.instruction(&Instruction::LocalSet(scratch));

                // Check tag
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
                self.func.instruction(&Instruction::I32Const(0));
                self.func.instruction(&Instruction::I32Ne); // tag != 0 = err

                self.func.instruction(&Instruction::If(BlockType::Empty));
                // Err: return the Result ptr
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::Return);
                self.func.instruction(&Instruction::End);

                // Ok: load the unwrapped value
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.emit_load_at(&expr.ty, 4);
            }

            // ── Codegen-specific nodes (pass-through or ignore) ──
            IrExprKind::Clone { expr: inner } | IrExprKind::Deref { expr: inner } => {
                self.emit_expr(inner);
            }

            // ── Unsupported ──
            _ => {
                self.func.instruction(&Instruction::Unreachable);
            }
        }
    }

    pub(super) fn emit_binop(&mut self, op: BinOp, left: &IrExpr, right: &IrExpr) {
        match op {
            // ── Arithmetic ──
            BinOp::AddInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::I64Add);
            }
            BinOp::SubInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::I64Sub);
            }
            BinOp::MulInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::I64Mul);
            }
            BinOp::DivInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::I64DivS);
            }
            BinOp::ModInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::I64RemS);
            }
            BinOp::AddFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::F64Add);
            }
            BinOp::SubFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::F64Sub);
            }
            BinOp::MulFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::F64Mul);
            }
            BinOp::DivFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::F64Div);
            }
            BinOp::ModFloat => {
                // WASM has no f64.rem; compute via: a - trunc(a/b) * b
                self.emit_expr(left);
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::F64Div);
                self.func.instruction(&Instruction::F64Trunc);
                self.emit_expr(right);
                self.func.instruction(&Instruction::F64Mul);
                self.func.instruction(&Instruction::F64Sub);
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
                self.func.instruction(&Instruction::I32And);
            }
            BinOp::Or => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.func.instruction(&Instruction::I32Or);
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
                self.func.instruction(&Instruction::I32Const(elem_size as i32));
                self.func.instruction(&Instruction::Call(self.emitter.rt.concat_list));
            }

            BinOp::PowInt | BinOp::PowFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                // Quick hack: use a loop (right is usually small int)
                // For now, just multiply — only works for x^2 case
                self.func.instruction(&Instruction::Unreachable);
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
        match &left.ty {
            Ty::Int => { self.func.instruction(&Instruction::I64Eq); }
            Ty::Float => { self.func.instruction(&Instruction::F64Eq); }
            Ty::Bool => { self.func.instruction(&Instruction::I32Eq); }
            Ty::String => { self.func.instruction(&Instruction::Call(self.emitter.rt.str_eq)); }
            Ty::Applied(crate::types::constructor::TypeConstructorId::List, args) => {
                let elem_size = args.first().map(|t| values::byte_size(t)).unwrap_or(8);
                self.func.instruction(&Instruction::I32Const(elem_size as i32));
                self.func.instruction(&Instruction::Call(self.emitter.rt.list_eq));
            }
            // Record: byte-compare the entire struct
            Ty::Record { fields } => {
                let size = values::record_size(fields);
                self.func.instruction(&Instruction::I32Const(size as i32));
                self.func.instruction(&Instruction::Call(self.emitter.rt.mem_eq));
            }
            // Named types (records/variants): compute size from registered fields
            Ty::Named(name, _) => {
                let fields = self.emitter.record_fields.get(name.as_str()).cloned().unwrap_or_default();
                let tag_offset = if self.emitter.variant_info.contains_key(name.as_str()) { 4u32 } else { 0 };
                let size = tag_offset + values::record_size(&fields);
                if size > 0 {
                    self.func.instruction(&Instruction::I32Const(size as i32));
                    self.func.instruction(&Instruction::Call(self.emitter.rt.mem_eq));
                } else {
                    self.func.instruction(&Instruction::I32Eq);
                }
            }
            // Tuple: byte-compare all elements
            Ty::Tuple(elems) => {
                let size: u32 = elems.iter().map(|t| values::byte_size(t)).sum();
                self.func.instruction(&Instruction::I32Const(size as i32));
                self.func.instruction(&Instruction::Call(self.emitter.rt.mem_eq));
            }
            // Option: deep equality via runtime
            Ty::Applied(crate::types::constructor::TypeConstructorId::Option, args) => {
                match args.first() {
                    Some(Ty::String) => self.func.instruction(&Instruction::Call(self.emitter.rt.option_eq_str)),
                    _ => self.func.instruction(&Instruction::Call(self.emitter.rt.option_eq_i64)),
                };
            }
            // Result: deep equality via runtime
            Ty::Applied(crate::types::constructor::TypeConstructorId::Result, _) => {
                self.func.instruction(&Instruction::Call(self.emitter.rt.result_eq_i64_str));
            }
            _ => { self.func.instruction(&Instruction::I32Eq); }
        }
        if negate {
            self.func.instruction(&Instruction::I32Eqz);
        }
    }

    pub(super) fn emit_cmp_instruction(&mut self, ty: &Ty, kind: CmpKind) {
        match (ty, kind) {
            (Ty::Int, CmpKind::Lt) => { self.func.instruction(&Instruction::I64LtS); }
            (Ty::Int, CmpKind::Gt) => { self.func.instruction(&Instruction::I64GtS); }
            (Ty::Int, CmpKind::Lte) => { self.func.instruction(&Instruction::I64LeS); }
            (Ty::Int, CmpKind::Gte) => { self.func.instruction(&Instruction::I64GeS); }
            (Ty::Float, CmpKind::Lt) => { self.func.instruction(&Instruction::F64Lt); }
            (Ty::Float, CmpKind::Gt) => { self.func.instruction(&Instruction::F64Gt); }
            (Ty::Float, CmpKind::Lte) => { self.func.instruction(&Instruction::F64Le); }
            (Ty::Float, CmpKind::Gte) => { self.func.instruction(&Instruction::F64Ge); }
            _ => { self.func.instruction(&Instruction::Unreachable); }
        }
    }

    /// Emit a store instruction for a value at base_ptr + offset.
    /// Assumes base_ptr is already on stack, followed by the value.
    pub fn emit_store_at(&mut self, ty: &Ty, offset: u32) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => {
                self.func.instruction(&Instruction::I64Store(MemArg {
                    offset: offset as u64, align: 3, memory_index: 0,
                }));
            }
            Some(ValType::F64) => {
                self.func.instruction(&Instruction::F64Store(MemArg {
                    offset: offset as u64, align: 3, memory_index: 0,
                }));
            }
            Some(ValType::I32) => {
                self.func.instruction(&Instruction::I32Store(MemArg {
                    offset: offset as u64, align: 2, memory_index: 0,
                }));
            }
            _ => {}
        }
    }

    /// Emit a load instruction from base_ptr (on stack) + offset.
    pub fn emit_load_at(&mut self, ty: &Ty, offset: u32) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => {
                self.func.instruction(&Instruction::I64Load(MemArg {
                    offset: offset as u64, align: 3, memory_index: 0,
                }));
            }
            Some(ValType::F64) => {
                self.func.instruction(&Instruction::F64Load(MemArg {
                    offset: offset as u64, align: 3, memory_index: 0,
                }));
            }
            Some(ValType::I32) => {
                self.func.instruction(&Instruction::I32Load(MemArg {
                    offset: offset as u64, align: 2, memory_index: 0,
                }));
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
                        // Substitute type parameters: T → type_args[0], U → type_args[1], etc.
                        // Generic params are typically single-letter names (T, U, A, B)
                        let generic_names = ["T", "U", "A", "B", "K", "V"];
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
