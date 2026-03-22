//! Control flow emission — for-in loops and match expressions.

use crate::ir::{IrExpr, IrExprKind, IrMatchArm, IrPattern, IrStmt};
use crate::types::Ty;
use wasm_encoder::{Instruction, ValType};

use super::FuncCompiler;
use super::values;
use super::wasm_macro::wasm;

fn has_typevar_in_ty(ty: &Ty) -> bool {
    ty.any_child_recursive(&|t| {
        matches!(t, Ty::TypeVar(_))
            || matches!(t, Ty::Named(n, args) if args.is_empty() && n.len() <= 2 && n.chars().next().map_or(false, |c| c.is_uppercase()))
    })
}

impl FuncCompiler<'_> {
    /// Emit a for...in loop. Currently supports Range iterables only.
    pub(super) fn emit_for_in(&mut self, var: crate::ir::VarId, var_tuple: Option<&[crate::ir::VarId]>, iterable: &IrExpr, body: &[IrStmt]) {
        match &iterable.kind {
            IrExprKind::Range { start, end, inclusive } => {
                let loop_var = self.var_map[&var.0];

                // Initialize loop variable to start
                self.emit_expr(start);
                wasm!(self.func, { local_set(loop_var); });

                // block $break { loop $loop { check; block $continue { body }; i++; br $loop } }
                let break_depth = self.depth;
                wasm!(self.func, { block_empty; });
                self.depth += 1;

                let loop_depth = self.depth;
                wasm!(self.func, { loop_empty; });
                self.depth += 1;

                // Break condition
                wasm!(self.func, { local_get(loop_var); });
                self.emit_expr(end);
                if *inclusive {
                    wasm!(self.func, { i64_gt_s; });
                } else {
                    wasm!(self.func, { i64_ge_s; });
                }
                wasm!(self.func, { br_if(self.depth - break_depth - 1); });

                // Inner block for continue target
                let continue_depth = self.depth;
                wasm!(self.func, { block_empty; });
                self.depth += 1;

                self.loop_stack.push(super::LoopLabels { break_depth, continue_depth });

                // Body
                for stmt in body {
                    self.emit_stmt(stmt);
                }

                self.loop_stack.pop();
                self.depth -= 1;
                wasm!(self.func, { end; }); // end continue block

                // Increment: var += 1 (always runs, even after continue)
                wasm!(self.func, {
                    local_get(loop_var);
                    i64_const(1);
                    i64_add;
                    local_set(loop_var);
                });

                // Loop back
                wasm!(self.func, { br(self.depth - loop_depth - 1); });

                self.depth -= 1;
                wasm!(self.func, { end; }); // end loop
                self.depth -= 1;
                wasm!(self.func, { end; }); // end break block
            }
            _ => {
                // Detect Map iterable: layout is [len:i32][key0][val0][key1][val1]...
                // where entries are stored inline (not as tuple pointers).
                let is_map = matches!(
                    &iterable.ty,
                    Ty::Applied(crate::types::TypeConstructorId::Map, _)
                );

                // scratch locals: collection ptr + index counter
                let list_scratch = self.scratch.alloc_i32();
                let idx_scratch = self.scratch.alloc_i32();
                let loop_var = self.var_map[&var.0];

                // Determine element type and entry stride
                let elem_ty = self.var_table.get(var).ty.clone();
                let entry_size = if is_map {
                    // Map entries are inline [key][val], not tuple pointers.
                    // Compute stride from key + value types.
                    if let Ty::Applied(_, args) = &iterable.ty {
                        let key_size = args.first().map(|t| values::byte_size(t)).unwrap_or(4);
                        let val_size = args.get(1).map(|t| values::byte_size(t)).unwrap_or(4);
                        key_size + val_size
                    } else {
                        values::byte_size(&elem_ty)
                    }
                } else {
                    values::byte_size(&elem_ty)
                };

                // Evaluate iterable and store ptr
                self.emit_expr(iterable);
                wasm!(self.func, { local_set(list_scratch); });

                // Initialize index = 0
                wasm!(self.func, {
                    i32_const(0);
                    local_set(idx_scratch);
                });

                // Structure: block $break { loop $loop { check; load; block $continue { body }; i++; br $loop } }
                let break_depth = self.depth;
                wasm!(self.func, { block_empty; });
                self.depth += 1;

                let loop_depth = self.depth;
                wasm!(self.func, { loop_empty; });
                self.depth += 1;

                // Break if index >= len
                wasm!(self.func, {
                    local_get(idx_scratch);
                    local_get(list_scratch);
                    i32_load(0);
                    i32_ge_u;
                    br_if(self.depth - break_depth - 1);
                });

                if is_map {
                    // Map iteration: entries are inline [key][val].
                    // Directly destructure into tuple vars (k, v) without loading a tuple ptr.
                    if let Some(tuple_vars) = var_tuple {
                        if let Ty::Applied(_, args) = &iterable.ty {
                            let key_ty = args.first().cloned().unwrap_or(Ty::String);
                            let val_ty = args.get(1).cloned().unwrap_or(Ty::Int);
                            let key_size = values::byte_size(&key_ty);

                            // Compute base address: list_scratch + 4 + idx * entry_size
                            // Load key at base + 0
                            if let Some(&k_local) = tuple_vars.first().and_then(|tv| self.var_map.get(&tv.0)) {
                                wasm!(self.func, {
                                    local_get(list_scratch);
                                    i32_const(4);
                                    i32_add;
                                    local_get(idx_scratch);
                                    i32_const(entry_size as i32);
                                    i32_mul;
                                    i32_add;
                                });
                                self.emit_load_at(&key_ty, 0);
                                wasm!(self.func, { local_set(k_local); });
                            }

                            // Load value at base + key_size
                            if let Some(&v_local) = tuple_vars.get(1).and_then(|tv| self.var_map.get(&tv.0)) {
                                wasm!(self.func, {
                                    local_get(list_scratch);
                                    i32_const(4);
                                    i32_add;
                                    local_get(idx_scratch);
                                    i32_const(entry_size as i32);
                                    i32_mul;
                                    i32_add;
                                });
                                self.emit_load_at(&val_ty, key_size);
                                wasm!(self.func, { local_set(v_local); });
                            }
                        }
                    }
                } else {
                    // List iteration: load element directly
                    wasm!(self.func, {
                        local_get(list_scratch);
                        i32_const(4);
                        i32_add;
                        local_get(idx_scratch);
                        i32_const(entry_size as i32);
                        i32_mul;
                        i32_add;
                    });
                    self.emit_load_at(&elem_ty, 0);
                    wasm!(self.func, { local_set(loop_var); });

                    // Tuple destructure (for list of tuples)
                    if let Some(tuple_vars) = var_tuple {
                        if let Ty::Tuple(elem_types) = &elem_ty {
                            let mut field_offset = 0u32;
                            for (i, &tv) in tuple_vars.iter().enumerate() {
                                if let Some(&local_idx) = self.var_map.get(&tv.0) {
                                    let ft = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                                    wasm!(self.func, { local_get(loop_var); });
                                    self.emit_load_at(&ft, field_offset);
                                    wasm!(self.func, { local_set(local_idx); });
                                    field_offset += values::byte_size(&ft);
                                }
                            }
                        }
                    }
                }

                // Inner block for continue target
                let continue_depth = self.depth;
                wasm!(self.func, { block_empty; });
                self.depth += 1;

                self.loop_stack.push(super::LoopLabels { break_depth, continue_depth });

                // Body
                for stmt in body {
                    self.emit_stmt(stmt);
                }

                self.loop_stack.pop();
                self.depth -= 1;
                wasm!(self.func, { end; }); // end continue block

                // Increment index (always runs, even after continue)
                wasm!(self.func, {
                    local_get(idx_scratch);
                    i32_const(1);
                    i32_add;
                    local_set(idx_scratch);
                });

                // Loop back
                wasm!(self.func, { br(self.depth - loop_depth - 1); });

                self.depth -= 1;
                wasm!(self.func, { end; }); // end loop
                self.depth -= 1;
                wasm!(self.func, { end; }); // end break block
                self.scratch.free_i32(idx_scratch);
                self.scratch.free_i32(list_scratch);
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
        let subject_ty = self.resolve_subject_type(subject, arms);

        // Resolve result_ty: trust arm body types over expr.ty when they disagree.
        // Checker's Union-Find can contaminate match result vars (e.g., binding to Int
        // when the actual result is Either[String, Int] = i32 pointer).
        let resolved_result = if !arms.is_empty() {
            let arm_vts: Vec<Option<ValType>> = arms.iter().map(|a| values::ty_to_valtype(&a.body.ty)).collect();
            let result_vt = values::ty_to_valtype(result_ty);
            // If all arm bodies agree on a WASM type and it differs from result_ty, use arm type
            if let Some(first_vt) = arm_vts[0] {
                if arm_vts.iter().all(|vt| *vt == Some(first_vt)) && result_vt != Some(first_vt) {
                    arms[0].body.ty.clone()
                } else {
                    result_ty.clone()
                }
            } else {
                result_ty.clone()
            }
        } else {
            result_ty.clone()
        };
        let result_ty = &resolved_result;

        self.emit_expr(subject);

        let is_i64 = matches!(values::ty_to_valtype(&subject_ty), Some(ValType::I64));
        let scratch = if is_i64 {
            self.scratch.alloc_i64()
        } else {
            self.scratch.alloc_i32()
        };

        wasm!(self.func, { local_set(scratch); });

        self.emit_match_arms(arms, scratch, &subject_ty, result_ty, 0);

        if is_i64 {
            self.scratch.free_i64(scratch);
        } else {
            self.scratch.free_i32(scratch);
        }
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
            wasm!(self.func, { unreachable; });
            return;
        }

        let arm = &arms[idx];
        let is_last = idx + 1 >= arms.len();

        eprintln!("[EMIT ARM] fn idx={} pattern={:?} subject_ty={:?} result_ty={:?}", idx, std::mem::discriminant(&arm.pattern), subject_ty, result_ty);
        match &arm.pattern {
            // Wildcard: always matches, emit body directly
            IrPattern::Wildcard => {
                self.emit_expr(&arm.body);
            }

            // Bind: store subject in variable, then emit body
            IrPattern::Bind { var, .. } => {
                if let Some(&local_idx) = self.var_map.get(&var.0) {
                    let var_ty = &self.var_table.get(*var).ty;
                    let var_vt = values::ty_to_valtype(var_ty);
                    let subj_vt = values::ty_to_valtype(subject_ty);
                    // Only bind if types match, or var type is Unknown (trust subject)
                    if var_vt == subj_vt || matches!(var_ty, Ty::Unknown) {
                        wasm!(self.func, {
                            local_get(scratch);
                            local_set(local_idx);
                        });
                    }
                }
                // Handle guard condition
                if let Some(guard) = &arm.guard {
                    self.emit_expr(guard);
                    let bt = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt));
                    self.depth += 1;
                    self.emit_expr(&arm.body);
                    wasm!(self.func, { else_; });
                    if is_last {
                        wasm!(self.func, { unreachable; });
                    } else {
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                    }
                    self.depth -= 1;
                    wasm!(self.func, { end; });
                } else {
                    self.emit_expr(&arm.body);
                }
            }

            // Literal: compare subject to literal, if-else
            IrPattern::Literal { expr: lit_expr } => {
                // Push subject
                wasm!(self.func, { local_get(scratch); });
                // Push literal
                self.emit_expr(lit_expr);
                // Compare
                match subject_ty {
                    Ty::Int => { wasm!(self.func, { i64_eq; }); }
                    Ty::Float => { wasm!(self.func, { f64_eq; }); }
                    Ty::Bool => { wasm!(self.func, { i32_eq; }); }
                    Ty::String => {
                        // String equality: compare pointers (interned literals are deduped)
                        wasm!(self.func, { i32_eq; });
                    }
                    _ => { wasm!(self.func, { i32_eq; }); }
                }

                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                self.depth += 1;
                self.emit_expr(&arm.body);
                wasm!(self.func, { else_; });

                if is_last {
                    wasm!(self.func, { unreachable; });
                } else {
                    self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                }

                self.depth -= 1;
                wasm!(self.func, { end; });
            }

            // Constructor pattern (e.g., Circle(r), Red)
            IrPattern::Constructor { name: ctor_name, args } => {
                let tag_result = self.find_variant_tag_by_ctor(ctor_name, subject_ty);
                eprintln!("[CTOR HANDLER] ctor='{}' tag={:?} subject_ty={:?} idx={} result_ty={:?}", ctor_name, tag_result, subject_ty, idx, result_ty);
                if let Some(tag_val) = tag_result {
                    wasm!(self.func, {
                        local_get(scratch);
                        i32_load(0);
                        i32_const(tag_val as i32);
                        i32_eq;
                    });

                    let bt = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt));
                    self.depth += 1;

                    // Resolve constructor field types from variant info + subject type_args
                    let ctor_fields = self.emitter.record_fields.get(ctor_name).cloned().unwrap_or_default();
                    let subject_type_args: &[Ty] = match subject_ty {
                        Ty::Named(_, args) if !args.is_empty() => args,
                        Ty::Applied(_, args) if !args.is_empty() => args,
                        _ => &[],
                    };
                    // Collect generic param names from ALL constructors of this variant type
                    // (not just the current ctor) to ensure correct index mapping with type_args.
                    // E.g., Either[A,B]: Left(A), Right(B) — gnames must be ["A","B"] not just ["B"].
                    let all_gnames: Vec<String> = if !subject_type_args.is_empty() {
                        let type_name = match subject_ty {
                            Ty::Named(n, _) => Some(n.as_str()),
                            _ => None,
                        };
                        let mut gn: Vec<&str> = Vec::new();
                        if let Some(tn) = type_name {
                            if let Some(cases) = self.emitter.variant_info.get(tn) {
                                for case in cases {
                                    for (_, fty) in &case.fields {
                                        super::expressions::collect_type_param_names(fty, &mut gn);
                                    }
                                }
                            }
                        }
                        gn.iter().map(|s| s.to_string()).collect()
                    } else { vec![] };
                    let gnames_refs: Vec<&str> = all_gnames.iter().map(|s| s.as_str()).collect();

                    // Bind constructor args (tuple payload fields)
                    let mut field_offset = 4u32; // skip tag
                    for (arg_idx, arg_pat) in args.iter().enumerate() {
                        let field_ty = ctor_fields.get(arg_idx)
                            .map(|(_, fty)| {
                                if !subject_type_args.is_empty() && !gnames_refs.is_empty() {
                                    super::expressions::substitute_type_params(fty, &gnames_refs, subject_type_args)
                                } else { fty.clone() }
                            })
                            .unwrap_or_else(|| Ty::Int);

                        if let IrPattern::Bind { var, ty: pat_ty } = arg_pat {
                            if let Some(&local_idx) = self.var_map.get(&var.0) {
                                // Use pattern's own type (set by lowering, resolved by mono)
                                // Fall back to field_ty from variant info only if pattern type is Unknown
                                let load_ty = if matches!(pat_ty, Ty::Unknown | Ty::TypeVar(_))
                                    || matches!(pat_ty, Ty::Named(n, a) if a.is_empty() && n.len() <= 2)
                                { &field_ty } else { pat_ty };
                                wasm!(self.func, { local_get(scratch); });
                                self.emit_load_at(load_ty, field_offset);
                                wasm!(self.func, { local_set(local_idx); });
                            }
                        }
                        field_offset += values::byte_size(&field_ty);
                    }

                    // Handle guard on constructor
                    if let Some(guard) = &arm.guard {
                        self.emit_expr(guard);
                        let bt2 = values::block_type(result_ty);
                        self.func.instruction(&Instruction::If(bt2));
                        self.depth += 1;
                        self.emit_expr(&arm.body);
                        wasm!(self.func, { else_; });
                        if is_last { wasm!(self.func, { unreachable; }); }
                        else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                        self.depth -= 1;
                        wasm!(self.func, { end; });
                    } else {
                        self.emit_expr(&arm.body);
                    }
                    wasm!(self.func, { else_; });
                    if is_last {
                        wasm!(self.func, { unreachable; });
                    } else {
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                    }
                    self.depth -= 1;
                    wasm!(self.func, { end; });
                } else {
                    eprintln!("[CTOR ELSE] ctor='{}' is_last={} — tag not found, falling through", ctor_name, is_last);
                    if is_last {
                        self.emit_expr(&arm.body);
                    } else {
                        wasm!(self.func, { unreachable; });
                    }
                }
            }

            // Some(x) pattern (Option)
            IrPattern::Some { inner } => {
                // some(x) is a non-null pointer. Check ptr != 0, then load value.
                wasm!(self.func, {
                    local_get(scratch);
                    i32_const(0);
                    i32_ne;
                });
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                self.depth += 1;

                // Bind the inner value
                if let IrPattern::Bind { var, .. } = inner.as_ref() {
                    if let Some(&local_idx) = self.var_map.get(&var.0) {
                        let inner_ty = if let Ty::Applied(_, args) = subject_ty {
                            args.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        wasm!(self.func, { local_get(scratch); });
                        self.emit_load_at(&inner_ty, 0);
                        wasm!(self.func, { local_set(local_idx); });
                    }
                }

                // Handle guard
                if let Some(guard) = &arm.guard {
                    self.emit_expr(guard);
                    let bt2 = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt2));
                    self.depth += 1;
                    self.emit_expr(&arm.body);
                    wasm!(self.func, { else_; });
                    if is_last {
                        wasm!(self.func, { unreachable; });
                    } else {
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                    }
                    self.depth -= 1;
                    wasm!(self.func, { end; });
                } else {
                    self.emit_expr(&arm.body);
                }

                wasm!(self.func, { else_; });
                if is_last {
                    wasm!(self.func, { unreachable; });
                } else {
                    self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                }
                self.depth -= 1;
                wasm!(self.func, { end; });
            }

            // None pattern (Option)
            IrPattern::None => {
                // None is represented as i32 0
                wasm!(self.func, {
                    local_get(scratch);
                    i32_eqz;
                });
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                self.depth += 1;
                self.emit_expr(&arm.body);
                wasm!(self.func, { else_; });
                if is_last {
                    wasm!(self.func, { unreachable; });
                } else {
                    self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                }
                self.depth -= 1;
                wasm!(self.func, { end; });
            }

            // RecordPattern: variant constructor match (e.g., Circle { radius })
            IrPattern::RecordPattern { name: ctor_name, fields: pat_fields, .. } => {
                // Look up the tag for this constructor
                let tag = self.find_variant_tag_by_ctor(ctor_name, subject_ty);

                if let Some(tag_val) = tag {
                    // Load tag from subject pointer
                    wasm!(self.func, {
                        local_get(scratch);
                        i32_load(0);
                        i32_const(tag_val as i32);
                        i32_eq;
                    });

                    let bt = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt));
                    self.depth += 1;

                    // Bind fields: load each field from subject + tag_offset + field_offset
                    let case_fields = self.emitter.record_fields.get(ctor_name).cloned().unwrap_or_default();
                    for pf in pat_fields {
                        // Find the field in the case's fields
                        if let Some((foff, fty)) = values::field_offset(&case_fields, &pf.name) {
                            let total_offset = 4 + foff; // 4 = tag size
                            if let Some(&local_idx) = self.find_var_by_field(&pf.name, &case_fields) {
                                wasm!(self.func, { local_get(scratch); });
                                self.emit_load_at(&fty, total_offset);
                                wasm!(self.func, { local_set(local_idx); });
                            }
                        }
                    }

                    self.emit_expr(&arm.body);
                    wasm!(self.func, { else_; });

                    if is_last {
                        wasm!(self.func, { unreachable; });
                    } else {
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                    }

                    self.depth -= 1;
                    wasm!(self.func, { end; });
                } else {
                    // Not a variant — treat as plain record (always matches)
                    self.emit_expr(&arm.body);
                }
            }

            // Ok(x) pattern (Result)
            IrPattern::Ok { inner } => {
                // Result ok = tag 0. Check tag, then bind value.
                wasm!(self.func, {
                    local_get(scratch);
                    i32_load(0);
                    i32_eqz;
                });
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                self.depth += 1;
                if let IrPattern::Bind { var, .. } = inner.as_ref() {
                    if let Some(&local_idx) = self.var_map.get(&var.0) {
                        let inner_ty = if let Ty::Applied(_, args) = subject_ty {
                            args.first().cloned().unwrap_or(Ty::Int)
                        } else { Ty::Int };
                        wasm!(self.func, { local_get(scratch); });
                        self.emit_load_at(&inner_ty, 4);
                        wasm!(self.func, { local_set(local_idx); });
                    }
                }
                if let Some(guard) = &arm.guard {
                    self.emit_expr(guard);
                    let bt2 = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt2));
                    self.depth += 1;
                    self.emit_expr(&arm.body);
                    wasm!(self.func, { else_; });
                    if is_last { wasm!(self.func, { unreachable; }); }
                    else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                    self.depth -= 1;
                    wasm!(self.func, { end; });
                } else {
                    self.emit_expr(&arm.body);
                }
                wasm!(self.func, { else_; });
                if is_last { wasm!(self.func, { unreachable; }); }
                else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                self.depth -= 1;
                wasm!(self.func, { end; });
            }

            // Err(e) pattern (Result)
            IrPattern::Err { inner } => {
                wasm!(self.func, {
                    local_get(scratch);
                    i32_load(0);
                    i32_const(0);
                    i32_ne;
                });
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                self.depth += 1;
                if let IrPattern::Bind { var, .. } = inner.as_ref() {
                    if let Some(&local_idx) = self.var_map.get(&var.0) {
                        let inner_ty = if let Ty::Applied(_, args) = subject_ty {
                            args.get(1).cloned().unwrap_or(Ty::String)
                        } else { Ty::String };
                        wasm!(self.func, { local_get(scratch); });
                        self.emit_load_at(&inner_ty, 4);
                        wasm!(self.func, { local_set(local_idx); });
                    }
                }
                if let Some(guard) = &arm.guard {
                    self.emit_expr(guard);
                    let bt2 = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt2));
                    self.depth += 1;
                    self.emit_expr(&arm.body);
                    wasm!(self.func, { else_; });
                    if is_last { wasm!(self.func, { unreachable; }); }
                    else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                    self.depth -= 1;
                    wasm!(self.func, { end; });
                } else {
                    self.emit_expr(&arm.body);
                }
                wasm!(self.func, { else_; });
                if is_last { wasm!(self.func, { unreachable; }); }
                else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                self.depth -= 1;
                wasm!(self.func, { end; });
            }

            // Tuple pattern: (a, b) => ...
            IrPattern::Tuple { elements } => {
                if let Ty::Tuple(elem_types) = subject_ty {
                    let has_literal = elements.iter().any(|p| matches!(p, IrPattern::Literal { .. }));

                    if has_literal && !is_last {
                        // Build condition: check all literal elements
                        let mut offset = 0u32;
                        let mut cond_count = 0;
                        for (i, elem_pat) in elements.iter().enumerate() {
                            let ft = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                            if let IrPattern::Literal { expr: lit_expr } = elem_pat {
                                wasm!(self.func, { local_get(scratch); });
                                self.emit_load_at(&ft, offset);
                                self.emit_expr(lit_expr);
                                match &ft {
                                    Ty::Int => { wasm!(self.func, { i64_eq; }); }
                                    Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }
                                    _ => { wasm!(self.func, { i32_eq; }); }
                                }
                                cond_count += 1;
                            }
                            offset += values::byte_size(&ft);
                        }
                        for _ in 1..cond_count {
                            wasm!(self.func, { i32_and; });
                        }
                        // if condition: bind + body, else: next arm
                        let resolved_result = values::ty_to_valtype(result_ty);
                        match resolved_result {
                            Some(ValType::I64) => { wasm!(self.func, { if_i64; }); }
                            Some(ValType::F64) => { wasm!(self.func, { if_f64; }); }
                            Some(ValType::I32) => { wasm!(self.func, { if_i32; }); }
                            _ => { wasm!(self.func, { if_i32; }); } // String/ptr results are i32
                        }
                        self.depth += 1;
                    }

                    // Bind elements
                    let mut offset = 0u32;
                    for (i, elem_pat) in elements.iter().enumerate() {
                        let ft = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                        if let IrPattern::Bind { var, .. } = elem_pat {
                            if let Some(&local_idx) = self.var_map.get(&var.0) {
                                wasm!(self.func, { local_get(scratch); });
                                self.emit_load_at(&ft, offset);
                                wasm!(self.func, { local_set(local_idx); });
                            }
                        }
                        offset += values::byte_size(&ft);
                    }

                    self.emit_expr(&arm.body);

                    if has_literal && !is_last {
                        wasm!(self.func, { else_; });
                        // Emit remaining arms
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                        wasm!(self.func, { end; });
                        self.depth -= 1;
                        return; // Don't fall through to normal next-arm processing
                    }
                } else {
                    self.emit_expr(&arm.body);
                }
            }

            // Catch-all for unsupported patterns
            _ => {
                if is_last {
                    self.emit_expr(&arm.body);
                } else {
                    wasm!(self.func, { unreachable; });
                }
            }
        }
    }
}
