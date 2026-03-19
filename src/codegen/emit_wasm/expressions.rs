//! IrExpr → WASM instruction emission.

use crate::ir::{BinOp, CallTarget, IrExpr, IrExprKind, IrMatchArm, IrPattern, UnOp};
use crate::types::Ty;
use wasm_encoder::{BlockType, Instruction, MemArg, ValType};

use super::FuncCompiler;
use super::values;

fn mem(offset: u64) -> MemArg {
    MemArg { offset, align: 2, memory_index: 0 }
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

            // ── Codegen-specific nodes (pass-through or ignore) ──
            IrExprKind::Clone { expr: inner } | IrExprKind::Deref { expr: inner } => {
                // In WASM, clone/deref are no-ops (no ownership system)
                self.emit_expr(inner);
            }

            // ── Unsupported ──
            _ => {
                self.func.instruction(&Instruction::Unreachable);
            }
        }
    }

    fn emit_binop(&mut self, op: BinOp, left: &IrExpr, right: &IrExpr) {
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

            BinOp::PowFloat => {
                // f64 ** f64: no native WASM instruction, but we can use
                // exp(y * ln(x)) via a simple integer power loop for now
                // For Phase 2: just emit unreachable — will fix later
                // Actually, let's handle integer exponents at least
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

    fn emit_eq(&mut self, left: &IrExpr, right: &IrExpr, negate: bool) {
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
            // Option: both none (0==0) or both some → compare pointed values
            Ty::Applied(crate::types::constructor::TypeConstructorId::Option, args) => {
                let inner_size = args.first().map(|t| values::byte_size(t)).unwrap_or(8);
                self.func.instruction(&Instruction::I32Const(inner_size as i32));
                self.func.instruction(&Instruction::Call(self.emitter.rt.mem_eq));
            }
            // Result: compare tag + value bytes
            Ty::Applied(crate::types::constructor::TypeConstructorId::Result, args) => {
                let ok_size = args.first().map(|t| values::byte_size(t)).unwrap_or(8);
                let err_size = args.get(1).map(|t| values::byte_size(t)).unwrap_or(4);
                let total = 4 + ok_size.max(err_size);
                self.func.instruction(&Instruction::I32Const(total as i32));
                self.func.instruction(&Instruction::Call(self.emitter.rt.mem_eq));
            }
            _ => { self.func.instruction(&Instruction::I32Eq); }
        }
        if negate {
            self.func.instruction(&Instruction::I32Eqz);
        }
    }

    fn emit_cmp_instruction(&mut self, ty: &Ty, kind: CmpKind) {
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

    fn emit_call(&mut self, target: &CallTarget, args: &[IrExpr], _ret_ty: &Ty) {
        match target {
            CallTarget::Named { name } => {
                match name.as_str() {
                    "println" => {
                        let arg = &args[0];
                        match &arg.ty {
                            Ty::String => {
                                self.emit_expr(arg);
                                self.func.instruction(&Instruction::Call(self.emitter.rt.println_str));
                            }
                            Ty::Int => {
                                self.emit_expr(arg);
                                self.func.instruction(&Instruction::Call(self.emitter.rt.println_int));
                            }
                            Ty::Bool => {
                                // Convert bool to "true"/"false"
                                self.emit_expr(arg);
                                let true_str = self.emitter.intern_string("true");
                                let false_str = self.emitter.intern_string("false");
                                self.func.instruction(&Instruction::If(BlockType::Result(wasm_encoder::ValType::I32)));
                                self.func.instruction(&Instruction::I32Const(true_str as i32));
                                self.func.instruction(&Instruction::Else);
                                self.func.instruction(&Instruction::I32Const(false_str as i32));
                                self.func.instruction(&Instruction::End);
                                self.func.instruction(&Instruction::Call(self.emitter.rt.println_str));
                            }
                            Ty::Float => {
                                // Phase 1: print float as int (truncated)
                                self.emit_expr(arg);
                                self.func.instruction(&Instruction::I64TruncF64S);
                                self.func.instruction(&Instruction::Call(self.emitter.rt.println_int));
                            }
                            _ => {
                                // Unsupported type: skip arg and print "<unsupported>"
                                let s = self.emitter.intern_string("<unsupported>");
                                self.func.instruction(&Instruction::I32Const(s as i32));
                                self.func.instruction(&Instruction::Call(self.emitter.rt.println_str));
                            }
                        }
                    }
                    "assert_eq" => {
                        self.emit_assert_eq(&args[0], &args[1]);
                    }
                    "assert" => {
                        // assert(cond) — trap if false
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Eqz);
                        self.func.instruction(&Instruction::If(BlockType::Empty));
                        self.func.instruction(&Instruction::Unreachable);
                        self.func.instruction(&Instruction::End);
                    }
                    "assert_ne" => {
                        // assert_ne(left, right) — trap if equal
                        self.emit_eq(&args[0], &args[1], false);
                        // If equal → trap
                        self.func.instruction(&Instruction::If(BlockType::Empty));
                        self.func.instruction(&Instruction::Unreachable);
                        self.func.instruction(&Instruction::End);
                    }
                    _ => {
                        // Check if this is a unit variant constructor (e.g., Red, None)
                        if args.is_empty() {
                            if let Some(tag) = self.find_unit_variant_tag(name) {
                                // Allocate [tag:i32]
                                self.func.instruction(&Instruction::I32Const(4));
                                self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
                                let scratch = self.match_i32_base + self.match_depth;
                                self.func.instruction(&Instruction::LocalSet(scratch));
                                self.func.instruction(&Instruction::LocalGet(scratch));
                                self.func.instruction(&Instruction::I32Const(tag as i32));
                                self.func.instruction(&Instruction::I32Store(MemArg {
                                    offset: 0, align: 2, memory_index: 0,
                                }));
                                self.func.instruction(&Instruction::LocalGet(scratch));
                                return;
                            }
                        }
                        // User-defined function call
                        for arg in args {
                            self.emit_expr(arg);
                        }
                        if let Some(&func_idx) = self.emitter.func_map.get(name.as_str()) {
                            self.func.instruction(&Instruction::Call(func_idx));
                        } else {
                            self.func.instruction(&Instruction::Unreachable);
                        }
                    }
                }
            }

            CallTarget::Module { module, func } => {
                match (module.as_str(), func.as_str()) {
                    ("int", "to_string") => {
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.int_to_string));
                    }
                    ("float", "to_string") => {
                        // Phase 1: truncate to int, then int_to_string
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I64TruncF64S);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.int_to_string));
                    }
                    ("string", "length") | ("string", "len") => {
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I64ExtendI32U);
                    }
                    ("int", "parse") => {
                        // Stub: return 0 for now
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::Drop);
                        self.func.instruction(&Instruction::I64Const(0));
                    }
                    ("string", "contains") => {
                        // string.contains(haystack, needle) -> bool
                        // Brute force: O(n*m) substring search
                        self.emit_expr(&args[0]); // haystack ptr
                        self.emit_expr(&args[1]); // needle ptr
                        self.func.instruction(&Instruction::Call(self.emitter.rt.str_contains));
                    }
                    ("string", "trim") => {
                        // Stub: return the string as-is (no whitespace handling)
                        self.emit_expr(&args[0]);
                    }
                    ("string", "to_upper") | ("string", "to_lower") => {
                        // Stub: return as-is
                        self.emit_expr(&args[0]);
                    }
                    ("string", "repeat") | ("string", "reverse") | ("string", "replace")
                    | ("string", "split") | ("string", "join") | ("string", "slice")
                    | ("string", "get") | ("string", "count") | ("string", "starts_with")
                    | ("string", "ends_with") | ("string", "index_of") => {
                        // Stub: emit args and return first arg or default
                        for arg in args { self.emit_expr(arg); }
                        for _ in 1..args.len() { self.func.instruction(&Instruction::Drop); }
                    }
                    ("list", "map") | ("list", "filter") | ("list", "fold")
                    | ("list", "find") | ("list", "any") | ("list", "all")
                    | ("list", "count") | ("list", "sort_by") | ("list", "flat_map")
                    | ("list", "filter_map") | ("list", "get") | ("list", "drop")
                    | ("list", "take") | ("list", "reverse") | ("list", "zip")
                    | ("list", "enumerate") | ("list", "contains") | ("list", "sort") => {
                        // Stub: emit args and unreachable (stdlib not yet implemented)
                        for arg in args { self.emit_expr(arg); }
                        for _ in 0..args.len() { self.func.instruction(&Instruction::Drop); }
                        self.func.instruction(&Instruction::Unreachable);
                    }
                    ("map", "len") | ("map", "length") | ("map", "size") => {
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I64ExtendI32U);
                    }
                    ("list", "len") | ("list", "length") => {
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I64ExtendI32U);
                    }
                    _ => {
                        // Unknown module call
                        for arg in args {
                            self.emit_expr(arg);
                        }
                        self.func.instruction(&Instruction::Unreachable);
                    }
                }
            }

            CallTarget::Method { object, method } => {
                // UFCS method calls: obj.method(args)
                match method.as_str() {
                    "to_string" if matches!(object.ty, Ty::Int) => {
                        self.emit_expr(object);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.int_to_string));
                    }
                    "len" | "length" | "string.len" | "list.len" | "map.len" => {
                        // .len() for String, List, Map — all store length at offset 0
                        self.emit_expr(object);
                        self.func.instruction(&Instruction::I32Load(mem(0)));
                        self.func.instruction(&Instruction::I64ExtendI32U);
                    }
                    "to_string" if matches!(object.ty, Ty::Float) => {
                        self.emit_expr(object);
                        self.func.instruction(&Instruction::I64TruncF64S);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.int_to_string));
                    }
                    "contains" if matches!(object.ty, Ty::String) => {
                        self.emit_expr(object);
                        self.emit_expr(&args[0]);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.str_contains));
                    }
                    _ => {
                        self.emit_expr(object);
                        for arg in args {
                            self.emit_expr(arg);
                        }
                        self.func.instruction(&Instruction::Unreachable);
                    }
                }
            }

            CallTarget::Computed { callee } => {
                // Closure call: callee is a closure ptr [table_idx: i32][env_ptr: i32]
                // Stack order for call_indirect: [env_ptr, args..., table_idx]
                let scratch = self.match_i32_base + self.match_depth;

                // Evaluate callee → closure ptr
                self.emit_expr(callee);
                self.func.instruction(&Instruction::LocalSet(scratch));

                // Push env_ptr (first hidden arg)
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Load(MemArg {
                    offset: 4, align: 2, memory_index: 0,
                }));

                // Push declared args
                for arg in args {
                    self.emit_expr(arg);
                }

                // Push table_idx (on top of stack for call_indirect)
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Load(MemArg {
                    offset: 0, align: 2, memory_index: 0,
                }));

                // Closure calling convention type: (env: i32, params...) -> ret
                if let Ty::Fn { params, ret } = &callee.ty {
                    let mut closure_params = vec![ValType::I32]; // env_ptr
                    for p in params {
                        if let Some(vt) = values::ty_to_valtype(p) {
                            closure_params.push(vt);
                        }
                    }
                    let ret_types = values::ret_type(ret);
                    let type_idx = self.emitter.register_type(closure_params, ret_types);
                    self.func.instruction(&Instruction::CallIndirect {
                        type_index: type_idx,
                        table_index: 0,
                    });
                } else {
                    self.func.instruction(&Instruction::Unreachable);
                }
            }
        }
    }

    /// Emit a FnRef as a closure: allocate [wrapper_table_idx, 0] on heap.
    fn emit_fn_ref_closure(&mut self, name: &str) {
        if let Some(&wrapper_table_idx) = self.emitter.fn_ref_wrappers.get(name) {
            // Allocate closure: [table_idx: i32][env_ptr: i32] = 8 bytes
            self.func.instruction(&Instruction::I32Const(8));
            self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
            let scratch = self.match_i32_base + self.match_depth;
            self.func.instruction(&Instruction::LocalSet(scratch));

            // Store table_idx
            self.func.instruction(&Instruction::LocalGet(scratch));
            self.func.instruction(&Instruction::I32Const(wrapper_table_idx as i32));
            self.func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

            // Store env_ptr = 0
            self.func.instruction(&Instruction::LocalGet(scratch));
            self.func.instruction(&Instruction::I32Const(0));
            self.func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

            // Return closure ptr
            self.func.instruction(&Instruction::LocalGet(scratch));
        } else {
            eprintln!("WARNING: FnRef wrapper not found for '{}', using direct table entry", name);
            self.func.instruction(&Instruction::Unreachable);
        }
    }

    /// Emit a lambda as a closure: allocate env + closure on heap.
    fn emit_lambda_closure(&mut self, _params: &[(crate::ir::VarId, Ty)], _body: &IrExpr) {
        let lambda_idx = self.emitter.lambda_counter.get();
        self.emitter.lambda_counter.set(lambda_idx + 1);

        if lambda_idx >= self.emitter.lambdas.len() {
            self.func.instruction(&Instruction::Unreachable);
            return;
        }

        let table_idx = self.emitter.lambdas[lambda_idx].table_idx;
        let captures = self.emitter.lambdas[lambda_idx].captures.clone();

        let scratch = self.match_i32_base + self.match_depth;

        if captures.is_empty() {
            // No captures: allocate closure [table_idx, 0]
            self.func.instruction(&Instruction::I32Const(8));
            self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
            self.func.instruction(&Instruction::LocalSet(scratch));

            self.func.instruction(&Instruction::LocalGet(scratch));
            self.func.instruction(&Instruction::I32Const(table_idx as i32));
            self.func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

            self.func.instruction(&Instruction::LocalGet(scratch));
            self.func.instruction(&Instruction::I32Const(0));
            self.func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

            self.func.instruction(&Instruction::LocalGet(scratch));
        } else {
            // Allocate env: each capture gets 8 bytes (padded for alignment)
            let env_size = (captures.len() as u32) * 8;
            self.func.instruction(&Instruction::I32Const(env_size as i32));
            self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
            let env_scratch = scratch; // reuse for env_ptr
            self.func.instruction(&Instruction::LocalSet(env_scratch));

            // Store each captured variable into env
            for (ci, (vid, ty)) in captures.iter().enumerate() {
                let offset = (ci as u32) * 8;
                self.func.instruction(&Instruction::LocalGet(env_scratch));
                if let Some(&local_idx) = self.var_map.get(&vid.0) {
                    self.func.instruction(&Instruction::LocalGet(local_idx));
                } else {
                    // Variable not in scope — shouldn't happen
                    self.func.instruction(&Instruction::I32Const(0));
                }
                self.emit_store_at(ty, offset);
            }

            // Allocate closure: [table_idx, env_ptr]
            self.func.instruction(&Instruction::I32Const(8));
            self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
            let closure_scratch = scratch + 1; // second i32 scratch slot
            self.func.instruction(&Instruction::LocalSet(closure_scratch));

            self.func.instruction(&Instruction::LocalGet(closure_scratch));
            self.func.instruction(&Instruction::I32Const(table_idx as i32));
            self.func.instruction(&Instruction::I32Store(MemArg { offset: 0, align: 2, memory_index: 0 }));

            self.func.instruction(&Instruction::LocalGet(closure_scratch));
            self.func.instruction(&Instruction::LocalGet(env_scratch));
            self.func.instruction(&Instruction::I32Store(MemArg { offset: 4, align: 2, memory_index: 0 }));

            self.func.instruction(&Instruction::LocalGet(closure_scratch));
        }
    }

    /// Emit assert_eq(left, right): compare values, trap if not equal.
    fn emit_assert_eq(&mut self, left: &IrExpr, right: &IrExpr) {
        // Use the same equality logic as BinOp::Eq
        self.emit_eq(left, right, false);
        // If not equal (result == 0), trap
        self.func.instruction(&Instruction::I32Eqz);
        self.func.instruction(&Instruction::If(BlockType::Empty));
        self.func.instruction(&Instruction::Unreachable);
        self.func.instruction(&Instruction::End);
    }

    /// Concatenate two strings on the heap via __concat_str runtime.
    fn emit_concat_str(&mut self, left: &IrExpr, right: &IrExpr) {
        self.emit_expr(left);
        self.emit_expr(right);
        self.func.instruction(&Instruction::Call(self.emitter.rt.concat_str));
    }

    /// String interpolation: convert each part to string, then concat.
    fn emit_string_interp(&mut self, parts: &[crate::ir::IrStringPart]) {
        if parts.is_empty() {
            let empty = self.emitter.intern_string("");
            self.func.instruction(&Instruction::I32Const(empty as i32));
            return;
        }

        // Emit first part as a string
        self.emit_string_part(&parts[0]);

        // For each subsequent part: emit it, then concat with accumulator
        for part in &parts[1..] {
            self.emit_string_part(part);
            self.func.instruction(&Instruction::Call(self.emitter.rt.concat_str));
        }
    }

    /// Emit a single string interpolation part as a string (i32 pointer).
    fn emit_string_part(&mut self, part: &crate::ir::IrStringPart) {
        match part {
            crate::ir::IrStringPart::Lit { value } => {
                let offset = self.emitter.intern_string(value);
                self.func.instruction(&Instruction::I32Const(offset as i32));
            }
            crate::ir::IrStringPart::Expr { expr } => {
                match &expr.ty {
                    Ty::String => self.emit_expr(expr),
                    Ty::Int => {
                        self.emit_expr(expr);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.int_to_string));
                    }
                    Ty::Bool => {
                        self.emit_expr(expr);
                        let t = self.emitter.intern_string("true");
                        let f = self.emitter.intern_string("false");
                        self.func.instruction(&Instruction::If(wasm_encoder::BlockType::Result(wasm_encoder::ValType::I32)));
                        self.func.instruction(&Instruction::I32Const(t as i32));
                        self.func.instruction(&Instruction::Else);
                        self.func.instruction(&Instruction::I32Const(f as i32));
                        self.func.instruction(&Instruction::End);
                    }
                    Ty::Float => {
                        self.emit_expr(expr);
                        self.func.instruction(&Instruction::I64TruncF64S);
                        self.func.instruction(&Instruction::Call(self.emitter.rt.int_to_string));
                    }
                    _ => {
                        // Fallback: emit the expression (already a string pointer or unsupported)
                        self.emit_expr(expr);
                    }
                }
            }
        }
    }
    /// Emit a record/variant construction: allocate memory, store fields.
    /// For variants (detected via name + type), prepends a tag i32 before fields.
    fn emit_record(&mut self, name: Option<&str>, fields: &[(String, IrExpr)], result_ty: &Ty) {
        // Check if this is a variant constructor
        let tag = self.resolve_variant_tag(name, result_ty);
        let tag_size: u32 = if tag.is_some() { 4 } else { 0 };

        // Compute field types and total size
        let field_types: Vec<(String, Ty)> = fields.iter()
            .map(|(n, expr)| (n.clone(), expr.ty.clone()))
            .collect();
        let total_size = tag_size + values::record_size(&field_types);

        // Allocate
        self.func.instruction(&Instruction::I32Const(total_size as i32));
        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));

        let scratch = self.match_i32_base + self.match_depth;
        self.func.instruction(&Instruction::LocalSet(scratch));

        // Write tag if variant
        if let Some(tag_val) = tag {
            self.func.instruction(&Instruction::LocalGet(scratch));
            self.func.instruction(&Instruction::I32Const(tag_val as i32));
            self.func.instruction(&Instruction::I32Store(MemArg {
                offset: 0, align: 2, memory_index: 0,
            }));
        }

        // Store each field (offset starts after tag)
        let mut offset = tag_size;
        for (_, field_expr) in fields {
            let field_size = values::byte_size(&field_expr.ty);
            self.func.instruction(&Instruction::LocalGet(scratch));
            self.emit_expr(field_expr);
            self.emit_store_at(&field_expr.ty, offset);
            offset += field_size;
        }

        // Return ptr
        self.func.instruction(&Instruction::LocalGet(scratch));
    }

    /// Look up variant tag for a constructor name within a variant type.
    fn resolve_variant_tag(&self, name: Option<&str>, result_ty: &Ty) -> Option<u32> {
        let ctor_name = name?;
        if let Ty::Named(type_name, _) = result_ty {
            if let Some(cases) = self.emitter.variant_info.get(type_name.as_str()) {
                for case in cases {
                    if case.name == ctor_name {
                        return Some(case.tag);
                    }
                }
            }
        }
        None
    }

    /// Emit a store instruction for a value at base_ptr + offset.
    /// Assumes base_ptr is already on stack, followed by the value.
    fn emit_store_at(&mut self, ty: &Ty, offset: u32) {
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

    /// Emit spread record: copy base, then overwrite specified fields.
    fn emit_spread_record(&mut self, base: &IrExpr, overrides: &[(String, IrExpr)], result_ty: &Ty) {
        let all_fields = self.extract_record_fields(result_ty);
        let tag_offset = self.variant_tag_offset(result_ty);
        let total_size = tag_offset + values::record_size(&all_fields);

        // Allocate new record
        self.func.instruction(&Instruction::I32Const(total_size as i32));
        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
        let result_scratch = self.match_i32_base + self.match_depth;
        self.func.instruction(&Instruction::LocalSet(result_scratch));

        // Evaluate base and store ptr
        self.emit_expr(base);
        let base_scratch = result_scratch + 1;
        self.func.instruction(&Instruction::LocalSet(base_scratch));

        // Copy all bytes from base to result (including tag if variant)
        // Byte-by-byte copy loop
        // Use i64 scratch as counter
        let counter = self.match_i64_base + self.match_depth;
        self.func.instruction(&Instruction::I64Const(0));
        self.func.instruction(&Instruction::LocalSet(counter));
        self.func.instruction(&Instruction::Block(BlockType::Empty));
        self.func.instruction(&Instruction::Loop(BlockType::Empty));
        // break if counter >= total_size
        self.func.instruction(&Instruction::LocalGet(counter));
        self.func.instruction(&Instruction::I64Const(total_size as i64));
        self.func.instruction(&Instruction::I64GeU);
        self.func.instruction(&Instruction::BrIf(1));
        // dst[i] = src[i]
        self.func.instruction(&Instruction::LocalGet(result_scratch));
        self.func.instruction(&Instruction::LocalGet(counter));
        self.func.instruction(&Instruction::I32WrapI64);
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::LocalGet(base_scratch));
        self.func.instruction(&Instruction::LocalGet(counter));
        self.func.instruction(&Instruction::I32WrapI64);
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));
        self.func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));
        // counter++
        self.func.instruction(&Instruction::LocalGet(counter));
        self.func.instruction(&Instruction::I64Const(1));
        self.func.instruction(&Instruction::I64Add);
        self.func.instruction(&Instruction::LocalSet(counter));
        self.func.instruction(&Instruction::Br(0));
        self.func.instruction(&Instruction::End);
        self.func.instruction(&Instruction::End);

        // Overwrite specified fields
        for (field_name, field_expr) in overrides {
            if let Some((offset, _)) = values::field_offset(&all_fields, field_name) {
                let total_offset = tag_offset + offset;
                self.func.instruction(&Instruction::LocalGet(result_scratch));
                self.emit_expr(field_expr);
                self.emit_store_at(&field_expr.ty, total_offset);
            }
        }

        // Return result ptr
        self.func.instruction(&Instruction::LocalGet(result_scratch));
    }

    /// Emit a list literal: allocate [len:i32][elem0][elem1]...
    fn emit_list(&mut self, elements: &[IrExpr], _list_ty: &Ty) {
        let elem_ty = if let Some(first) = elements.first() {
            first.ty.clone()
        } else {
            Ty::Int // empty list fallback
        };
        let elem_size = values::byte_size(&elem_ty);
        let n = elements.len() as u32;
        let total = 4 + n * elem_size;

        self.func.instruction(&Instruction::I32Const(total as i32));
        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));

        let scratch = self.match_i32_base + self.match_depth;
        self.func.instruction(&Instruction::LocalSet(scratch));

        // Store length
        self.func.instruction(&Instruction::LocalGet(scratch));
        self.func.instruction(&Instruction::I32Const(n as i32));
        self.func.instruction(&Instruction::I32Store(MemArg {
            offset: 0, align: 2, memory_index: 0,
        }));

        // Store each element
        for (i, elem) in elements.iter().enumerate() {
            let offset = 4 + (i as u32) * elem_size;
            self.func.instruction(&Instruction::LocalGet(scratch));
            self.emit_expr(elem);
            self.emit_store_at(&elem.ty, offset);
        }

        self.func.instruction(&Instruction::LocalGet(scratch));
    }

    /// Emit index access: list_ptr + 4 + index * elem_size
    fn emit_index_access(&mut self, object: &IrExpr, index: &IrExpr, result_ty: &Ty) {
        let elem_size = values::byte_size(result_ty);

        self.emit_expr(object); // list ptr
        self.func.instruction(&Instruction::I32Const(4)); // skip len
        self.func.instruction(&Instruction::I32Add);

        // Add index * elem_size
        self.emit_expr(index);
        // Index might be i64 (Int), convert to i32
        if matches!(&index.ty, Ty::Int) {
            self.func.instruction(&Instruction::I32WrapI64);
        }
        self.func.instruction(&Instruction::I32Const(elem_size as i32));
        self.func.instruction(&Instruction::I32Mul);
        self.func.instruction(&Instruction::I32Add);

        // Load element
        self.emit_load_at(result_ty, 0);
    }

    /// Emit a tuple construction: allocate memory, store each element sequentially.
    fn emit_tuple(&mut self, elements: &[IrExpr]) {
        let element_types: Vec<(String, Ty)> = elements.iter().enumerate()
            .map(|(i, e)| (format!("_{}", i), e.ty.clone()))
            .collect();
        let total_size = values::record_size(&element_types);

        self.func.instruction(&Instruction::I32Const(total_size as i32));
        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));

        let scratch = self.match_i32_base + self.match_depth;
        self.func.instruction(&Instruction::LocalSet(scratch));

        let mut offset = 0u32;
        for elem in elements {
            let size = values::byte_size(&elem.ty);
            self.func.instruction(&Instruction::LocalGet(scratch));
            self.emit_expr(elem);
            self.emit_store_at(&elem.ty, offset);
            offset += size;
        }

        self.func.instruction(&Instruction::LocalGet(scratch));
    }

    /// Emit a tuple index access: load from tuple pointer + element offset.
    fn emit_tuple_index(&mut self, object: &IrExpr, index: usize, result_ty: &Ty) {
        // Compute offset by summing sizes of elements before `index`
        let offset = if let Ty::Tuple(elem_types) = &object.ty {
            elem_types.iter().take(index).map(|t| values::byte_size(t)).sum::<u32>()
        } else {
            0
        };

        self.emit_expr(object);
        self.emit_load_at(result_ty, offset);
    }

    /// Emit a field access: load from record/variant pointer + field offset.
    fn emit_member(&mut self, object: &IrExpr, field: &str) {
        let fields = self.extract_record_fields(&object.ty);
        // If the object is a variant type, fields start after the tag (offset +4)
        let tag_offset = self.variant_tag_offset(&object.ty);

        self.emit_expr(object); // ptr on stack

        if let Some((field_offset, field_ty)) = values::field_offset(&fields, field) {
            let total_offset = tag_offset + field_offset;
            self.emit_load_at(&field_ty, total_offset);
        } else {
            self.func.instruction(&Instruction::Unreachable);
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
    fn variant_tag_offset(&self, ty: &Ty) -> u32 {
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
    fn extract_record_fields(&self, ty: &Ty) -> Vec<(String, Ty)> {
        match ty {
            Ty::Record { fields } => fields.clone(),
            Ty::Named(name, _) => {
                // Look up the named type in the type declarations
                // For now, search the emitter's stored type info
                if let Some(fields) = self.emitter.record_fields.get(name.as_str()) {
                    fields.clone()
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }

    /// Emit a for...in loop. Currently supports Range iterables only.
    fn emit_for_in(&mut self, var: crate::ir::VarId, var_tuple: Option<&[crate::ir::VarId]>, iterable: &IrExpr, body: &[crate::ir::IrStmt]) {
        match &iterable.kind {
            IrExprKind::Range { start, end, inclusive } => {
                let loop_var = self.var_map[&var.0];

                // Initialize loop variable to start
                self.emit_expr(start);
                self.func.instruction(&Instruction::LocalSet(loop_var));

                // block $break
                let break_depth = self.depth;
                self.func.instruction(&Instruction::Block(BlockType::Empty));
                self.depth += 1;

                // loop $continue
                let continue_depth = self.depth;
                self.func.instruction(&Instruction::Loop(BlockType::Empty));
                self.depth += 1;

                self.loop_stack.push(super::LoopLabels { break_depth, continue_depth });

                // Break condition: if var >= end (exclusive) or var > end (inclusive)
                self.func.instruction(&Instruction::LocalGet(loop_var));
                self.emit_expr(end);
                if *inclusive {
                    self.func.instruction(&Instruction::I64GtS); // var > end → break
                } else {
                    self.func.instruction(&Instruction::I64GeS); // var >= end → break
                }
                self.func.instruction(&Instruction::BrIf(self.depth - break_depth - 1));

                // Body
                for stmt in body {
                    self.emit_stmt(stmt);
                }

                // Increment: var += 1
                self.func.instruction(&Instruction::LocalGet(loop_var));
                self.func.instruction(&Instruction::I64Const(1));
                self.func.instruction(&Instruction::I64Add);
                self.func.instruction(&Instruction::LocalSet(loop_var));

                // Continue
                self.func.instruction(&Instruction::Br(self.depth - continue_depth - 1));

                self.loop_stack.pop();
                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end loop
                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end block
            }
            _ => {
                // List (or other collection) for...in
                // scratch[0] = list ptr, scratch[1] = index counter
                let list_scratch = self.match_i32_base + self.match_depth;
                let idx_scratch = list_scratch + 1;
                let loop_var = self.var_map[&var.0];

                // Determine element type and size
                let elem_ty = self.var_table.get(var).ty.clone();
                let elem_size = values::byte_size(&elem_ty);

                // Evaluate iterable and store list ptr
                self.emit_expr(iterable);
                self.func.instruction(&Instruction::LocalSet(list_scratch));

                // Initialize index = 0
                self.func.instruction(&Instruction::I32Const(0));
                self.func.instruction(&Instruction::LocalSet(idx_scratch));

                // block $break
                let break_depth = self.depth;
                self.func.instruction(&Instruction::Block(BlockType::Empty));
                self.depth += 1;

                // loop $continue
                let continue_depth = self.depth;
                self.func.instruction(&Instruction::Loop(BlockType::Empty));
                self.depth += 1;

                self.loop_stack.push(super::LoopLabels { break_depth, continue_depth });

                // Break if index >= len
                self.func.instruction(&Instruction::LocalGet(idx_scratch));
                self.func.instruction(&Instruction::LocalGet(list_scratch));
                self.func.instruction(&Instruction::I32Load(MemArg {
                    offset: 0, align: 2, memory_index: 0,
                })); // load len
                self.func.instruction(&Instruction::I32GeU);
                self.func.instruction(&Instruction::BrIf(self.depth - break_depth - 1));

                // Load element: var = list[index]
                // address = list_ptr + 4 + index * elem_size
                self.func.instruction(&Instruction::LocalGet(list_scratch));
                self.func.instruction(&Instruction::I32Const(4));
                self.func.instruction(&Instruction::I32Add);
                self.func.instruction(&Instruction::LocalGet(idx_scratch));
                self.func.instruction(&Instruction::I32Const(elem_size as i32));
                self.func.instruction(&Instruction::I32Mul);
                self.func.instruction(&Instruction::I32Add);
                self.emit_load_at(&elem_ty, 0);
                self.func.instruction(&Instruction::LocalSet(loop_var));

                // Tuple destructure: extract fields from loop_var into var_tuple locals
                if let Some(tuple_vars) = var_tuple {
                    if let Ty::Tuple(elem_types) = &elem_ty {
                        let mut field_offset = 0u32;
                        for (i, &tv) in tuple_vars.iter().enumerate() {
                            if let Some(&local_idx) = self.var_map.get(&tv.0) {
                                let ft = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                                self.func.instruction(&Instruction::LocalGet(loop_var));
                                self.emit_load_at(&ft, field_offset);
                                self.func.instruction(&Instruction::LocalSet(local_idx));
                                field_offset += values::byte_size(&ft);
                            }
                        }
                    }
                }

                // Body
                for stmt in body {
                    self.emit_stmt(stmt);
                }

                // Increment index
                self.func.instruction(&Instruction::LocalGet(idx_scratch));
                self.func.instruction(&Instruction::I32Const(1));
                self.func.instruction(&Instruction::I32Add);
                self.func.instruction(&Instruction::LocalSet(idx_scratch));

                // Continue
                self.func.instruction(&Instruction::Br(self.depth - continue_depth - 1));

                self.loop_stack.pop();
                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end loop
                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end block
            }
        }
    }

    /// Emit a match expression as a chain of if-else checks.
    ///
    /// Strategy: store subject in a scratch local, then for each arm:
    /// - Literal pattern: compare subject to literal, branch if equal
    /// - Wildcard: unconditional (last arm)
    /// - Bind: store subject in the bound variable's local, unconditional
    fn emit_match(&mut self, subject: &IrExpr, arms: &[IrMatchArm], result_ty: &Ty) {
        // Determine scratch local for the subject
        let scratch = match values::ty_to_valtype(&subject.ty) {
            Some(ValType::I64) => self.match_i64_base + self.match_depth,
            _ => self.match_i32_base + self.match_depth,
        };
        self.match_depth += 1;

        // Evaluate subject and store in scratch
        self.emit_expr(subject);
        self.func.instruction(&Instruction::LocalSet(scratch));

        // Emit arms as nested if-else
        self.emit_match_arms(arms, scratch, &subject.ty, result_ty, 0);

        self.match_depth -= 1;
    }

    fn emit_match_arms(
        &mut self,
        arms: &[IrMatchArm],
        scratch: u32,
        subject_ty: &Ty,
        result_ty: &Ty,
        idx: usize,
    ) {
        if idx >= arms.len() {
            // No arms matched — should not happen with exhaustive match
            self.func.instruction(&Instruction::Unreachable);
            return;
        }

        let arm = &arms[idx];
        let is_last = idx + 1 >= arms.len();

        match &arm.pattern {
            // Wildcard: always matches, emit body directly
            IrPattern::Wildcard => {
                self.emit_expr(&arm.body);
            }

            // Bind: store subject in variable, then emit body
            IrPattern::Bind { var } => {
                if let Some(&local_idx) = self.var_map.get(&var.0) {
                    self.func.instruction(&Instruction::LocalGet(scratch));
                    self.func.instruction(&Instruction::LocalSet(local_idx));
                }
                self.emit_expr(&arm.body);
            }

            // Literal: compare subject to literal, if-else
            IrPattern::Literal { expr: lit_expr } => {
                // Push subject
                self.func.instruction(&Instruction::LocalGet(scratch));
                // Push literal
                self.emit_expr(lit_expr);
                // Compare
                match subject_ty {
                    Ty::Int => { self.func.instruction(&Instruction::I64Eq); }
                    Ty::Float => { self.func.instruction(&Instruction::F64Eq); }
                    Ty::Bool => { self.func.instruction(&Instruction::I32Eq); }
                    Ty::String => {
                        // String equality: compare pointers (interned literals are deduped)
                        self.func.instruction(&Instruction::I32Eq);
                    }
                    _ => { self.func.instruction(&Instruction::I32Eq); }
                }

                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                self.depth += 1;
                self.emit_expr(&arm.body);
                self.func.instruction(&Instruction::Else);

                if is_last {
                    self.func.instruction(&Instruction::Unreachable);
                } else {
                    self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                }

                self.depth -= 1;
                self.func.instruction(&Instruction::End);
            }

            // Constructor pattern (e.g., Red, Some(x), None)
            IrPattern::Constructor { name: ctor_name, args: _ } => {
                // Look up tag for this constructor
                if let Some(tag_val) = self.find_variant_tag_by_ctor(ctor_name, subject_ty) {
                    // Load tag from subject and compare
                    self.func.instruction(&Instruction::LocalGet(scratch));
                    self.func.instruction(&Instruction::I32Load(MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                    self.func.instruction(&Instruction::I32Const(tag_val as i32));
                    self.func.instruction(&Instruction::I32Eq);

                    let bt = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt));
                    self.depth += 1;
                    self.emit_expr(&arm.body);
                    self.func.instruction(&Instruction::Else);
                    if is_last {
                        self.func.instruction(&Instruction::Unreachable);
                    } else {
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                    }
                    self.depth -= 1;
                    self.func.instruction(&Instruction::End);
                } else if is_last {
                    self.emit_expr(&arm.body);
                } else {
                    self.func.instruction(&Instruction::Unreachable);
                }
            }

            // Some(x) pattern (Option)
            IrPattern::Some { inner } => {
                // some(x) is a non-null pointer. Check ptr != 0, then load value.
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Const(0));
                self.func.instruction(&Instruction::I32Ne);
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                self.depth += 1;

                // Bind the inner value
                if let IrPattern::Bind { var } = inner.as_ref() {
                    if let Some(&local_idx) = self.var_map.get(&var.0) {
                        // Load value from the Some pointer
                        let inner_ty = self.var_table.get(*var).ty.clone();
                        self.func.instruction(&Instruction::LocalGet(scratch));
                        self.emit_load_at(&inner_ty, 0);
                        self.func.instruction(&Instruction::LocalSet(local_idx));
                    }
                }

                self.emit_expr(&arm.body);
                self.func.instruction(&Instruction::Else);
                if is_last {
                    self.func.instruction(&Instruction::Unreachable);
                } else {
                    self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                }
                self.depth -= 1;
                self.func.instruction(&Instruction::End);
            }

            // None pattern (Option)
            IrPattern::None => {
                // None is represented as i32 0
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Eqz);
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                self.depth += 1;
                self.emit_expr(&arm.body);
                self.func.instruction(&Instruction::Else);
                if is_last {
                    self.func.instruction(&Instruction::Unreachable);
                } else {
                    self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                }
                self.depth -= 1;
                self.func.instruction(&Instruction::End);
            }

            // RecordPattern: variant constructor match (e.g., Circle { radius })
            IrPattern::RecordPattern { name: ctor_name, fields: pat_fields, .. } => {
                // Look up the tag for this constructor
                let tag = self.find_variant_tag_by_ctor(ctor_name, subject_ty);

                if let Some(tag_val) = tag {
                    // Load tag from subject pointer
                    self.func.instruction(&Instruction::LocalGet(scratch));
                    self.func.instruction(&Instruction::I32Load(MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                    self.func.instruction(&Instruction::I32Const(tag_val as i32));
                    self.func.instruction(&Instruction::I32Eq);

                    let bt = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt));
                    self.depth += 1;

                    // Bind fields: load each field from subject + tag_offset + field_offset
                    let case_fields = self.emitter.record_fields.get(ctor_name).cloned().unwrap_or_default();
                    for pf in pat_fields {
                        // Find the field in the case's fields
                        if let Some((foff, fty)) = values::field_offset(&case_fields, &pf.name) {
                            let total_offset = 4 + foff; // 4 = tag size
                            // Look up VarId for this field name in var_map
                            // The pattern binds to a var with the same name
                            // We need to find the VarId — it should be in var_map
                            // The IR guarantees pattern fields create bindings in var_table
                            // with the field name. We search by checking all var_map entries.
                            // Actually, the var_table is indexed by VarId and has names.
                            // We need to find the VarId that was allocated for this field name.
                            // The scan_pattern in statements.rs should have registered it.
                            // For now, find the local by searching var_map for the right VarId.

                            // Simple approach: find the VarId from var_map whose name matches
                            // This is set up by scan_pattern which registers field bindings
                            if let Some(&local_idx) = self.find_var_by_field(&pf.name, &case_fields) {
                                self.func.instruction(&Instruction::LocalGet(scratch));
                                self.emit_load_at(&fty, total_offset);
                                self.func.instruction(&Instruction::LocalSet(local_idx));
                            }
                        }
                    }

                    self.emit_expr(&arm.body);
                    self.func.instruction(&Instruction::Else);

                    if is_last {
                        self.func.instruction(&Instruction::Unreachable);
                    } else {
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                    }

                    self.depth -= 1;
                    self.func.instruction(&Instruction::End);
                } else {
                    // Not a variant — treat as plain record (always matches)
                    self.emit_expr(&arm.body);
                }
            }

            // Catch-all for unsupported patterns
            _ => {
                if is_last {
                    self.emit_expr(&arm.body);
                } else {
                    self.func.instruction(&Instruction::Unreachable);
                }
            }
        }
    }

    /// Find the variant tag for a constructor name, searching variant_info by subject type.
    fn find_variant_tag_by_ctor(&self, ctor_name: &str, subject_ty: &Ty) -> Option<u32> {
        let type_name = match subject_ty {
            Ty::Named(name, _) => name.as_str(),
            Ty::Variant { name, .. } => name.as_str(),
            _ => return None,
        };
        let cases = self.emitter.variant_info.get(type_name)?;
        cases.iter().find(|c| c.name == ctor_name).map(|c| c.tag)
    }

    /// Find variant tag for a unit constructor called as a function (e.g., `Red`).
    fn find_unit_variant_tag(&self, name: &str) -> Option<u32> {
        for cases in self.emitter.variant_info.values() {
            for case in cases {
                if case.name == name && case.fields.is_empty() {
                    return Some(case.tag);
                }
            }
        }
        None
    }

    /// Find local index for a pattern field binding by name.
    fn find_var_by_field(&self, field_name: &str, _case_fields: &[(String, Ty)]) -> Option<&u32> {
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

#[derive(Clone, Copy)]
enum CmpKind {
    Lt,
    Gt,
    Lte,
    Gte,
}
