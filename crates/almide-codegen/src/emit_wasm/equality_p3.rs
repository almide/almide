impl FuncCompiler<'_> {
    /// Emit a store instruction for a value at base_ptr + offset.
    /// Assumes base_ptr is already on stack, followed by the value.
    ///
    /// Narrow Almide sized types (Int8/Int16/UInt8/UInt16) ride in the
    /// WASM i32 bucket but occupy 1 or 2 bytes on the heap — we emit
    /// the width-matching `i32.store8` / `i32.store16` so adjacent
    /// fields don't overwrite. Same story for `i64.store8` / `_16` /
    /// `_32` on Int64 narrow writes (future path).
    pub fn emit_store_at(&mut self, ty: &Ty, offset: u32) {
        match ty {
            Ty::Int8 | Ty::UInt8 => { wasm!(self.func, { i32_store8(offset); }); }
            Ty::Int16 | Ty::UInt16 => { wasm!(self.func, { i32_store16(offset); }); }
            _ => match values::ty_to_valtype(ty) {
                Some(ValType::I64) => { wasm!(self.func, { i64_store(offset); }); }
                Some(ValType::F64) => { wasm!(self.func, { f64_store(offset); }); }
                Some(ValType::F32) => { wasm!(self.func, { f32_store(offset); }); }
                Some(ValType::I32) => { wasm!(self.func, { i32_store(offset); }); }
                _ => {}
            }
        }
    }

    /// Emit a load instruction from base_ptr (on stack) + offset.
    /// Narrow sized-int loads use the signed / unsigned variant
    /// matching the Almide type so the i32-bucket value carries the
    /// correct sign-extension / zero-extension for subsequent ops.
    pub fn emit_load_at(&mut self, ty: &Ty, offset: u32) {
        match ty {
            Ty::Int8  => { wasm!(self.func, { i32_load8_s(offset); }); }
            Ty::UInt8 => { wasm!(self.func, { i32_load8_u(offset); }); }
            Ty::Int16 => { wasm!(self.func, { i32_load16_s(offset); }); }
            Ty::UInt16 => { wasm!(self.func, { i32_load16_u(offset); }); }
            _ => match values::ty_to_valtype(ty) {
                Some(ValType::I64) => { wasm!(self.func, { i64_load(offset); }); }
                Some(ValType::F64) => { wasm!(self.func, { f64_load(offset); }); }
                Some(ValType::F32) => { wasm!(self.func, { f32_load(offset); }); }
                Some(ValType::I32) => { wasm!(self.func, { i32_load(offset); }); }
                _ => {}
            }
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
        extract_record_fields(ty, &self.emitter.record_fields, &self.emitter.variant_info)
    }

    /// Find local index for a pattern field binding by name.
    pub(super) fn find_var_by_field(&self, field_name: &str, _case_fields: &[(String, Ty)]) -> Option<&u32> {
        // Pick the SMALLEST matching VarId, not first-in-iteration: var_map is a
        // HashMap whose iteration order is host-pointer-width dependent, so a
        // first-match would choose a different local index on wasm32 (the
        // playground) vs x86-64 → a wrong-slot local.get → garbage read → trap.
        self.var_map.iter()
            .filter(|&(&var_id, _)| (var_id as usize) < self.var_table.len()
                && self.var_table.get(almide_ir::VarId(var_id)).name == field_name)
            .min_by_key(|&(&var_id, _)| var_id)
            .map(|(_, local_idx)| local_idx)
    }
}

impl FuncCompiler<'_> {
    /// Find variant tag for a unit constructor called as a function (e.g., `Red`).
    #[allow(dead_code)] // Will be used for WASM variant equality codegen
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

    /// Compute the allocation size for a variant constructor. All constructors
    /// of the same variant type are padded to the maximum size so that
    /// `mem_eq` can safely compare any two values of the type.
    pub(super) fn variant_alloc_size(&self, ctor_name: &str) -> u32 {
        for cases in self.emitter.variant_info.values() {
            if cases.iter().any(|c| c.name == ctor_name) {
                let max_payload = cases.iter()
                    .map(|c| super::values::record_size(&c.fields))
                    .max().unwrap_or(0);
                return 4 + max_payload; // tag + max payload
            }
        }
        4 // fallback: tag only
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
        let cases = cases?;
        cases.iter().find(|c| c.name == ctor_name).map(|c| c.tag)
    }
}

/// Extract field names and types from a record/named type.
///
/// Handles `Ty::Record`, `Ty::OpenRecord`, and `Ty::Named` with full generic
/// substitution via `variant_info`. This is the single canonical implementation;
/// `FuncCompiler::extract_record_fields` delegates here.
pub(super) fn extract_record_fields(
    ty: &Ty,
    record_fields: &BTreeMap<String, Vec<(String, Ty)>>,
    variant_info: &BTreeMap<String, Vec<VariantCase>>,
) -> Vec<(String, Ty)> {
    match ty {
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            fields.iter().map(|(n, t)| (n.to_string(), t.clone())).collect()
        }
        Ty::Named(name, type_args) => {
            // Try full qualified name first (e.g. "todoapp.Todo"), then bare name ("Todo").
            // Module-qualified types from submodules carry the prefix in the IR, but
            // record_fields is keyed by the declaration name (unprefixed).
            let fields_opt = record_fields.get(name.as_str()).or_else(|| {
                let bare = name.as_str().rsplit('.').next().unwrap_or(name.as_str());
                if bare != name.as_str() { record_fields.get(bare) } else { None }
            });
            if let Some(fields) = fields_opt {
                if type_args.is_empty() {
                    fields.clone()
                } else {
                    // Collect generic param names from ALL constructors of the variant type
                    // (not just this ctor) for correct index mapping.
                    // E.g., Either[A,B]: Left(A), Right(B) → gnames = ["A","B"], not just ["B"]
                    let mut generic_names: Vec<&str> = Vec::new();
                    if let Some(cases) = variant_info.get(name.as_str()) {
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
