impl FuncCompiler<'_> {
    /// Emit arm body with optional guard. Extracted to avoid duplication in nested patterns.
    fn emit_arm_body_or_guard(
        &mut self,
        arm: &IrMatchArm,
        arms: &[IrMatchArm],
        scratch: u32,
        subject_ty: &Ty,
        result_ty: &Ty,
        idx: usize,
        is_last: bool,
    ) {
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
    }

    /// Bind an inner pattern from a container (Option/Result) and emit the arm body.
    /// `container_scratch` is the outer pointer (Option ptr or Result ptr).
    /// `inner_offset` is the offset to load the inner value (0 for Option, 4 for Result).
    /// Returns true if body was emitted (for conditional inner patterns like Constructor).
    /// #607: emit the DISCRIMINANT tests for one constructor-arg pattern at
    /// `field_offset` inside the ctor pointed to by `ctor_local`, recursively.
    /// Pushes one i32 bool per test and returns the count. The flat arm-binding
    /// loop only handled `Bind`/`Literal`; a nested `Constructor`/`Some`/`None`/
    /// `Ok`/`Err` arg got NO inner discriminant test, so wasm matched the wrong
    /// arm (silent-wrong, exit 0). `RecordPattern`/`Tuple`/`Bind`/`Wildcard`
    /// need no test (a record always matches its ctor; binds are unconditional).
    fn emit_arg_tests(&mut self, ctor_local: u32, field_offset: u32, field_ty: &Ty, pat: &IrPattern) -> u32 {
        match pat {
            IrPattern::Literal { expr } => {
                wasm!(self.func, { local_get(ctor_local); });
                self.emit_load_at(field_ty, field_offset);
                self.emit_expr(expr);
                self.emit_eq_typed(field_ty);
                1
            }
            IrPattern::Constructor { name, args } => {
                let inner = self.scratch.alloc_i32();
                wasm!(self.func, { local_get(ctor_local); i32_load(field_offset); local_set(inner); });
                let mut count = 0;
                if let Some(tag) = self.find_variant_tag_by_ctor(name, field_ty) {
                    wasm!(self.func, { local_get(inner); i32_load(0); i32_const(tag as i32); i32_eq; });
                    count += 1;
                }
                let inner_fields = self.emitter.fields_of(name);
                let mut off = 4u32;
                for (i, ap) in args.iter().enumerate() {
                    let aty = inner_fields.get(i).map(|(_, t)| t.clone()).unwrap_or(Ty::Int);
                    count += self.emit_arg_tests(inner, off, &aty, ap);
                    off += values::byte_size(&aty);
                }
                self.scratch.free_i32(inner);
                count
            }
            IrPattern::Some { .. } => {
                wasm!(self.func, { local_get(ctor_local); i32_load(field_offset); i32_const(0); i32_ne; });
                1
            }
            IrPattern::None => {
                wasm!(self.func, { local_get(ctor_local); i32_load(field_offset); i32_eqz; });
                1
            }
            // A variant-RECORD ctor arg (`Held(Circle { r })`) — the inner
            // `RecordPattern` carries the variant ctor NAME, so its tag must be
            // tested (Circle vs Square) exactly like a tuple-payload ctor.
            IrPattern::RecordPattern { name, .. } => {
                if let Some(tag) = self.find_variant_tag_by_ctor(name, field_ty) {
                    wasm!(self.func, { local_get(ctor_local); i32_load(field_offset); i32_load(0); i32_const(tag as i32); i32_eq; });
                    1
                } else { 0 }
            }
            _ => 0,
        }
    }

    /// #607: bind every `Bind` var reachable through a constructor-arg pattern at
    /// `field_offset` inside `ctor_local`, recursing through nested ctor / Some /
    /// record payloads. The discriminant tests are emitted separately (in the
    /// arm guard); this only loads + binds, so it runs inside the matched branch.
    fn bind_arg(&mut self, ctor_local: u32, field_offset: u32, field_ty: &Ty, pat: &IrPattern) {
        match pat {
            IrPattern::Bind { var, ty: pat_ty } => {
                if let Some(&local_idx) = self.var_map.get(&var.0) {
                    let load_ty = if pat_ty.is_unresolved()
                        || matches!(pat_ty, Ty::Named(n, a) if a.is_empty() && n.len() <= 2)
                    { field_ty } else { pat_ty };
                    wasm!(self.func, { local_get(ctor_local); });
                    self.emit_load_at(load_ty, field_offset);
                    wasm!(self.func, { local_set(local_idx); });
                }
            }
            IrPattern::Constructor { name, args } => {
                let inner = self.scratch.alloc_i32();
                wasm!(self.func, { local_get(ctor_local); i32_load(field_offset); local_set(inner); });
                let inner_fields = self.emitter.fields_of(name);
                let mut off = 4u32;
                for (i, ap) in args.iter().enumerate() {
                    let aty = inner_fields.get(i).map(|(_, t)| t.clone()).unwrap_or(Ty::Int);
                    self.bind_arg(inner, off, &aty, ap);
                    off += values::byte_size(&aty);
                }
                self.scratch.free_i32(inner);
            }
            IrPattern::Some { inner: inner_pat } => {
                // Some payload is at offset 0 of the inner pointer.
                let inner = self.scratch.alloc_i32();
                wasm!(self.func, { local_get(ctor_local); i32_load(field_offset); local_set(inner); });
                let payload_ty = match field_ty {
                    Ty::Applied(_, a) => a.first().cloned().unwrap_or(Ty::Int),
                    _ => Ty::Int,
                };
                self.bind_arg(inner, 0, &payload_ty, inner_pat);
                self.scratch.free_i32(inner);
            }
            IrPattern::RecordPattern { name, fields: pat_fields, .. } => {
                // The record value is a pointer at field_offset. For a
                // variant-RECORD ctor (named) the fields sit AFTER the 4-byte
                // tag; resolve the ctor's fields for the offsets (mirrors the
                // top-level RecordPattern arm).
                let rec = self.scratch.alloc_i32();
                wasm!(self.func, { local_get(ctor_local); i32_load(field_offset); local_set(rec); });
                let is_variant = self.find_variant_tag_by_ctor(name, field_ty).is_some();
                let tag_off = if is_variant { 4u32 } else { 0u32 };
                let rec_fields = if is_variant {
                    self.emitter.fields_of(name)
                } else {
                    self.extract_record_fields(field_ty)
                };
                for pf in pat_fields {
                    if let Some((off, fty)) = super::values::field_offset(&rec_fields, &pf.name) {
                        let total = tag_off + off;
                        match &pf.pattern {
                            Some(IrPattern::Bind { var, .. }) => {
                                if let Some(&local_idx) = self.var_map.get(&var.0) {
                                    wasm!(self.func, { local_get(rec); });
                                    self.emit_load_at(&fty, total);
                                    wasm!(self.func, { local_set(local_idx); });
                                }
                            }
                            Some(inner_pat) => self.bind_arg(rec, total, &fty, inner_pat),
                            None => {
                                if let Some(&local_idx) = self.find_var_by_field(&pf.name, &rec_fields) {
                                    wasm!(self.func, { local_get(rec); });
                                    self.emit_load_at(&fty, total);
                                    wasm!(self.func, { local_set(local_idx); });
                                }
                            }
                        }
                    }
                }
                self.scratch.free_i32(rec);
            }
            _ => {}
        }
    }

    fn emit_inner_pattern_and_body(
        &mut self,
        inner: &IrPattern,
        container_scratch: u32,
        inner_offset: u32,
        inner_ty: &Ty,
        arm: &IrMatchArm,
        arms: &[IrMatchArm],
        outer_scratch: u32,
        subject_ty: &Ty,
        result_ty: &Ty,
        idx: usize,
        is_last: bool,
    ) -> bool {
        match inner {
            IrPattern::Bind { var, .. } => {
                if let Some(&local_idx) = self.var_map.get(&var.0) {
                    wasm!(self.func, { local_get(container_scratch); });
                    self.emit_load_at(inner_ty, inner_offset);
                    wasm!(self.func, { local_set(local_idx); });
                }
                false // caller emits body
            }
            IrPattern::Wildcard => {
                false
            }
            // A literal nested inside a container constructor, e.g.
            // `some("target")`, `ok(0)`, `err("EOF")`. The container guard
            // (`Some`/`Ok`/`Err`) only checked the tag/non-null; the inner
            // literal equality was NEVER emitted, so wasm matched ANY
            // `some(_)`/`ok(_)`/`err(_)` — silently wrong for every
            // string/int-dispatching match (e.g. balanced-parens on wasm
            // matched `(` against `[`). Load the inner value and compare it to
            // the literal with the shared type-directed equality, then emit the
            // body only on a match.
            IrPattern::Literal { expr: lit_expr } => {
                wasm!(self.func, { local_get(container_scratch); });
                self.emit_load_at(inner_ty, inner_offset);
                self.emit_expr(lit_expr);
                let inner_ty_c = inner_ty.clone();
                self.emit_eq_typed(&inner_ty_c);
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                let lit_guard = self.depth_push();
                self.emit_arm_body_or_guard(arm, arms, outer_scratch, subject_ty, result_ty, idx, is_last);
                wasm!(self.func, { else_; });
                if is_last { wasm!(self.func, { unreachable; }); }
                else { self.emit_match_arms(arms, outer_scratch, subject_ty, result_ty, idx + 1); }
                self.depth_pop(lit_guard);
                wasm!(self.func, { end; });
                true // body was emitted conditionally
            }
            IrPattern::Tuple { elements } => {
                // Inner value is a tuple pointer
                let inner_scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_get(container_scratch);
                    i32_load(inner_offset);
                    local_set(inner_scratch);
                });
                if let Ty::Tuple(elem_types) = inner_ty {
                    let mut offset = 0u32;
                    for (i, elem_pat) in elements.iter().enumerate() {
                        let ety = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                        if let IrPattern::Bind { var, .. } = elem_pat {
                            if let Some(&local_idx) = self.var_map.get(&var.0) {
                                wasm!(self.func, { local_get(inner_scratch); });
                                self.emit_load_at(&ety, offset);
                                wasm!(self.func, { local_set(local_idx); });
                            }
                        }
                        offset += values::byte_size(&ety);
                    }
                }
                self.scratch.free_i32(inner_scratch);
                false // caller emits body
            }
            IrPattern::Constructor { name: ctor_name, args } => {
                // Inner value is a variant pointer — need conditional tag check
                let inner_scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_get(container_scratch);
                    i32_load(inner_offset);
                    local_set(inner_scratch);
                });
                let tag = self.find_variant_tag_by_ctor(ctor_name, inner_ty);
                if let Some(tag_val) = tag {
                    wasm!(self.func, {
                        local_get(inner_scratch);
                        i32_load(0);
                        i32_const(tag_val as i32);
                        i32_eq;
                    });
                    let bt = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt));
                    let ctor_guard = self.depth_push();

                    // Bind constructor fields with type param substitution
                    let ctor_fields = self.emitter.fields_of(ctor_name.as_str());
                    let type_args: &[Ty] = match inner_ty {
                        Ty::Named(_, args) if !args.is_empty() => args,
                        Ty::Applied(_, args) if !args.is_empty() => args,
                        _ => &[],
                    };
                    let all_gnames: Vec<String> = if !type_args.is_empty() {
                        let type_name = match inner_ty { Ty::Named(n, _) => Some(n.as_str()), _ => None };
                        let mut gn: Vec<&str> = Vec::new();
                        if let Some(tn) = type_name {
                            if let Some(cases) = self.emitter.variant_info.get(tn) {
                                for case in cases { for (_, fty) in &case.fields { super::expressions::collect_type_param_names(fty, &mut gn); } }
                            }
                        }
                        gn.iter().map(|s| s.to_string()).collect()
                    } else { vec![] };
                    let gnames_refs: Vec<&str> = all_gnames.iter().map(|s| s.as_str()).collect();

                    let mut field_offset = 4u32;
                    for (arg_idx, arg_pat) in args.iter().enumerate() {
                        let field_ty = ctor_fields.get(arg_idx)
                            .map(|(_, fty)| {
                                if !type_args.is_empty() && !gnames_refs.is_empty() {
                                    super::expressions::substitute_type_params(fty, &gnames_refs, type_args)
                                } else { fty.clone() }
                            })
                            .unwrap_or(Ty::Int);
                        if let IrPattern::Bind { var, ty: pat_ty } = arg_pat {
                            if let Some(&local_idx) = self.var_map.get(&var.0) {
                                let load_ty = if pat_ty.is_unresolved()
                                    || matches!(pat_ty, Ty::Named(n, a) if a.is_empty() && n.len() <= 2)
                                { &field_ty } else { pat_ty };
                                wasm!(self.func, { local_get(inner_scratch); });
                                self.emit_load_at(load_ty, field_offset);
                                wasm!(self.func, { local_set(local_idx); });
                            }
                        }
                        field_offset += values::byte_size(&field_ty);
                    }

                    self.emit_arm_body_or_guard(arm, arms, outer_scratch, subject_ty, result_ty, idx, is_last);
                    wasm!(self.func, { else_; });
                    if is_last { wasm!(self.func, { unreachable; }); }
                    else { self.emit_match_arms(arms, outer_scratch, subject_ty, result_ty, idx + 1); }
                    self.depth_pop(ctor_guard);
                    wasm!(self.func, { end; });
                } else {
                    // Tag not found — just emit body (best effort)
                    self.scratch.free_i32(inner_scratch);
                    return false;
                }
                self.scratch.free_i32(inner_scratch);
                true // body was emitted
            }
            IrPattern::Some { inner: inner2 } => {
                // Nested Some: load inner ptr, check non-null
                let inner_scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_get(container_scratch);
                    i32_load(inner_offset);
                    local_set(inner_scratch);
                });
                wasm!(self.func, {
                    local_get(inner_scratch);
                    i32_const(0);
                    i32_ne;
                });
                let nested_ty = if let Ty::Applied(_, args) = inner_ty {
                    args.first().cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                let some_guard = self.depth_push();
                let emitted = self.emit_inner_pattern_and_body(
                    inner2, inner_scratch, 0, &nested_ty,
                    arm, arms, outer_scratch, subject_ty, result_ty, idx, is_last,
                );
                if !emitted {
                    self.emit_arm_body_or_guard(arm, arms, outer_scratch, subject_ty, result_ty, idx, is_last);
                }
                wasm!(self.func, { else_; });
                if is_last { wasm!(self.func, { unreachable; }); }
                else { self.emit_match_arms(arms, outer_scratch, subject_ty, result_ty, idx + 1); }
                self.depth_pop(some_guard);
                wasm!(self.func, { end; });
                self.scratch.free_i32(inner_scratch);
                true
            }
            IrPattern::None => {
                // Nested None: check inner ptr == 0
                let inner_scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_get(container_scratch);
                    i32_load(inner_offset);
                    local_set(inner_scratch);
                });
                wasm!(self.func, { local_get(inner_scratch); i32_eqz; });
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                let none_guard = self.depth_push();
                self.emit_match_arm_body(&arm.body, result_ty);
                wasm!(self.func, { else_; });
                if is_last { wasm!(self.func, { unreachable; }); }
                else { self.emit_match_arms(arms, outer_scratch, subject_ty, result_ty, idx + 1); }
                self.depth_pop(none_guard);
                wasm!(self.func, { end; });
                self.scratch.free_i32(inner_scratch);
                true
            }
            IrPattern::Ok { inner: inner2 } => {
                // Nested Ok: load inner, check tag == 0
                let inner_scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_get(container_scratch);
                    i32_load(inner_offset);
                    local_set(inner_scratch);
                });
                wasm!(self.func, {
                    local_get(inner_scratch);
                    i32_load(0);
                    i32_eqz;
                });
                let nested_ty = if let Ty::Applied(_, args) = inner_ty {
                    args.first().cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                let ok_guard = self.depth_push();
                let emitted = self.emit_inner_pattern_and_body(
                    inner2, inner_scratch, 4, &nested_ty,
                    arm, arms, outer_scratch, subject_ty, result_ty, idx, is_last,
                );
                if !emitted {
                    self.emit_arm_body_or_guard(arm, arms, outer_scratch, subject_ty, result_ty, idx, is_last);
                }
                wasm!(self.func, { else_; });
                if is_last { wasm!(self.func, { unreachable; }); }
                else { self.emit_match_arms(arms, outer_scratch, subject_ty, result_ty, idx + 1); }
                self.depth_pop(ok_guard);
                wasm!(self.func, { end; });
                self.scratch.free_i32(inner_scratch);
                true
            }
            IrPattern::Err { inner: inner2 } => {
                // Nested Err: load inner, check tag != 0
                let inner_scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_get(container_scratch);
                    i32_load(inner_offset);
                    local_set(inner_scratch);
                });
                wasm!(self.func, {
                    local_get(inner_scratch);
                    i32_load(0);
                    i32_const(0);
                    i32_ne;
                });
                let nested_ty = if let Ty::Applied(_, args) = inner_ty {
                    args.get(1).cloned().unwrap_or(Ty::String)
                } else { Ty::String };
                let bt = values::block_type(result_ty);
                self.func.instruction(&Instruction::If(bt));
                let err_guard = self.depth_push();
                let emitted = self.emit_inner_pattern_and_body(
                    inner2, inner_scratch, 4, &nested_ty,
                    arm, arms, outer_scratch, subject_ty, result_ty, idx, is_last,
                );
                if !emitted {
                    self.emit_arm_body_or_guard(arm, arms, outer_scratch, subject_ty, result_ty, idx, is_last);
                }
                wasm!(self.func, { else_; });
                if is_last { wasm!(self.func, { unreachable; }); }
                else { self.emit_match_arms(arms, outer_scratch, subject_ty, result_ty, idx + 1); }
                self.depth_pop(err_guard);
                wasm!(self.func, { end; });
                self.scratch.free_i32(inner_scratch);
                true
            }
            IrPattern::RecordPattern { name: ctor_name, fields: pat_fields, .. } => {
                // Inner variant with record payload (e.g., ok(Parsed { value, rest }))
                let inner_scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_get(container_scratch);
                    i32_load(inner_offset);
                    local_set(inner_scratch);
                });
                let tag = self.find_variant_tag_by_ctor(ctor_name, inner_ty);
                if let Some(tag_val) = tag {
                    wasm!(self.func, {
                        local_get(inner_scratch);
                        i32_load(0);
                        i32_const(tag_val as i32);
                        i32_eq;
                    });
                    let bt = values::block_type(result_ty);
                    self.func.instruction(&Instruction::If(bt));
                    let ctor_guard = self.depth_push();
                    // Bind named fields from the variant's record layout
                    let case_fields = self.emitter.fields_of(ctor_name.as_str());
                    for pf in pat_fields {
                        if let Some((foff, fty)) = values::field_offset(&case_fields, &pf.name) {
                            let total_offset = 4 + foff; // 4 = tag size
                            let local_idx = if let Some(almide_ir::IrPattern::Bind { var, .. }) = &pf.pattern {
                                self.var_map.get(&var.0).copied()
                            } else {
                                self.find_var_by_field(&pf.name, &case_fields).copied()
                            };
                            if let Some(idx) = local_idx {
                                wasm!(self.func, { local_get(inner_scratch); });
                                self.emit_load_at(&fty, total_offset);
                                wasm!(self.func, { local_set(idx); });
                            }
                        }
                    }
                    self.emit_arm_body_or_guard(arm, arms, outer_scratch, subject_ty, result_ty, idx, is_last);
                    wasm!(self.func, { else_; });
                    if is_last { wasm!(self.func, { unreachable; }); }
                    else { self.emit_match_arms(arms, outer_scratch, subject_ty, result_ty, idx + 1); }
                    self.depth_pop(ctor_guard);
                    wasm!(self.func, { end; });
                } else {
                    // Not a variant — plain record inside container. Bind fields directly.
                    let record_fields = self.extract_record_fields(inner_ty);
                    for pf in pat_fields {
                        if let Some((foff, fty)) = values::field_offset(&record_fields, &pf.name) {
                            let local_idx = if let Some(almide_ir::IrPattern::Bind { var, .. }) = &pf.pattern {
                                self.var_map.get(&var.0).copied()
                            } else {
                                self.find_var_by_field(&pf.name, &record_fields).copied()
                            };
                            if let Some(idx) = local_idx {
                                wasm!(self.func, { local_get(inner_scratch); });
                                self.emit_load_at(&fty, inner_offset + foff);
                                wasm!(self.func, { local_set(idx); });
                            }
                        }
                    }
                    self.scratch.free_i32(inner_scratch);
                    return false; // caller emits body
                }
                self.scratch.free_i32(inner_scratch);
                true
            }
            _ => false,
        }
    }
}
