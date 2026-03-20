//! Control flow emission — for-in loops and match expressions.

use crate::ir::{IrExpr, IrExprKind, IrMatchArm, IrPattern, IrStmt};
use crate::types::Ty;
use wasm_encoder::{BlockType, Instruction, MemArg, ValType};

use super::FuncCompiler;
use super::values;

impl FuncCompiler<'_> {
    /// Emit a for...in loop. Currently supports Range iterables only.
    pub(super) fn emit_for_in(&mut self, var: crate::ir::VarId, var_tuple: Option<&[crate::ir::VarId]>, iterable: &IrExpr, body: &[IrStmt]) {
        match &iterable.kind {
            IrExprKind::Range { start, end, inclusive } => {
                let loop_var = self.var_map[&var.0];

                // Initialize loop variable to start
                self.emit_expr(start);
                self.func.instruction(&Instruction::LocalSet(loop_var));

                // block $break { loop $loop { check; block $continue { body }; i++; br $loop } }
                let break_depth = self.depth;
                self.func.instruction(&Instruction::Block(BlockType::Empty));
                self.depth += 1;

                let loop_depth = self.depth;
                self.func.instruction(&Instruction::Loop(BlockType::Empty));
                self.depth += 1;

                // Break condition
                self.func.instruction(&Instruction::LocalGet(loop_var));
                self.emit_expr(end);
                if *inclusive {
                    self.func.instruction(&Instruction::I64GtS);
                } else {
                    self.func.instruction(&Instruction::I64GeS);
                }
                self.func.instruction(&Instruction::BrIf(self.depth - break_depth - 1));

                // Inner block for continue target
                let continue_depth = self.depth;
                self.func.instruction(&Instruction::Block(BlockType::Empty));
                self.depth += 1;

                self.loop_stack.push(super::LoopLabels { break_depth, continue_depth });

                // Body
                for stmt in body {
                    self.emit_stmt(stmt);
                }

                self.loop_stack.pop();
                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end continue block

                // Increment: var += 1 (always runs, even after continue)
                self.func.instruction(&Instruction::LocalGet(loop_var));
                self.func.instruction(&Instruction::I64Const(1));
                self.func.instruction(&Instruction::I64Add);
                self.func.instruction(&Instruction::LocalSet(loop_var));

                // Loop back
                self.func.instruction(&Instruction::Br(self.depth - loop_depth - 1));

                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end loop
                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end break block
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

                // Structure: block $break { loop $loop { check; load; block $continue { body }; i++; br $loop } }
                // continue → br to $continue end (skips rest of body, runs i++)
                let break_depth = self.depth;
                self.func.instruction(&Instruction::Block(BlockType::Empty));
                self.depth += 1;

                let loop_depth = self.depth;
                self.func.instruction(&Instruction::Loop(BlockType::Empty));
                self.depth += 1;

                // Break if index >= len
                self.func.instruction(&Instruction::LocalGet(idx_scratch));
                self.func.instruction(&Instruction::LocalGet(list_scratch));
                self.func.instruction(&Instruction::I32Load(MemArg {
                    offset: 0, align: 2, memory_index: 0,
                }));
                self.func.instruction(&Instruction::I32GeU);
                self.func.instruction(&Instruction::BrIf(self.depth - break_depth - 1));

                // Load element
                self.func.instruction(&Instruction::LocalGet(list_scratch));
                self.func.instruction(&Instruction::I32Const(4));
                self.func.instruction(&Instruction::I32Add);
                self.func.instruction(&Instruction::LocalGet(idx_scratch));
                self.func.instruction(&Instruction::I32Const(elem_size as i32));
                self.func.instruction(&Instruction::I32Mul);
                self.func.instruction(&Instruction::I32Add);
                self.emit_load_at(&elem_ty, 0);
                self.func.instruction(&Instruction::LocalSet(loop_var));

                // Tuple destructure
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

                // Inner block for continue target
                let continue_depth = self.depth;
                self.func.instruction(&Instruction::Block(BlockType::Empty));
                self.depth += 1;

                self.loop_stack.push(super::LoopLabels { break_depth, continue_depth });

                // Body
                for stmt in body {
                    self.emit_stmt(stmt);
                }

                self.loop_stack.pop();
                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end continue block

                // Increment index (always runs, even after continue)
                self.func.instruction(&Instruction::LocalGet(idx_scratch));
                self.func.instruction(&Instruction::I32Const(1));
                self.func.instruction(&Instruction::I32Add);
                self.func.instruction(&Instruction::LocalSet(idx_scratch));

                // Loop back
                self.func.instruction(&Instruction::Br(self.depth - loop_depth - 1));

                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end loop
                self.depth -= 1;
                self.func.instruction(&Instruction::End); // end break block
            }
        }
    }

    /// Emit a match expression as a chain of if-else checks.
    ///
    /// Strategy: store subject in a scratch local, then for each arm:
    /// - Literal pattern: compare subject to literal, branch if equal
    /// - Wildcard: unconditional (last arm)
    /// - Bind: store subject in the bound variable's local, unconditional
    pub(super) fn emit_match(&mut self, subject: &IrExpr, arms: &[IrMatchArm], result_ty: &Ty) {
        // Resolve subject type — IR may have wrong type on Var nodes (type inference gap)
        let subject_ty = self.resolve_subject_type(subject, arms);

        // Evaluate subject BEFORE incrementing depth (subject may use scratch too)
        self.emit_expr(subject);

        let scratch = match values::ty_to_valtype(&subject_ty) {
            Some(ValType::I64) => self.match_i64_base + self.match_depth,
            _ => self.match_i32_base + self.match_depth,
        };
        self.match_depth += 1;

        self.func.instruction(&Instruction::LocalSet(scratch));

        self.emit_match_arms(arms, scratch, &subject_ty, result_ty, 0);

        self.match_depth -= 1;
    }

    /// Resolve the actual subject type, fixing IR type inference gaps.
    /// If the subject Var has the wrong type but patterns indicate a container type, fix it.
    pub(super) fn resolve_subject_type(&self, subject: &IrExpr, arms: &[IrMatchArm]) -> Ty {
        let ty = &subject.ty;
        // If patterns are Ok/Err/Some/None but subject type isn't a container, look up VarTable
        let has_container_pattern = arms.iter().any(|a| matches!(
            &a.pattern,
            IrPattern::Ok { .. } | IrPattern::Err { .. } | IrPattern::Some { .. } | IrPattern::None
        ));
        if has_container_pattern && !matches!(ty, Ty::Applied(_, _)) {
            // Try to get the real type from VarTable
            if let IrExprKind::Var { id } = &subject.kind {
                let info = self.var_table.get(*id);
                if matches!(&info.ty, Ty::Applied(_, _)) {
                    return info.ty.clone();
                }
            }
        }
        ty.clone()
    }

    pub(super) fn emit_match_arms(
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
                    let var_ty = &self.var_table.get(*var).ty;
                    let var_vt = values::ty_to_valtype(var_ty);
                    let subj_vt = values::ty_to_valtype(subject_ty);
                    // Only bind if types match, or var type is Unknown (trust subject)
                    if var_vt == subj_vt || matches!(var_ty, Ty::Unknown) {
                        self.func.instruction(&Instruction::LocalGet(scratch));
                        self.func.instruction(&Instruction::LocalSet(local_idx));
                    }
                }
                // Handle guard condition
                if let Some(guard) = &arm.guard {
                    self.emit_expr(guard);
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
                } else {
                    self.emit_expr(&arm.body);
                }
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

            // Constructor pattern (e.g., Circle(r), Red)
            IrPattern::Constructor { name: ctor_name, args } => {
                if let Some(tag_val) = self.find_variant_tag_by_ctor(ctor_name, subject_ty) {
                    self.func.instruction(&Instruction::LocalGet(scratch));
                    self.func.instruction(&Instruction::I32Load(MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                    self.func.instruction(&Instruction::I32Const(tag_val as i32));
                    self.func.instruction(&Instruction::I32Eq);

                    let bt = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt));
                    self.depth += 1;

                    // Bind constructor args (tuple payload fields)
                    let mut field_offset = 4u32; // skip tag
                    for arg_pat in args {
                        if let IrPattern::Bind { var } = arg_pat {
                            if let Some(&local_idx) = self.var_map.get(&var.0) {
                                let var_ty = self.var_table.get(*var).ty.clone();
                                self.func.instruction(&Instruction::LocalGet(scratch));
                                self.emit_load_at(&var_ty, field_offset);
                                self.func.instruction(&Instruction::LocalSet(local_idx));
                                field_offset += values::byte_size(&var_ty);
                            }
                        } else if let IrPattern::Wildcard = arg_pat {
                            // Skip wildcard — still advance offset
                            // Need to know the type... use i64 (8 bytes) as default
                            field_offset += 8;
                        }
                    }

                    // Handle guard on constructor
                    if let Some(guard) = &arm.guard {
                        self.emit_expr(guard);
                        let bt2 = values::block_type(result_ty);
                        self.func.instruction(&Instruction::If(bt2));
                        self.depth += 1;
                        self.emit_expr(&arm.body);
                        self.func.instruction(&Instruction::Else);
                        if is_last { self.func.instruction(&Instruction::Unreachable); }
                        else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                        self.depth -= 1;
                        self.func.instruction(&Instruction::End);
                    } else {
                        self.emit_expr(&arm.body);
                    }
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
                        let inner_ty = if let Ty::Applied(_, args) = subject_ty {
                            args.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        self.func.instruction(&Instruction::LocalGet(scratch));
                        self.emit_load_at(&inner_ty, 0);
                        self.func.instruction(&Instruction::LocalSet(local_idx));
                    }
                }

                // Handle guard
                if let Some(guard) = &arm.guard {
                    self.emit_expr(guard);
                    let bt2 = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt2));
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
                } else {
                    self.emit_expr(&arm.body);
                }

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

            // Ok(x) pattern (Result)
            IrPattern::Ok { inner } => {
                // Result ok = tag 0. Check tag, then bind value.
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
                self.func.instruction(&Instruction::I32Eqz); // tag == 0
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                self.depth += 1;
                if let IrPattern::Bind { var } = inner.as_ref() {
                    if let Some(&local_idx) = self.var_map.get(&var.0) {
                        let inner_ty = if let Ty::Applied(_, args) = subject_ty {
                            args.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        self.func.instruction(&Instruction::LocalGet(scratch));
                        self.emit_load_at(&inner_ty, 4);
                        self.func.instruction(&Instruction::LocalSet(local_idx));
                    }
                }
                if let Some(guard) = &arm.guard {
                    self.emit_expr(guard);
                    let bt2 = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt2));
                    self.depth += 1;
                    self.emit_expr(&arm.body);
                    self.func.instruction(&Instruction::Else);
                    if is_last { self.func.instruction(&Instruction::Unreachable); }
                    else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                    self.depth -= 1;
                    self.func.instruction(&Instruction::End);
                } else {
                    self.emit_expr(&arm.body);
                }
                self.func.instruction(&Instruction::Else);
                if is_last { self.func.instruction(&Instruction::Unreachable); }
                else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                self.depth -= 1;
                self.func.instruction(&Instruction::End);
            }

            // Err(e) pattern (Result)
            IrPattern::Err { inner } => {
                self.func.instruction(&Instruction::LocalGet(scratch));
                self.func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
                self.func.instruction(&Instruction::I32Const(0));
                self.func.instruction(&Instruction::I32Ne);
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                self.depth += 1;
                if let IrPattern::Bind { var } = inner.as_ref() {
                    if let Some(&local_idx) = self.var_map.get(&var.0) {
                        let inner_ty = if let Ty::Applied(_, args) = subject_ty {
                            args.get(1).cloned().unwrap_or(Ty::String)
                        } else { Ty::String };
                        self.func.instruction(&Instruction::LocalGet(scratch));
                        self.emit_load_at(&inner_ty, 4);
                        self.func.instruction(&Instruction::LocalSet(local_idx));
                    }
                }
                if let Some(guard) = &arm.guard {
                    self.emit_expr(guard);
                    let bt2 = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt2));
                    self.depth += 1;
                    self.emit_expr(&arm.body);
                    self.func.instruction(&Instruction::Else);
                    if is_last { self.func.instruction(&Instruction::Unreachable); }
                    else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                    self.depth -= 1;
                    self.func.instruction(&Instruction::End);
                } else {
                    self.emit_expr(&arm.body);
                }
                self.func.instruction(&Instruction::Else);
                if is_last { self.func.instruction(&Instruction::Unreachable); }
                else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                self.depth -= 1;
                self.func.instruction(&Instruction::End);
            }

            // Tuple pattern: (a, b) => ...
            IrPattern::Tuple { elements } => {
                // Tuple always matches (destructure only). Bind each element.
                if let Ty::Tuple(elem_types) = subject_ty {
                    let mut offset = 0u32;
                    for (i, elem_pat) in elements.iter().enumerate() {
                        if let IrPattern::Bind { var } = elem_pat {
                            if let Some(&local_idx) = self.var_map.get(&var.0) {
                                let ft = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                                self.func.instruction(&Instruction::LocalGet(scratch));
                                self.emit_load_at(&ft, offset);
                                self.func.instruction(&Instruction::LocalSet(local_idx));
                                offset += values::byte_size(&ft);
                            }
                        } else if let IrPattern::Wildcard = elem_pat {
                            let ft = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                            offset += values::byte_size(&ft);
                        }
                    }
                }
                self.emit_expr(&arm.body);
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
}
