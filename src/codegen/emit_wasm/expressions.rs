//! IrExpr → WASM instruction emission.

use crate::ir::{BinOp, CallTarget, IrExpr, IrExprKind, UnOp};
use crate::types::Ty;
use wasm_encoder::{BlockType, Instruction, MemArg};

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
                let local_idx = self.var_map[&id.0];
                self.func.instruction(&Instruction::LocalGet(local_idx));
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
                // do block with guards compiles like a loop that runs once
                // (guards break out early)
                let break_depth = self.depth;
                self.func.instruction(&Instruction::Block(values::block_type(&expr.ty)));
                self.depth += 1;

                self.loop_stack.push(super::LoopLabels {
                    break_depth,
                    continue_depth: break_depth, // no continue in do blocks
                });

                for stmt in stmts {
                    self.emit_stmt(stmt);
                }
                if let Some(e) = tail {
                    self.emit_expr(e);
                }

                self.loop_stack.pop();
                self.depth -= 1;
                self.func.instruction(&Instruction::End);
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

            // ── Codegen-specific nodes (pass-through or ignore) ──
            IrExprKind::Clone { expr: inner } | IrExprKind::Deref { expr: inner } => {
                // In WASM, clone/deref are no-ops (no ownership system)
                self.emit_expr(inner);
            }

            // ── Unsupported in Phase 1 ──
            _ => {
                // Emit unreachable for unimplemented features
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

            // ── Phase 2+ ──
            BinOp::PowFloat | BinOp::XorInt | BinOp::ConcatList => {
                self.func.instruction(&Instruction::Unreachable);
            }
        }
    }

    fn emit_eq(&mut self, left: &IrExpr, right: &IrExpr, negate: bool) {
        self.emit_expr(left);
        self.emit_expr(right);
        match &left.ty {
            Ty::Int => self.func.instruction(&Instruction::I64Eq),
            Ty::Float => self.func.instruction(&Instruction::F64Eq),
            Ty::Bool => self.func.instruction(&Instruction::I32Eq),
            _ => self.func.instruction(&Instruction::I32Eq), // pointer equality fallback
        };
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
                    _ => {
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
                    ("string", "length") => {
                        // Load length from string pointer
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
                // Phase 2+: indirect calls via funcref
                self.emit_expr(callee);
                for arg in args {
                    self.emit_expr(arg);
                }
                self.func.instruction(&Instruction::Unreachable);
            }
        }
    }

    /// Concatenate two strings on the heap.
    fn emit_concat_str(&mut self, left: &IrExpr, right: &IrExpr) {
        // left_ptr, right_ptr on stack
        // new_len = left.len + right.len
        // result = alloc(4 + new_len)
        // mem32[result] = new_len
        // memcpy result+4, left+4, left.len
        // memcpy result+4+left.len, right+4, right.len

        // We need locals to hold intermediate values.
        // For Phase 1, use a simplified approach: store ptrs in scratch area.
        // Actually, we'll use the emitter's alloc and manual copy.
        // This is complex; for now emit unreachable as a placeholder.
        // TODO: implement string concatenation
        self.emit_expr(left);
        self.func.instruction(&Instruction::Drop);
        self.emit_expr(right);
        self.func.instruction(&Instruction::Drop);
        self.func.instruction(&Instruction::Unreachable);
    }

    /// String interpolation: concatenate parts.
    fn emit_string_interp(&mut self, parts: &[crate::ir::IrStringPart]) {
        // Phase 1: simplified — emit unreachable
        // Phase 2: allocate buffer, write each part
        let _ = parts;
        self.func.instruction(&Instruction::Unreachable);
    }
}

#[derive(Clone, Copy)]
enum CmpKind {
    Lt,
    Gt,
    Lte,
    Gte,
}
