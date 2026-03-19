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

            // ── Match ──
            IrExprKind::Match { subject, arms } => {
                self.emit_match(subject, arms, &expr.ty);
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

            // Constructor pattern (e.g., Some(x), None)
            IrPattern::Constructor { name: _, args: _ } => {
                // Phase 2+: needs variant memory layout
                // For now, handle Option None/Some as special cases
                if is_last {
                    self.emit_expr(&arm.body);
                } else {
                    self.func.instruction(&Instruction::Unreachable);
                }
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

            // Catch-all for unsupported patterns
            _ => {
                if is_last {
                    // Treat as wildcard
                    self.emit_expr(&arm.body);
                } else {
                    self.func.instruction(&Instruction::Unreachable);
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum CmpKind {
    Lt,
    Gt,
    Lte,
    Gte,
}
