//! Deep equality comparison for WASM codegen.
//!
//! Type-aware recursive equality for List, Option, Result, Tuple, Record, Variant.

use super::FuncCompiler;
use super::values;
use crate::ir::IrExpr;
use crate::types::Ty;
use wasm_encoder::ValType;
use super::expressions::CmpKind;

impl FuncCompiler<'_> {
    pub(super) fn emit_eq(&mut self, left: &IrExpr, right: &IrExpr, negate: bool) {
        self.emit_expr(left);
        self.emit_expr(right);
        // Use the more specific type for comparison dispatch.
        // Try to infer type from IR expression structure when both sides are Unknown
        let inferred = self.infer_expr_ty(left).or_else(|| self.infer_expr_ty(right));
        let cmp_ty = match (&left.ty, &right.ty) {
            (Ty::Unknown, Ty::Unknown) | (Ty::TypeVar(_), Ty::TypeVar(_))
            | (Ty::Unknown, Ty::TypeVar(_)) | (Ty::TypeVar(_), Ty::Unknown) => {
                if let Some(ref t) = inferred { t } else { &left.ty }
            }
            (Ty::Unknown, _) | (Ty::TypeVar(_), _) => &right.ty,
            (_, Ty::Unknown) | (_, Ty::TypeVar(_)) => &left.ty,
            (l, r) if !Self::is_compound_ty(l) && Self::is_compound_ty(r) => r,
            _ => &left.ty,
        };
        self.emit_eq_typed(cmp_ty);
        if negate {
            wasm!(self.func, { i32_eqz; });
        }
    }

    /// Emit type-aware equality for two values on stack. Consumes [a, b], produces i32.
    /// Recursive: handles nested containers correctly.
    pub(super) fn emit_eq_typed(&mut self, ty: &Ty) {
        use crate::types::constructor::TypeConstructorId;
        match ty {
            Ty::Int => { wasm!(self.func, { i64_eq; }); }
            Ty::Float => { wasm!(self.func, { f64_eq; }); }
            Ty::Bool => { wasm!(self.func, { i32_eq; }); }
            Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }

            Ty::Applied(TypeConstructorId::List, args) => {
                let elem_ty = args.first().cloned().unwrap_or(Ty::Int);
                // If elem is a value type (no pointers), use byte comparison
                if self.is_value_type(&elem_ty) {
                    let elem_size = values::byte_size(&elem_ty);
                    wasm!(self.func, {
                        i32_const(elem_size as i32);
                        call(self.emitter.rt.list_eq);
                    });
                } else {
                    // Deep list equality: compare element by element
                    self.emit_list_eq_deep(&elem_ty);
                }
            }

            Ty::Applied(TypeConstructorId::Option, args) => {
                let inner_ty = args.first().cloned().unwrap_or(Ty::Int);
                self.emit_option_eq_deep(&inner_ty);
            }

            Ty::Applied(TypeConstructorId::Result, args) => {
                let ok_ty = args.first().cloned().unwrap_or(Ty::Int);
                let err_ty = args.get(1).cloned().unwrap_or(Ty::String);
                self.emit_result_eq_deep(&ok_ty, &err_ty);
            }

            Ty::Tuple(elems) => {
                if elems.iter().all(|t| self.is_value_type(t)) {
                    let size: u32 = elems.iter().map(|t| values::byte_size(t)).sum();
                    wasm!(self.func, { i32_const(size as i32); call(self.emitter.rt.mem_eq); });
                } else {
                    self.emit_tuple_eq_deep(elems);
                }
            }

            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                if fields.iter().all(|(_, t)| self.is_value_type(t)) {
                    let size = values::record_size(fields);
                    wasm!(self.func, { i32_const(size as i32); call(self.emitter.rt.mem_eq); });
                } else {
                    // Field-by-field deep equality
                    self.emit_record_eq_deep(fields);
                }
            }

            Ty::Named(name, type_args) => {
                if let Some(cases) = self.emitter.variant_info.get(name.as_str()).cloned() {
                    let has_pointers = cases.iter().any(|c| c.fields.iter().any(|(_, ft)| !self.is_value_type(ft)));
                    if has_pointers {
                        // Use pre-registered eq function (handles recursion safely)
                        if let Some(&eq_idx) = self.emitter.eq_funcs.get(name.as_str()) {
                            wasm!(self.func, { call(eq_idx); });
                        } else {
                            // Fallback: inline deep comparison (non-recursive types)
                            self.emit_variant_eq_deep(&cases, type_args);
                        }
                    } else {
                        let max_payload = cases.iter()
                            .map(|c| values::record_size(&c.fields))
                            .max().unwrap_or(0);
                        let size = 4 + max_payload;
                        wasm!(self.func, { i32_const(size as i32); call(self.emitter.rt.mem_eq); });
                    }
                } else {
                    let fields = self.emitter.record_fields.get(name.as_str()).cloned().unwrap_or_default();
                    if fields.iter().any(|(_, ft)| !self.is_value_type(ft)) {
                        self.emit_record_eq_deep(&fields);
                    } else {
                        let size = values::record_size(&fields);
                        if size > 0 {
                            wasm!(self.func, { i32_const(size as i32); call(self.emitter.rt.mem_eq); });
                        } else {
                            wasm!(self.func, { i32_eq; });
                        }
                    }
                }
            }

            Ty::Variant { name, cases, .. } => {
                let has_pointers = cases.iter().any(|c| {
                    match &c.payload {
                        crate::types::VariantPayload::Tuple(ts) => ts.iter().any(|t| !self.is_value_type(t)),
                        crate::types::VariantPayload::Record(fs) => fs.iter().any(|(_, t, _)| !self.is_value_type(t)),
                        _ => false,
                    }
                });
                if has_pointers {
                    // Use pre-registered eq function if available
                    if let Some(&eq_idx) = self.emitter.eq_funcs.get(name.as_str()) {
                        wasm!(self.func, { call(eq_idx); });
                    } else {
                        // Fallback: inline (non-recursive types without pre-registration)
                        let case_infos: Vec<super::VariantCase> = cases.iter().enumerate().map(|(i, c)| {
                            let fields: Vec<(String, Ty)> = match &c.payload {
                                crate::types::VariantPayload::Tuple(ts) =>
                                    ts.iter().enumerate().map(|(j, t)| (format!("_{}", j), t.clone())).collect(),
                                crate::types::VariantPayload::Record(fs) =>
                                    fs.iter().map(|(n, t, _)| (n.clone(), t.clone())).collect(),
                                _ => vec![],
                            };
                            super::VariantCase { name: c.name.clone(), tag: i as u32, fields }
                        }).collect();
                        self.emit_variant_eq_deep(&case_infos, &[]);
                    }
                } else {
                    let max_payload: u32 = cases.iter()
                        .map(|c| match &c.payload {
                            crate::types::VariantPayload::Unit => 0,
                            crate::types::VariantPayload::Tuple(ts) => ts.iter().map(|t| values::byte_size(t)).sum(),
                            crate::types::VariantPayload::Record(fs) => fs.iter().map(|(_, t, _)| values::byte_size(t)).sum(),
                        })
                        .max().unwrap_or(0);
                    let size = 4 + max_payload;
                    wasm!(self.func, { i32_const(size as i32); call(self.emitter.rt.mem_eq); });
                }
            }

            _ => { wasm!(self.func, { i32_eq; }); }
        }
    }

    /// True if type is stored inline (no heap pointers that need deep comparison).
    fn is_value_type(&self, ty: &Ty) -> bool {
        matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit)
    }

    fn is_compound_ty(ty: &Ty) -> bool {
        matches!(ty, Ty::Named(_, _) | Ty::Applied(_, _) | Ty::Variant { .. }
            | Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Tuple(_) | Ty::String)
    }

    /// Try to infer a concrete type from an IR expression when expr.ty is Unknown.
    fn infer_expr_ty(&self, expr: &IrExpr) -> Option<Ty> {
        use crate::ir::IrExprKind;
        match &expr.kind {
            IrExprKind::TupleIndex { object, index } => {
                // Try to get the tuple type from the object, then extract element type
                let obj_ty = if matches!(&object.ty, Ty::Unknown | Ty::TypeVar(_)) {
                    // Try VarTable
                    if let IrExprKind::Var { id } = &object.kind {
                        if (id.0 as usize) < self.var_table.len() {
                            let info = self.var_table.get(*id);
                            if !matches!(&info.ty, Ty::Unknown | Ty::TypeVar(_)) {
                                Some(info.ty.clone())
                            } else { None }
                        } else { None }
                    } else { None }
                } else {
                    Some(object.ty.clone())
                };
                if let Some(Ty::Tuple(elems)) = obj_ty {
                    elems.get(*index as usize).cloned()
                } else { None }
            }
            IrExprKind::Var { id } => {
                if (id.0 as usize) < self.var_table.len() {
                    let info = self.var_table.get(*id);
                    if !matches!(&info.ty, Ty::Unknown | Ty::TypeVar(_)) {
                        Some(info.ty.clone())
                    } else { None }
                } else { None }
            }
            _ => None,
        }
    }

    /// Deep list equality: [a_ptr, b_ptr] → i32
    fn emit_list_eq_deep(&mut self, elem_ty: &Ty) {
        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let elem_size = values::byte_size(elem_ty);
        wasm!(self.func, {
            local_set(b); // b
            local_set(a); // a
            // Same pointer → true
            local_get(a); local_get(b); i32_eq;
            if_i32; i32_const(1);
            else_;
              // Different lengths → false
              local_get(a); i32_load(0);
              local_get(b); i32_load(0);
              i32_ne;
              if_i32; i32_const(0);
              else_;
                // Compare element by element
                i32_const(0); local_set(i); // i
                block_empty; loop_empty;
                  local_get(i); local_get(a); i32_load(0); i32_ge_u; br_if(1);
                  // Load a[i]
                  local_get(a); i32_const(4); i32_add;
                  local_get(i); i32_const(elem_size as i32); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        // Load b[i]
        wasm!(self.func, {
                  local_get(b); i32_const(4); i32_add;
                  local_get(i); i32_const(elem_size as i32); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        // Compare elements (recursive)
        let elem_ty_clone = elem_ty.clone();
        self.emit_eq_typed(&elem_ty_clone);
        wasm!(self.func, {
                  i32_eqz; // not equal?
                  if_empty;
                    i32_const(0); return_;
                  end;
                  local_get(i); i32_const(1); i32_add; local_set(i);
                  br(0);
                end; end;
                // All elements matched
                i32_const(1);
              end;
            end;
        });
        self.scratch.free_i32(i);
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    /// Deep option equality: [a_ptr, b_ptr] → i32
    fn emit_option_eq_deep(&mut self, inner_ty: &Ty) {
        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(b); // b
            local_set(a); // a
            // Both none → true
            local_get(a); i32_eqz; local_get(b); i32_eqz; i32_and;
            if_i32; i32_const(1);
            else_;
              // One none → false
              local_get(a); i32_eqz; local_get(b); i32_eqz; i32_or;
              if_i32; i32_const(0);
              else_;
                // Both some: compare inner values
                local_get(a);
        });
        self.emit_load_at(inner_ty, 0);
        wasm!(self.func, { local_get(b); });
        self.emit_load_at(inner_ty, 0);
        let inner_clone = inner_ty.clone();
        self.emit_eq_typed(&inner_clone);
        wasm!(self.func, {
              end;
            end;
        });
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    /// Deep result equality: [a_ptr, b_ptr] → i32
    fn emit_result_eq_deep(&mut self, ok_ty: &Ty, err_ty: &Ty) {
        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(b); // b
            local_set(a); // a
            // Tags must match
            local_get(a); i32_load(0);
            local_get(b); i32_load(0);
            i32_ne;
            if_i32; i32_const(0);
            else_;
              // Same tag. If tag==0 (ok): compare ok values
              local_get(a); i32_load(0); i32_eqz;
              if_i32;
                local_get(a);
        });
        self.emit_load_at(ok_ty, 4);
        wasm!(self.func, { local_get(b); });
        self.emit_load_at(ok_ty, 4);
        let ok_clone = ok_ty.clone();
        self.emit_eq_typed(&ok_clone);
        wasm!(self.func, {
              else_;
                // tag==1 (err): compare err values
                local_get(a);
        });
        self.emit_load_at(err_ty, 4);
        wasm!(self.func, { local_get(b); });
        self.emit_load_at(err_ty, 4);
        let err_clone = err_ty.clone();
        self.emit_eq_typed(&err_clone);
        wasm!(self.func, {
              end;
            end;
        });
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    /// Deep tuple equality: [a_ptr, b_ptr] → i32
    fn emit_tuple_eq_deep(&mut self, elems: &[Ty]) {
        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(b); // b
            local_set(a); // a
        });
        // Compare each field, short-circuit on mismatch
        let mut offset: u32 = 0;
        for (i, elem_ty) in elems.iter().enumerate() {
            let elem_size = values::byte_size(elem_ty);
            wasm!(self.func, { local_get(a); });
            self.emit_load_at(elem_ty, offset);
            wasm!(self.func, { local_get(b); });
            self.emit_load_at(elem_ty, offset);
            let elem_clone = elem_ty.clone();
            self.emit_eq_typed(&elem_clone);
            if i < elems.len() - 1 {
                // Short-circuit: if not equal, return 0
                wasm!(self.func, { i32_eqz; if_empty; i32_const(0); return_; end; });
            }
            offset += elem_size;
        }
        // If we reach here, all fields matched. Last comparison result is on stack.
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    /// Deep record equality: [a_ptr, b_ptr] → i32
    fn emit_record_eq_deep(&mut self, fields: &[(std::string::String, Ty)]) {
        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(b);
            local_set(a);
        });
        let mut offset: u32 = 0;
        for (i, (_, field_ty)) in fields.iter().enumerate() {
            let field_size = values::byte_size(field_ty);
            wasm!(self.func, { local_get(a); });
            self.emit_load_at(field_ty, offset);
            wasm!(self.func, { local_get(b); });
            self.emit_load_at(field_ty, offset);
            let field_clone = field_ty.clone();
            self.emit_eq_typed(&field_clone);
            if i < fields.len() - 1 {
                wasm!(self.func, { i32_eqz; if_empty; i32_const(0); return_; end; });
            }
            offset += field_size;
        }
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    /// Deep variant equality: [a_ptr, b_ptr] → i32
    /// Compares tag, then if tags match, compares payload fields deeply.
    fn emit_variant_eq_deep(&mut self, cases: &[super::VariantCase], _type_args: &[Ty]) {
        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(b);
            local_set(a);
            // Compare tags
            local_get(a); i32_load(0);
            local_get(b); i32_load(0);
            i32_ne;
            if_empty;
              i32_const(0); return_; // different tags → not equal
            end;
        });

        if cases.is_empty() || cases.iter().all(|c| c.fields.is_empty()) {
            // All unit variants — tags matched, so equal
            wasm!(self.func, { i32_const(1); });
        } else {
            // Compare payload fields based on tag
            // For simplicity: compare each field at its offset, starting after tag (offset 4)
            // Use the max-payload approach but with deep comparison
            // Build a union of all field types across cases, and compare field-by-field
            // For correctness, we iterate the longest case and compare each field deeply
            let max_case = cases.iter().max_by_key(|c| c.fields.len()).cloned();
            if let Some(case) = max_case {
                let mut offset = 4u32;
                for (i, (_, field_ty)) in case.fields.iter().enumerate() {
                    let field_size = values::byte_size(field_ty);
                    wasm!(self.func, { local_get(a); });
                    self.emit_load_at(field_ty, offset);
                    wasm!(self.func, { local_get(b); });
                    self.emit_load_at(field_ty, offset);
                    let ft = field_ty.clone();
                    self.emit_eq_typed(&ft);
                    if i < case.fields.len() - 1 {
                        wasm!(self.func, { i32_eqz; if_empty; i32_const(0); return_; end; });
                    }
                    offset += field_size;
                }
            } else {
                wasm!(self.func, { i32_const(1); });
            }
        }

        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    pub(super) fn emit_cmp_instruction(&mut self, ty: &Ty, kind: CmpKind) {
        match (ty, kind) {
            (Ty::Int, CmpKind::Lt) => { wasm!(self.func, { i64_lt_s; }); }
            (Ty::Int, CmpKind::Gt) => { wasm!(self.func, { i64_gt_s; }); }
            (Ty::Int, CmpKind::Lte) => { wasm!(self.func, { i64_le_s; }); }
            (Ty::Int, CmpKind::Gte) => { wasm!(self.func, { i64_ge_s; }); }
            (Ty::Float, CmpKind::Lt) => { wasm!(self.func, { f64_lt; }); }
            (Ty::Float, CmpKind::Gt) => { wasm!(self.func, { f64_gt; }); }
            (Ty::Float, CmpKind::Lte) => { wasm!(self.func, { f64_le; }); }
            (Ty::Float, CmpKind::Gte) => { wasm!(self.func, { f64_ge; }); }
            (Ty::String, CmpKind::Lt) => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(0); i32_lt_s; });
            }
            (Ty::String, CmpKind::Gt) => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(0); i32_gt_s; });
            }
            (Ty::String, CmpKind::Lte) => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(0); i32_le_s; });
            }
            (Ty::String, CmpKind::Gte) => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(0); i32_ge_s; });
            }
            _ => { wasm!(self.func, { unreachable; }); }
        }
    }

    /// Emit a store instruction for a value at base_ptr + offset.
    /// Assumes base_ptr is already on stack, followed by the value.
    pub fn emit_store_at(&mut self, ty: &Ty, offset: u32) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => {
                wasm!(self.func, { i64_store(offset); });
            }
            Some(ValType::F64) => {
                wasm!(self.func, { f64_store(offset); });
            }
            Some(ValType::I32) => {
                wasm!(self.func, { i32_store(offset); });
            }
            _ => {}
        }
    }

    /// Emit a load instruction from base_ptr (on stack) + offset.
    pub fn emit_load_at(&mut self, ty: &Ty, offset: u32) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => {
                wasm!(self.func, { i64_load(offset); });
            }
            Some(ValType::F64) => {
                wasm!(self.func, { f64_load(offset); });
            }
            Some(ValType::I32) => {
                wasm!(self.func, { i32_load(offset); });
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
            Ty::Record { fields } | Ty::OpenRecord { fields } => fields.clone(),
            Ty::Named(name, type_args) => {
                if let Some(fields) = self.emitter.record_fields.get(name.as_str()) {
                    if type_args.is_empty() {
                        fields.clone()
                    } else {
                        // Collect generic param names from ALL constructors of the variant type
                        // (not just this ctor) for correct index mapping.
                        // E.g., Either[A,B]: Left(A), Right(B) → gnames = ["A","B"], not just ["B"]
                        let mut generic_names: Vec<&str> = Vec::new();
                        if let Some(cases) = self.emitter.variant_info.get(name.as_str()) {
                            for case in cases {
                                for (_, fty) in &case.fields {
                                    super::expressions::collect_type_param_names(fty, &mut generic_names);
                                }
                            }
                        }
                        if generic_names.is_empty() {
                            // Fallback: collect from this ctor's fields only (non-variant records)
                            for (_, fty) in fields {
                                super::expressions::collect_type_param_names(fty, &mut generic_names);
                            }
                        }
                        fields.iter().map(|(fname, fty)| {
                            let resolved = super::expressions::substitute_type_params(fty, &generic_names, type_args);
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
            _ => {
                // Fallback: search all variant_info for the constructor
                for cases in self.emitter.variant_info.values() {
                    if let Some(c) = cases.iter().find(|c| c.name == ctor_name) {
                        return Some(c.tag);
                    }
                }
                return None;
            }
        };
        let cases = self.emitter.variant_info.get(type_name);
        if cases.is_none() && ctor_name == "Just" {
            eprintln!("[TAG MISS] ctor='{}' type='{}' variant_info_keys={:?}", ctor_name, type_name, self.emitter.variant_info.keys().collect::<Vec<_>>());
        }
        let cases = cases?;
        cases.iter().find(|c| c.name == ctor_name).map(|c| c.tag)
    }
}
