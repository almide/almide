impl FuncCompiler<'_> {
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

        match &arm.pattern {
            // Wildcard: always matches, but may have a guard
            IrPattern::Wildcard => {
                if let Some(guard) = &arm.guard {
                    self.emit_expr(guard);
                    let bt = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt));
                    let _g = self.depth_push();
                    self.emit_match_arm_body(&arm.body, result_ty);
                    wasm!(self.func, { else_; });
                    if is_last {
                        wasm!(self.func, { unreachable; });
                    } else {
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                    }
                    self.depth_pop(_g);
                    wasm!(self.func, { end; });
                } else {
                    self.emit_match_arm_body(&arm.body, result_ty);
                }
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
                    let _g = self.depth_push();
                    self.emit_match_arm_body(&arm.body, result_ty);
                    wasm!(self.func, { else_; });
                    if is_last {
                        wasm!(self.func, { unreachable; });
                    } else {
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                    }
                    self.depth_pop(_g);
                    wasm!(self.func, { end; });
                } else {
                    self.emit_match_arm_body(&arm.body, result_ty);
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
                        // String equality: byte-level comparison (runtime strings have different pointers)
                        wasm!(self.func, { call(self.emitter.rt.string.eq); });
                    }
                    _ => { wasm!(self.func, { i32_eq; }); }
                }

                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                let _g = self.depth_push();
                self.emit_match_arm_body(&arm.body, result_ty);
                wasm!(self.func, { else_; });

                if is_last {
                    wasm!(self.func, { unreachable; });
                } else {
                    self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                }

                self.depth_pop(_g);
                wasm!(self.func, { end; });
            }

            // Constructor pattern (e.g., Circle(r), Red)
            IrPattern::Constructor { name: ctor_name, args } => {
                let tag_result = self.find_variant_tag_by_ctor(ctor_name, subject_ty);
                if let Some(tag_val) = tag_result {
                    wasm!(self.func, {
                        local_get(scratch);
                        i32_load(0);
                        i32_const(tag_val as i32);
                        i32_eq;
                    });

                    let bt = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt));
                    let ctor_guard = self.depth_push();

                    // Resolve constructor field types from variant info + subject type_args
                    let ctor_fields = self.emitter.fields_of(ctor_name);
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

                    // #607: any arg that needs a discriminant test — a literal OR
                    // a NESTED constructor/Some/None (the inner tag must gate the
                    // arm or wasm matches the wrong nested case silently).
                    let has_literal_args = args.iter().any(|p| matches!(p,
                        IrPattern::Literal { .. } | IrPattern::Constructor { .. }
                        | IrPattern::Some { .. } | IrPattern::None
                        | IrPattern::RecordPattern { .. }));

                    // Resolve field types
                    let resolved_fields: Vec<Ty> = (0..args.len()).map(|arg_idx| {
                        ctor_fields.get(arg_idx)
                            .map(|(_, fty)| {
                                if !subject_type_args.is_empty() && !gnames_refs.is_empty() {
                                    super::expressions::substitute_type_params(fty, &gnames_refs, subject_type_args)
                                } else { fty.clone() }
                            })
                            .unwrap_or_else(|| Ty::Int)
                    }).collect();

                    // If there are literal args, emit value checks as additional condition
                    let literal_guard = if has_literal_args && !is_last {
                        let mut field_offset = 4u32;
                        let mut cond_count = 0;
                        for (arg_idx, arg_pat) in args.iter().enumerate() {
                            let field_ty = resolved_fields[arg_idx].clone();
                            // #607: recursive discriminant tests (literal + nested
                            // ctor tag + Some/None) instead of only top-level literals.
                            cond_count += self.emit_arg_tests(scratch, field_offset, &field_ty, arg_pat);
                            field_offset += values::byte_size(&field_ty);
                        }
                        for _ in 1..cond_count {
                            wasm!(self.func, { i32_and; });
                        }
                        let bt2 = values::block_type(result_ty);
                        self.func.instruction(&Instruction::If(bt2));
                        Some(self.depth_push())
                    } else { None };

                    // Bind constructor args (recursively — #607). The flat loop
                    // bound only top-level `Bind`; a nested ctor/Some/record arg's
                    // inner Binds were SILENTLY DROPPED (bound 0). bind_arg walks
                    // through nested payloads. (The discriminant tests for those
                    // nested patterns were emitted in the guard above for non-last
                    // arms; the last arm is exhaustive so it only binds.)
                    let mut field_offset = 4u32; // skip tag
                    for (arg_idx, arg_pat) in args.iter().enumerate() {
                        let field_ty = resolved_fields[arg_idx].clone();
                        self.bind_arg(scratch, field_offset, &field_ty, arg_pat);
                        field_offset += values::byte_size(&field_ty);
                    }

                    // Handle guard on constructor
                    if let Some(guard) = &arm.guard {
                        self.emit_expr(guard);
                        let bt2 = values::block_type(result_ty);
                        self.func.instruction(&Instruction::If(bt2));
                        let guard_g = self.depth_push();
                        self.emit_match_arm_body(&arm.body, result_ty);
                        wasm!(self.func, { else_; });
                        if is_last { wasm!(self.func, { unreachable; }); }
                        else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                        self.depth_pop(guard_g);
                        wasm!(self.func, { end; });
                    } else {
                        self.emit_match_arm_body(&arm.body, result_ty);
                    }

                    // Close literal guard if present
                    if let Some(lg) = literal_guard {
                        wasm!(self.func, { else_; });
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                        self.depth_pop(lg);
                        wasm!(self.func, { end; });
                    }
                    wasm!(self.func, { else_; });
                    if is_last {
                        wasm!(self.func, { unreachable; });
                    } else {
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                    }
                    self.depth_pop(ctor_guard);
                    wasm!(self.func, { end; });
                } else {
                    if is_last {
                        self.emit_match_arm_body(&arm.body, result_ty);
                    } else {
                        wasm!(self.func, { unreachable; });
                    }
                }
            }

            // Some(x) pattern (Option)
            IrPattern::Some { inner } => {
                // some(x) is a non-null pointer. Check ptr != 0, then bind inner.
                wasm!(self.func, {
                    local_get(scratch);
                    i32_const(0);
                    i32_ne;
                });
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                let some_guard = self.depth_push();

                let inner_ty = if let Ty::Applied(_, args) = subject_ty {
                    args.first().cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };

                // Bind inner pattern (handles Bind, Tuple, Constructor, nested Some, etc.)
                let body_emitted = self.emit_inner_pattern_and_body(
                    inner, scratch, 0, &inner_ty,
                    arm, arms, scratch, subject_ty, result_ty, idx, is_last,
                );
                if !body_emitted {
                    self.emit_arm_body_or_guard(arm, arms, scratch, subject_ty, result_ty, idx, is_last);
                }

                wasm!(self.func, { else_; });
                if is_last {
                    wasm!(self.func, { unreachable; });
                } else {
                    self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                }
                self.depth_pop(some_guard);
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
                let none_guard = self.depth_push();
                self.emit_match_arm_body(&arm.body, result_ty);
                wasm!(self.func, { else_; });
                if is_last {
                    wasm!(self.func, { unreachable; });
                } else {
                    self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                }
                self.depth_pop(none_guard);
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
                    let rec_guard = self.depth_push();

                    // Bind fields: load each field from subject + tag_offset + field_offset
                    let case_fields = self.emitter.fields_of(ctor_name);
                    for pf in pat_fields {
                        if let Some((foff, fty)) = values::field_offset(&case_fields, &pf.name) {
                            let total_offset = 4 + foff; // 4 = tag size
                            // Use VarId from Bind pattern (populated by lowering) to avoid name collisions
                            let local_idx = if let Some(almide_ir::IrPattern::Bind { var, .. }) = &pf.pattern {
                                self.var_map.get(&var.0).copied()
                            } else {
                                self.find_var_by_field(&pf.name, &case_fields).copied()
                            };
                            if let Some(idx) = local_idx {
                                wasm!(self.func, { local_get(scratch); });
                                self.emit_load_at(&fty, total_offset);
                                wasm!(self.func, { local_set(idx); });
                            }
                        }
                    }

                    self.emit_match_arm_body(&arm.body, result_ty);
                    wasm!(self.func, { else_; });

                    if is_last {
                        wasm!(self.func, { unreachable; });
                    } else {
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                    }

                    self.depth_pop(rec_guard);
                    wasm!(self.func, { end; });
                } else {
                    // Plain record (not a variant): the structural shape is guaranteed
                    // by the type checker, so the record match itself always succeeds.
                    // Still need to bind fields (pattern = None means implicit bind from
                    // field name → VarId) and then run any guard; on guard failure, fall
                    // through to the next arm.
                    let case_fields = self.extract_record_fields(subject_ty);
                    for pf in pat_fields {
                        if let Some((foff, fty)) = values::field_offset(&case_fields, &pf.name) {
                            let local_idx = if let Some(almide_ir::IrPattern::Bind { var, .. }) = &pf.pattern {
                                self.var_map.get(&var.0).copied()
                            } else {
                                self.find_var_by_field(&pf.name, &case_fields).copied()
                            };
                            if let Some(idx) = local_idx {
                                wasm!(self.func, { local_get(scratch); });
                                self.emit_load_at(&fty, foff);
                                wasm!(self.func, { local_set(idx); });
                            }
                        }
                    }
                    // Run guard (if any) and fall through to subsequent arms when it fails.
                    self.emit_arm_body_or_guard(arm, arms, scratch, subject_ty, result_ty, idx, is_last);
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
                let ok_guard = self.depth_push();

                let inner_ty = if let Ty::Applied(_, args) = subject_ty {
                    args.first().cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };

                let body_emitted = self.emit_inner_pattern_and_body(
                    inner, scratch, 4, &inner_ty,
                    arm, arms, scratch, subject_ty, result_ty, idx, is_last,
                );
                if !body_emitted {
                    self.emit_arm_body_or_guard(arm, arms, scratch, subject_ty, result_ty, idx, is_last);
                }

                wasm!(self.func, { else_; });
                if is_last { wasm!(self.func, { unreachable; }); }
                else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                self.depth_pop(ok_guard);
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
                let err_guard = self.depth_push();

                let inner_ty = if let Ty::Applied(_, args) = subject_ty {
                    args.get(1).cloned().unwrap_or(Ty::String)
                } else { Ty::String };

                let body_emitted = self.emit_inner_pattern_and_body(
                    inner, scratch, 4, &inner_ty,
                    arm, arms, scratch, subject_ty, result_ty, idx, is_last,
                );
                if !body_emitted {
                    self.emit_arm_body_or_guard(arm, arms, scratch, subject_ty, result_ty, idx, is_last);
                }

                wasm!(self.func, { else_; });
                if is_last { wasm!(self.func, { unreachable; }); }
                else { self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1); }
                self.depth_pop(err_guard);
                wasm!(self.func, { end; });
            }

            // List pattern: [a, b] => ... — pass through (list matching not yet implemented in WASM)
            IrPattern::List { .. } => {
                self.emit_match_arm_body(&arm.body, result_ty);
            }

            // Tuple pattern: (a, b) => ...
            IrPattern::Tuple { elements } => {
                if let Ty::Tuple(elem_types) = subject_ty {
                    let has_literal = elements.iter().any(|p| matches!(p, IrPattern::Literal { .. }));

                    let tuple_guard = if has_literal && !is_last {
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
                        Some(self.depth_push())
                    } else {
                        None
                    };

                    // Check if any tuple elements have nested patterns (Some, None, Constructor)
                    let has_nested = elements.iter().any(|p| matches!(p, IrPattern::Some { .. } | IrPattern::None | IrPattern::Constructor { .. }));

                    if has_nested && !is_last {
                        // Build condition: all nested patterns must match
                        let mut offset = 0u32;
                        let mut cond_count = 0;
                        for (i, elem_pat) in elements.iter().enumerate() {
                            let ft = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                            match elem_pat {
                                IrPattern::Some { .. } => {
                                    // Option: check ptr != 0
                                    wasm!(self.func, { local_get(scratch); });
                                    self.emit_load_at(&ft, offset);
                                    wasm!(self.func, { i32_const(0); i32_ne; });
                                    cond_count += 1;
                                }
                                IrPattern::None => {
                                    wasm!(self.func, { local_get(scratch); });
                                    self.emit_load_at(&ft, offset);
                                    wasm!(self.func, { i32_eqz; });
                                    cond_count += 1;
                                }
                                IrPattern::Constructor { name: ctor_name, .. } => {
                                    // Variant element: the tuple slot holds a pointer to
                                    // `[tag:i32][payload…]`. Match by loading that tag and
                                    // comparing it to the constructor's. (#633: this arm was
                                    // missing, so a Constructor element contributed no
                                    // condition operand → the `If` had nothing on the stack.)
                                    if let Some(tag_val) = self.find_variant_tag_by_ctor(ctor_name, &ft) {
                                        wasm!(self.func, {
                                            local_get(scratch);
                                            i32_load(offset);
                                            i32_load(0);
                                            i32_const(tag_val as i32);
                                            i32_eq;
                                        });
                                        cond_count += 1;
                                    }
                                }
                                _ => {}
                            }
                            offset += values::byte_size(&ft);
                        }
                        for _ in 1..cond_count {
                            wasm!(self.func, { i32_and; });
                        }
                        // Defensive: if no element produced a condition operand (e.g.
                        // a Constructor whose element type couldn't be resolved to a
                        // variant), the `If` would have nothing on the stack. Push a
                        // constant `true` so the module stays well-formed — the arm
                        // then matches unconditionally (the safe direction, and the
                        // bind loop below still runs).
                        if cond_count == 0 {
                            wasm!(self.func, { i32_const(1); });
                        }
                        let bt = values::block_type(result_ty);
                        self.func.instruction(&Instruction::If(bt));
                        let nested_guard = self.depth_push();

                        // Bind elements (including unwrap for Some)
                        let mut offset2 = 0u32;
                        for (i, elem_pat) in elements.iter().enumerate() {
                            let ft = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                            match elem_pat {
                                IrPattern::Bind { var, .. } => {
                                    if let Some(&local_idx) = self.var_map.get(&var.0) {
                                        wasm!(self.func, { local_get(scratch); });
                                        self.emit_load_at(&ft, offset2);
                                        wasm!(self.func, { local_set(local_idx); });
                                    }
                                }
                                IrPattern::Some { inner } => {
                                    // Load option ptr, then bind inner
                                    if let IrPattern::Bind { var, .. } = inner.as_ref() {
                                        if let Some(&local_idx) = self.var_map.get(&var.0) {
                                            let inner_ty = if let Ty::Applied(_, args) = &ft {
                                                args.first().cloned().unwrap_or(Ty::Int)
                                            } else { Ty::Int };
                                            // Load option ptr from tuple
                                            let opt_scratch = self.scratch.alloc_i32();
                                            wasm!(self.func, { local_get(scratch); i32_load(offset2); local_set(opt_scratch); });
                                            wasm!(self.func, { local_get(opt_scratch); });
                                            self.emit_load_at(&inner_ty, 0);
                                            wasm!(self.func, { local_set(local_idx); });
                                            self.scratch.free_i32(opt_scratch);
                                        }
                                    }
                                }
                                IrPattern::Constructor { name: ctor_name, args } => {
                                    // Bind any payload binders of a variant element. The
                                    // element slot holds a pointer to `[tag][field0]…`;
                                    // payload fields start at byte 4. (#633)
                                    if !args.is_empty() {
                                        let ctor_fields = self.emitter.fields_of(ctor_name);
                                        let var_scratch = self.scratch.alloc_i32();
                                        wasm!(self.func, { local_get(scratch); i32_load(offset2); local_set(var_scratch); });
                                        let mut field_offset = 4u32; // skip tag
                                        for (arg_idx, arg_pat) in args.iter().enumerate() {
                                            let field_ty = ctor_fields.get(arg_idx)
                                                .map(|(_, fty)| fty.clone())
                                                .unwrap_or(Ty::Int);
                                            if let IrPattern::Bind { var, .. } = arg_pat {
                                                if let Some(&local_idx) = self.var_map.get(&var.0) {
                                                    wasm!(self.func, { local_get(var_scratch); });
                                                    self.emit_load_at(&field_ty, field_offset);
                                                    wasm!(self.func, { local_set(local_idx); });
                                                }
                                            }
                                            field_offset += values::byte_size(&field_ty);
                                        }
                                        self.scratch.free_i32(var_scratch);
                                    }
                                }
                                _ => {}
                            }
                            offset2 += values::byte_size(&ft);
                        }

                        self.emit_match_arm_body(&arm.body, result_ty);

                        wasm!(self.func, { else_; });
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                        self.depth_pop(nested_guard);
                        wasm!(self.func, { end; });

                        if let Some(tg) = tuple_guard {
                            self.depth_pop(tg);
                        }
                        return; // Don't fall through to normal processing
                    }

                    // Simple case: only Bind patterns
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

                    self.emit_match_arm_body(&arm.body, result_ty);

                    if let Some(tg) = tuple_guard {
                        wasm!(self.func, { else_; });
                        // Emit remaining arms
                        self.emit_match_arms(arms, scratch, subject_ty, result_ty, idx + 1);
                        wasm!(self.func, { end; });
                        self.depth_pop(tg);
                        return; // Don't fall through to normal next-arm processing
                    }
                } else {
                    self.emit_match_arm_body(&arm.body, result_ty);
                }
            }
        }
    }
}
