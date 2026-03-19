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

            // ── For-in loop (Range only in Phase 2) ──
            IrExprKind::ForIn { var, iterable, body, .. } => {
                self.emit_for_in(*var, iterable, body);
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
    fn emit_load_at(&mut self, ty: &Ty, offset: u32) {
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
    fn emit_for_in(&mut self, var: crate::ir::VarId, iterable: &IrExpr, body: &[crate::ir::IrStmt]) {
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
                // Non-range iterables: Phase 2+ (List, etc.)
                self.func.instruction(&Instruction::Unreachable);
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
