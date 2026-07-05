/// Infer the type of a bind value from its IR expression structure.
/// Used when value.ty and stmt ty are both Unknown.
fn infer_bind_type(expr: &IrExpr) -> Ty {
    match &expr.kind {
        IrExprKind::LitInt { .. } => Ty::Int,
        IrExprKind::LitFloat { .. } => Ty::Float,
        IrExprKind::LitBool { .. } => Ty::Bool,
        IrExprKind::LitStr { .. } => Ty::String,
        // TupleIndex: infer from parent tuple type
        IrExprKind::TupleIndex { object, index } => {
            if let Ty::Tuple(elems) = &object.ty {
                elems.get(*index).cloned().unwrap_or(Ty::Unknown)
            } else {
                Ty::Unknown
            }
        }
        // BinOp: infer from operation kind
        IrExprKind::BinOp { op, .. } => op.result_ty().unwrap_or(Ty::Unknown),
        // Try/Unwrap/ToOption: unwrap inner type
        IrExprKind::Try { expr: inner }
        | IrExprKind::Unwrap { expr: inner }
        | IrExprKind::ToOption { expr: inner } => {
            infer_bind_type(inner)
        }
        IrExprKind::UnwrapOr { expr: inner, .. } => {
            infer_bind_type(inner)
        }
        // Call: infer return type from module+func name
        IrExprKind::Call { target, .. } => {
            match target {
                almide_ir::CallTarget::Module { module, func, .. } => {
                    match (module.as_str(), func.as_str()) {
                        ("random", "int") | ("datetime", _)
                        | ("env", "unix_timestamp") | ("env", "millis")
                        | ("list", "len") | ("string", "len") | ("map", "len") => Ty::Int,
                        ("random", "float") => Ty::Float,
                        _ => Ty::Unknown,
                    }
                }
                _ => Ty::Unknown,
            }
        }
        _ => Ty::Unknown,
    }
}

/// Result of pre-scanning a function body for local variables.
pub struct LocalScanResult {
    pub binds: Vec<(VarId, ValType)>,
}

/// Pre-scan a function body to collect all local variable bindings
/// and count scratch local depth.
pub fn collect_locals(
    body: &IrExpr,
    var_table: &almide_ir::VarTable,
    record_fields: &RecordFieldLookup,
    variant_info: &VariantInfoLookup,
) -> LocalScanResult {
    let mut binds = Vec::new();
    scan_expr(body, &mut binds, var_table, record_fields, variant_info);
    LocalScanResult { binds }
}

impl FuncCompiler<'_> {
    /// Bind every leaf of a let-destructure pattern, recursing into nested
    /// tuple/record sub-patterns. `base_local` holds the aggregate pointer for a
    /// Tuple/RecordPattern, or the scalar/pointer value for a `Bind`. Mirrors
    /// `scan_destructure_pattern` (which pre-allocates the leaf locals); without
    /// the recursion a nested sub-pattern left its leaves zeroed (#654).
    fn emit_bind_destructure(&mut self, pattern: &almide_ir::IrPattern, base_local: u32, base_ty: &Ty) {
        match pattern {
            almide_ir::IrPattern::Bind { var, .. } => {
                if let Some(&local_idx) = self.var_map.get(&var.0) {
                    wasm!(self.func, { local_get(base_local); local_set(local_idx); });
                }
            }
            almide_ir::IrPattern::Tuple { elements } => {
                let elem_types = if let Ty::Tuple(tys) = base_ty { tys.clone() } else { vec![] };
                let mut offset = 0u32;
                for (i, elem_pat) in elements.iter().enumerate() {
                    let elem_ty = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                    self.emit_destructure_elem(elem_pat, base_local, offset, &elem_ty);
                    offset += super::values::byte_size(&elem_ty);
                }
            }
            almide_ir::IrPattern::RecordPattern { fields, .. } => {
                let record_fields = self.extract_record_fields(base_ty);
                for pf in fields {
                    if let Some((offset, field_ty)) = super::values::field_offset(&record_fields, &pf.name) {
                        if let Some(sub) = &pf.pattern {
                            self.emit_destructure_elem(sub, base_local, offset, &field_ty);
                        } else if let Some(&local_idx) = self.find_var_by_field(&pf.name, &record_fields) {
                            wasm!(self.func, { local_get(base_local); });
                            self.emit_load_at(&field_ty, offset);
                            wasm!(self.func, { local_set(local_idx); });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Bind a sub-pattern living at `offset` from `base_local`. A leaf `Bind`
    /// loads the scalar/pointer directly; a nested aggregate loads its pointer
    /// and recurses with it as the new base.
    fn emit_destructure_elem(&mut self, pat: &almide_ir::IrPattern, base_local: u32, offset: u32, elem_ty: &Ty) {
        match pat {
            almide_ir::IrPattern::Bind { var, .. } => {
                if let Some(&local_idx) = self.var_map.get(&var.0) {
                    wasm!(self.func, { local_get(base_local); });
                    self.emit_load_at(elem_ty, offset);
                    wasm!(self.func, { local_set(local_idx); });
                }
            }
            almide_ir::IrPattern::Tuple { .. } | almide_ir::IrPattern::RecordPattern { .. } => {
                let sub = self.scratch.alloc_i32();
                wasm!(self.func, { local_get(base_local); });
                self.emit_load_at(elem_ty, offset);
                wasm!(self.func, { local_set(sub); });
                self.emit_bind_destructure(pat, sub, elem_ty);
                self.scratch.free_i32(sub);
            }
            _ => {}
        }
    }
}

// ── LocalScanner: IrVisitor-based local variable collector ──────────
//
// Collects all local variable bindings in a function body for WASM local
// allocation. Uses walk_expr/walk_stmt for exhaustive traversal; only
// overrides ForIn, Match (which register bindings) and Bind/BindDestructure.

struct LocalScanner<'a> {
    locals: &'a mut Vec<(VarId, ValType)>,
    vt: &'a almide_ir::VarTable,
    record_fields: &'a RecordFieldLookup,
    variant_info: &'a VariantInfoLookup,
}

impl IrVisitor for LocalScanner<'_> {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                let elem_ty = match &iterable.ty {
                    almide_lang::types::Ty::Applied(almide_lang::types::TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                    almide_lang::types::Ty::Applied(almide_lang::types::TypeConstructorId::Map, args) if args.len() == 2 =>
                        almide_lang::types::Ty::Tuple(vec![args[0].clone(), args[1].clone()]),
                    _ => self.vt.get(*var).ty.clone(),
                };
                self.locals.push((*var, values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I64)));
                if let Some(tuple_vars) = var_tuple {
                    for tv in tuple_vars {
                        let tv_type = values::ty_to_valtype(&self.vt.get(*tv).ty).unwrap_or(ValType::I64);
                        self.locals.push((*tv, tv_type));
                    }
                }
                self.visit_expr(iterable);
                for stmt in body { self.visit_stmt(stmt); }
            }
            IrExprKind::Match { subject, arms } => {
                self.visit_expr(subject);
                let resolved_ty = resolve_scan_subject_ty(subject, arms, self.vt);
                for arm in arms {
                    scan_pattern(&arm.pattern, &resolved_ty, self.locals, self.vt, self.record_fields, self.variant_info);
                    self.visit_expr(&arm.body);
                }
            }
            _ => walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &IrStmt) {
        match &stmt.kind {
            IrStmtKind::Bind { var, ty, value, .. } => {
                let effective_ty = if let IrExprKind::Try { expr: inner }
                    | IrExprKind::Unwrap { expr: inner } = &value.kind {
                    if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, args) = &value.ty {
                        args.first().cloned().unwrap_or(value.ty.clone())
                    } else if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, args) = &inner.ty {
                        args.first().cloned().unwrap_or(value.ty.clone())
                    } else {
                        value.ty.clone()
                    }
                } else {
                    value.ty.clone()
                };
                let resolved_ty = if !effective_ty.is_unresolved() {
                    effective_ty
                } else if !ty.is_unresolved() {
                    ty.clone()
                } else {
                    infer_bind_type(value)
                };
                if let Some(vt_wasm) = values::ty_to_valtype(&resolved_ty) {
                    self.locals.push((*var, vt_wasm));
                }
                self.visit_expr(value);
            }
            IrStmtKind::BindDestructure { pattern, value } => {
                scan_destructure_pattern(pattern, &value.ty, self.locals, self.vt, self.record_fields, self.variant_info);
                self.visit_expr(value);
            }
            _ => walk_stmt(self, stmt),
        }
    }
}

fn scan_expr(
    expr: &IrExpr,
    locals: &mut Vec<(VarId, ValType)>,
    vt: &almide_ir::VarTable,
    record_fields: &RecordFieldLookup,
    variant_info: &VariantInfoLookup,
) {
    LocalScanner { locals, vt, record_fields, variant_info }.visit_expr(expr);
}

/// Resolve match subject type, fixing IR type inference gaps.
fn resolve_scan_subject_ty(subject: &IrExpr, arms: &[almide_ir::IrMatchArm], vt: &almide_ir::VarTable) -> almide_lang::types::Ty {
    let has_container = arms.iter().any(|a| matches!(
        &a.pattern,
        almide_ir::IrPattern::Ok { .. } | almide_ir::IrPattern::Err { .. }
        | almide_ir::IrPattern::Some { .. } | almide_ir::IrPattern::None
    ));
    if has_container && !matches!(&subject.ty, almide_lang::types::Ty::Applied(_, _)) {
        if let IrExprKind::Var { id } = &subject.kind {
            let info = vt.get(*id);
            if matches!(&info.ty, almide_lang::types::Ty::Applied(_, _)) {
                return info.ty.clone();
            }
        }
    }
    subject.ty.clone()
}

/// Scan a destructuring pattern (let (a, b) = ...) for variable bindings.
fn scan_destructure_pattern(
    pattern: &almide_ir::IrPattern,
    value_ty: &almide_lang::types::Ty,
    locals: &mut Vec<(VarId, ValType)>,
    vt: &almide_ir::VarTable,
    record_fields: &RecordFieldLookup,
    variant_info: &VariantInfoLookup,
) {
    match pattern {
        almide_ir::IrPattern::Tuple { elements } => {
            let elem_types = if let almide_lang::types::Ty::Tuple(tys) = value_ty { tys.clone() } else { vec![] };
            for (i, elem) in elements.iter().enumerate() {
                let elem_ty = elem_types.get(i).cloned().unwrap_or(almide_lang::types::Ty::Int);
                scan_destructure_pattern(elem, &elem_ty, locals, vt, record_fields, variant_info);
            }
        }
        almide_ir::IrPattern::Bind { var, .. } => {
            if let Some(val_type) = values::ty_to_valtype(value_ty) {
                locals.push((*var, val_type));
            }
        }
        almide_ir::IrPattern::RecordPattern { fields, .. } => {
            // Record destructure: resolve field types from value_ty (authoritative).
            // Uses extract_record_fields for full generic substitution.
            let resolved_fields = extract_record_fields(value_ty, record_fields, variant_info);
            let existing_ids: std::collections::HashSet<u32> = locals.iter().map(|(v, _)| v.0).collect();
            for field in fields {
                // Resolve field type from the record type (not VarTable -- VarTable may have stale types)
                let field_ty = resolved_fields.iter()
                    .find(|(n, _)| n == &field.name)
                    .map(|(_, t)| t.clone())
                    .unwrap_or(almide_lang::types::Ty::Int);
                if let Some(pat) = &field.pattern {
                    scan_destructure_pattern(pat, &field_ty, locals, vt, record_fields, variant_info);
                } else {
                    // Implicit bind: field name = var name. Look up VarId from VarTable.
                    // Use field_ty from the record type for the WASM local declaration.
                    for i in (0..vt.len()).rev() {
                        let info = vt.get(almide_ir::VarId(i as u32));
                        if info.name == field.name && !existing_ids.contains(&(i as u32)) {
                            if let Some(val_type) = values::ty_to_valtype(&field_ty) {
                                locals.push((almide_ir::VarId(i as u32), val_type));
                            }
                            break;
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Scan a match pattern for variable bindings.
fn scan_pattern(
    pattern: &almide_ir::IrPattern,
    subject_ty: &almide_lang::types::Ty,
    locals: &mut Vec<(VarId, ValType)>,
    vt: &almide_ir::VarTable,
    record_fields: &RecordFieldLookup,
    variant_info: &VariantInfoLookup,
) {
    match pattern {
        almide_ir::IrPattern::Bind { var, ty } => {
            // Use pattern's own type (set by lowering, updated by mono) — no VarTable dependency
            let effective_ty = if matches!(ty, almide_lang::types::Ty::Unknown) { subject_ty } else { ty };
            if let Some(val_type) = values::ty_to_valtype(effective_ty) {
                locals.push((*var, val_type));
            }
        }
        almide_ir::IrPattern::Constructor { name: _ctor_name, args } => {
            // Resolve field types from subject_ty's type_args for generic variants
            let subject_type_args: Vec<almide_lang::types::Ty> = match subject_ty {
                almide_lang::types::Ty::Named(_, args) if !args.is_empty() => args.clone(),
                almide_lang::types::Ty::Applied(_, args) if !args.is_empty() => args.clone(),
                almide_lang::types::Ty::Variant { .. } => {
                    // Use pattern.ty (set by mono substitute_pattern_types) — no VarTable
                    for arg in args.iter() {
                        if let almide_ir::IrPattern::Bind { var, ty } = arg {
                            let effective_ty = if ty.is_unresolved() {
                                &vt.get(*var).ty // fallback only
                            } else { ty };
                            if let Some(val_type) = values::ty_to_valtype(effective_ty) {
                                locals.push((*var, val_type));
                            }
                        }
                    }
                    return;
                }
                _ => vec![],
            };
            for (_i, arg) in args.iter().enumerate() {
                if let almide_ir::IrPattern::Bind { var, ty: pat_ty } = arg {
                    // Use pattern.ty first (set by mono), fall back to VarTable + substitution
                    let resolved = if !pat_ty.is_unresolved()
                        && !matches!(pat_ty, almide_lang::types::Ty::Named(n, a) if a.is_empty() && n.len() <= 2 && n.chars().next().map_or(false, |c| c.is_uppercase()))
                    {
                        pat_ty.clone()
                    } else if !subject_type_args.is_empty() {
                        let var_ty = vt.get(*var).ty.clone();
                        let mut gnames = Vec::new();
                        super::expressions::collect_type_param_names(&var_ty, &mut gnames);
                        if gnames.is_empty() { var_ty } else {
                            super::expressions::substitute_type_params(&var_ty, &gnames, &subject_type_args)
                        }
                    } else { vt.get(*var).ty.clone() };
                    if let Some(val_type) = values::ty_to_valtype(&resolved) {
                        locals.push((*var, val_type));
                    }
                } else {
                    scan_pattern(arg, subject_ty, locals, vt, record_fields, variant_info);
                }
            }
        }
        almide_ir::IrPattern::Tuple { elements } => {
            let elem_types = if let almide_lang::types::Ty::Tuple(tys) = subject_ty { tys.clone() } else { vec![] };
            for (i, elem) in elements.iter().enumerate() {
                let et = elem_types.get(i).cloned().unwrap_or(subject_ty.clone());
                scan_pattern(elem, &et, locals, vt, record_fields, variant_info);
            }
        }
        almide_ir::IrPattern::Some { inner } | almide_ir::IrPattern::Ok { inner } => {
            let inner_ty = if let almide_lang::types::Ty::Applied(_, args) = subject_ty {
                args.first().cloned().unwrap_or(subject_ty.clone())
            } else {
                // subject_ty is not Applied — try VarTable for inner binding
                if let almide_ir::IrPattern::Bind { var, .. } = inner.as_ref() {
                    let vt_ty = &vt.get(*var).ty;
                    if !vt_ty.is_unresolved() {
                        vt_ty.clone()
                    } else { subject_ty.clone() }
                } else { subject_ty.clone() }
            };
            scan_pattern(inner, &inner_ty, locals, vt, record_fields, variant_info);
        }
        almide_ir::IrPattern::Err { inner } => {
            let inner_ty = if let almide_lang::types::Ty::Applied(_, args) = subject_ty {
                args.get(1).cloned().unwrap_or(subject_ty.clone())
            } else {
                if let almide_ir::IrPattern::Bind { var, .. } = inner.as_ref() {
                    let vt_ty = &vt.get(*var).ty;
                    if !vt_ty.is_unresolved() {
                        vt_ty.clone()
                    } else { subject_ty.clone() }
                } else { subject_ty.clone() }
            };
            scan_pattern(inner, &inner_ty, locals, vt, record_fields, variant_info);
        }
        almide_ir::IrPattern::RecordPattern { name: _, fields, .. } => {
            // For pattern=None fields, the binding is implicit (field name = var name).
            // The lowerer has already allocated VarIds for these in the VarTable.
            // Search from the END of VarTable to find the most recent (correct scope) VarId,
            // and skip VarIds already registered in locals to avoid duplicates.
            // Resolve field types from subject_ty (structural or nominal) so local
            // valtypes match the actual value layout — falls back to VarTable only
            // when the record type cannot be resolved.
            let resolved_fields = extract_record_fields(subject_ty, record_fields, variant_info);
            let existing_ids: std::collections::HashSet<u32> = locals.iter().map(|(v, _)| v.0).collect();
            for field in fields {
                if let Some(pat) = &field.pattern {
                    scan_pattern(pat, subject_ty, locals, vt, record_fields, variant_info);
                } else {
                    // Implicit bind: find VarId by field name, searching from end (most recent scope)
                    for i in (0..vt.len()).rev() {
                        let info = vt.get(almide_ir::VarId(i as u32));
                        if info.name == field.name && !existing_ids.contains(&(i as u32)) {
                            let field_ty = resolved_fields.iter()
                                .find(|(n, _)| n == &field.name)
                                .map(|(_, t)| t.clone())
                                .unwrap_or_else(|| info.ty.clone());
                            if let Some(val_type) = values::ty_to_valtype(&field_ty) {
                                locals.push((almide_ir::VarId(i as u32), val_type));
                            }
                            break;
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Collect all VarId references in an expression.
fn collect_var_refs(expr: &IrExpr, refs: &mut std::collections::HashSet<VarId>) {
    struct VarCollector<'a> { refs: &'a mut std::collections::HashSet<VarId> }
    impl IrVisitor for VarCollector<'_> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Var { id } = &expr.kind { self.refs.insert(*id); }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) { walk_stmt(self, stmt); }
    }
    VarCollector { refs }.visit_expr(expr);
}

/// Collect all VarId references in a statement.
fn collect_stmt_var_refs(stmt: &IrStmt, refs: &mut std::collections::HashSet<VarId>) {
    struct VarCollector<'a> { refs: &'a mut std::collections::HashSet<VarId> }
    impl IrVisitor for VarCollector<'_> {
        fn visit_expr(&mut self, expr: &IrExpr) {
            if let IrExprKind::Var { id } = &expr.kind { self.refs.insert(*id); }
            walk_expr(self, expr);
        }
        fn visit_stmt(&mut self, stmt: &IrStmt) { walk_stmt(self, stmt); }
    }
    VarCollector { refs }.visit_stmt(stmt);
}

/// EARLY-RETURN LEAK FIX: a `__tco_`/`__br_`/`__perceus_` temp DONATES its reference
/// (it never gets its own scope-end dec — see pass_perceus.rs:309-315/414-418), so it
/// must NOT be tracked as an owned heap local: decing one on an early-return would free
/// a donated ref (double-free). The prefix set is the union of the move-exempt and
/// terminal-dec-skip families, conservatively broad (excluding extra never double-frees;
/// the worst case is a bounded leak of a transient temp).
pub(super) fn is_donate_temp(name: &str) -> bool {
    name.starts_with("__tco_") || name.starts_with("__br_") || name.starts_with("__perceus_")
}
