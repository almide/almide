//! Collection construction and access emission — records, lists, tuples, maps.

use almide_ir::{IrExpr, IrExprKind};
use almide_lang::types::Ty;
use super::FuncCompiler;
use super::values;

impl FuncCompiler<'_> {
    /// Emit a record/variant construction: allocate memory, store fields.
    /// For variants (detected via name + type), prepends a tag i32 before fields.
    pub(super) fn emit_record(&mut self, name: Option<&str>, fields: &[(almide_base::intern::Sym, IrExpr)], result_ty: &Ty) {
        // Check if this is a variant constructor
        let tag = self.resolve_variant_tag(name, result_ty);
        let tag_size: u32 = if tag.is_some() { 4 } else { 0 };

        // Compute total size from type definition (includes defaults)
        let type_field_size: u32 = if let Some(ctor_name) = name {
            let mut size = 0u32;
            // Check variant cases
            for cases in self.emitter.variant_info.values() {
                for case in cases {
                    if case.name == ctor_name {
                        size = case.fields.iter().map(|(_, ty)| values::byte_size(ty)).sum();
                    }
                }
            }
            if size == 0 {
                if let Some(rf) = self.emitter.record_fields.get(ctor_name) {
                    size = rf.iter().map(|(_, ty)| values::byte_size(ty)).sum();
                }
            }
            size
        } else { 0 };
        let explicit_size: u32 = fields.iter().map(|(_, e)| values::byte_size(&e.ty)).sum();
        let total_size = tag_size + if type_field_size > 0 { type_field_size } else { explicit_size };

        // Allocate
        wasm!(self.func, {
            i32_const(total_size as i32);
            call(self.emitter.rt.alloc);
        });

        let scratch = self.scratch.alloc_i32();
        wasm!(self.func, { local_set(scratch); });

        // Write tag if variant
        if let Some(tag_val) = tag {
            wasm!(self.func, {
                local_get(scratch);
                i32_const(tag_val as i32);
                i32_store(0);
            });
        }

        // Build merged field list in type-definition order
        // Explicit fields + defaults, ordered by type definition
        let explicit_map: std::collections::HashMap<&str, &IrExpr> =
            fields.iter().map(|(n, e)| (n.as_str(), e)).collect();

        // Get type-definition field order from variant_info or record_fields
        let type_fields: Vec<(String, Ty)> = if let Some(ctor_name) = name {
            // Try variant case
            let mut found = Vec::new();
            for cases in self.emitter.variant_info.values() {
                for case in cases {
                    if case.name == ctor_name {
                        found = case.fields.clone();
                    }
                }
            }
            if found.is_empty() {
                if let Some(rf) = self.emitter.record_fields.get(ctor_name) {
                    found = rf.clone();
                }
            }
            found
        } else { vec![] };

        let mut offset = tag_size;
        if !type_fields.is_empty() {
            // Emit in type-definition order
            for (field_name, field_ty) in &type_fields {
                wasm!(self.func, { local_get(scratch); });
                if let Some(expr) = explicit_map.get(field_name.as_str()) {
                    self.emit_expr(expr);
                    // Use type-definition type when expr.ty is Unknown
                    let store_ty = if matches!(&expr.ty, Ty::Unknown | Ty::TypeVar(_)) {
                        field_ty
                    } else {
                        &expr.ty
                    };
                    self.emit_store_at(store_ty, offset);
                    offset += values::byte_size(store_ty);
                } else if let Some(ctor_name) = name {
                    if let Some(default_expr) = self.emitter.default_fields.get(&(ctor_name.to_string(), field_name.clone())) {
                        let default_expr = default_expr.clone();
                        let dt = match (&default_expr.ty, &default_expr.kind) {
                            (Ty::Unknown, almide_ir::IrExprKind::LitInt { .. }) => Ty::Int,
                            (Ty::Unknown, almide_ir::IrExprKind::LitFloat { .. }) => Ty::Float,
                            (Ty::Unknown, almide_ir::IrExprKind::LitBool { .. }) => Ty::Bool,
                            (Ty::Unknown, almide_ir::IrExprKind::LitStr { .. }) => Ty::String,
                            _ => default_expr.ty.clone(),
                        };
                        self.emit_expr(&default_expr);
                        self.emit_store_at(&dt, offset);
                        offset += values::byte_size(&dt);
                    } else {
                        // No value — zero-fill
                        offset += values::byte_size(field_ty);
                    }
                } else {
                    offset += values::byte_size(field_ty);
                }
            }
        } else {
            // No type info: emit explicit fields only
            for (_, field_expr) in fields {
                let field_size = values::byte_size(&field_expr.ty);
                wasm!(self.func, { local_get(scratch); });
                self.emit_expr(field_expr);
                self.emit_store_at(&field_expr.ty, offset);
                offset += field_size;
            }
        }

        self.scratch.free_i32(scratch);
        wasm!(self.func, { local_get(scratch); });
    }

    /// Look up variant tag for a constructor name within a variant type.
    pub(super) fn resolve_variant_tag(&self, name: Option<&str>, result_ty: &Ty) -> Option<u32> {
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

    /// Emit spread record: copy base, then overwrite specified fields.
    pub(super) fn emit_spread_record(&mut self, base: &IrExpr, overrides: &[(almide_base::intern::Sym, IrExpr)], result_ty: &Ty) {
        let all_fields = self.extract_record_fields(result_ty);
        let tag_offset = self.variant_tag_offset(result_ty);
        let total_size = tag_offset + values::record_size(&all_fields);

        // Allocate new record
        wasm!(self.func, {
            i32_const(total_size as i32);
            call(self.emitter.rt.alloc);
        });
        let result_scratch = self.scratch.alloc_i32();
        wasm!(self.func, { local_set(result_scratch); });

        // Evaluate base and store ptr
        self.emit_expr(base);
        let base_scratch = self.scratch.alloc_i32();
        wasm!(self.func, { local_set(base_scratch); });

        // Copy all bytes from base to result (including tag if variant)
        let counter = self.scratch.alloc_i64();
        wasm!(self.func, {
            i64_const(0);
            local_set(counter);
            block_empty;
            loop_empty;
        });
        // break if counter >= total_size
        wasm!(self.func, {
            local_get(counter);
            i64_const(total_size as i64);
            i64_ge_u;
            br_if(1);
        });
        // dst[i] = src[i]
        wasm!(self.func, {
            local_get(result_scratch);
            local_get(counter);
            i32_wrap_i64;
            i32_add;
            local_get(base_scratch);
            local_get(counter);
            i32_wrap_i64;
            i32_add;
            i32_load8_u(0);
            i32_store8(0);
        });
        // counter++
        wasm!(self.func, {
            local_get(counter);
            i64_const(1);
            i64_add;
            local_set(counter);
            br(0);
            end;
            end;
        });

        // Overwrite specified fields
        for (field_name, field_expr) in overrides {
            // Use the record's declared field type for the store width — the
            // override expression's own `.ty` may be Unknown when inference
            // was incomplete (e.g. lambda body without propagated types),
            // whereas the record layout is authoritative.
            if let Some((offset, field_ty)) = values::field_offset(&all_fields, field_name) {
                let total_offset = tag_offset + offset;
                wasm!(self.func, { local_get(result_scratch); });
                self.emit_expr(field_expr);
                self.emit_store_at(&field_ty, total_offset);
            }
        }

        // Return result ptr
        self.scratch.free_i64(counter);
        self.scratch.free_i32(base_scratch);
        self.scratch.free_i32(result_scratch);
        wasm!(self.func, { local_get(result_scratch); });
    }

    /// Emit a list literal: allocate [len:i32][elem0][elem1]...
    pub(super) fn emit_list(&mut self, elements: &[IrExpr], _list_ty: &Ty) {
        let elem_ty = if let Some(first) = elements.first() {
            first.ty.clone()
        } else {
            Ty::Int // empty list fallback
        };
        let elem_size = values::byte_size(&elem_ty);
        let n = elements.len() as u32;
        let total = 4 + n * elem_size;

        wasm!(self.func, {
            i32_const(total as i32);
            call(self.emitter.rt.alloc);
        });

        let scratch = self.scratch.alloc_i32();
        wasm!(self.func, { local_set(scratch); });

        // Store length
        wasm!(self.func, {
            local_get(scratch);
            i32_const(n as i32);
            i32_store(0);
        });

        // Store each element
        for (i, elem) in elements.iter().enumerate() {
            let offset = 4 + (i as u32) * elem_size;
            wasm!(self.func, { local_get(scratch); });
            self.emit_expr(elem);
            self.emit_store_at(&elem.ty, offset);
        }

        self.scratch.free_i32(scratch);
        wasm!(self.func, { local_get(scratch); });
    }

    /// Emit index access: list_ptr + 4 + index * elem_size
    pub(super) fn emit_index_access(&mut self, object: &IrExpr, index: &IrExpr, result_ty: &Ty) {
        let elem_size = values::byte_size(result_ty);

        self.emit_expr(object); // list ptr

        // Optimize constant index: compute offset at compile time
        if let IrExprKind::LitInt { value } = &index.kind {
            let offset = 4 + (*value as u32) * (elem_size as u32);
            self.emit_load_at(result_ty, offset);
            return;
        }

        wasm!(self.func, {
            i32_const(4);
            i32_add;
        });

        // Add index * elem_size
        self.emit_expr(index);
        if matches!(&index.ty, Ty::Int) {
            wasm!(self.func, { i32_wrap_i64; });
        }
        wasm!(self.func, {
            i32_const(elem_size as i32);
            i32_mul;
            i32_add;
        });

        // Load element
        self.emit_load_at(result_ty, 0);
    }

    /// Emit a tuple construction: allocate memory, store each element sequentially.
    pub(super) fn emit_tuple(&mut self, elements: &[IrExpr]) {
        let element_types: Vec<(String, Ty)> = elements.iter().enumerate()
            .map(|(i, e)| {
                let ty = if matches!(&e.ty, Ty::Unknown | Ty::TypeVar(_)) {
                    if let almide_ir::IrExprKind::Var { id } = &e.kind {
                        let vt_ty = &self.var_table.get(*id).ty;
                        if !matches!(vt_ty, Ty::Unknown | Ty::TypeVar(_)) { vt_ty.clone() }
                        else { e.ty.clone() }
                    } else { e.ty.clone() }
                } else { e.ty.clone() };
                (format!("_{}", i), ty)
            })
            .collect();
        let total_size = values::record_size(&element_types);

        wasm!(self.func, {
            i32_const(total_size as i32);
            call(self.emitter.rt.alloc);
        });

        let scratch = self.scratch.alloc_i32();
        wasm!(self.func, { local_set(scratch); });

        let mut offset = 0u32;
        for elem in elements {
            // Resolve element type: use VarTable for Var refs, infer for Unknown
            let elem_ty = if matches!(&elem.ty, Ty::Unknown | Ty::TypeVar(_)) {
                if let almide_ir::IrExprKind::Var { id } = &elem.kind {
                    let vt_ty = &self.var_table.get(*id).ty;
                    if !matches!(vt_ty, Ty::Unknown | Ty::TypeVar(_)) { vt_ty.clone() }
                    else { self.infer_type_from_expr(elem) }
                } else {
                    self.infer_type_from_expr(elem)
                }
            } else {
                elem.ty.clone()
            };
            let size = values::byte_size(&elem_ty);
            wasm!(self.func, { local_get(scratch); });
            self.emit_expr(elem);
            self.emit_store_at(&elem_ty, offset);
            offset += size;
        }

        self.scratch.free_i32(scratch);
        wasm!(self.func, { local_get(scratch); });
    }

    /// Emit a tuple index access: load from tuple pointer + element offset.
    pub(super) fn emit_tuple_index(&mut self, object: &IrExpr, index: usize, result_ty: &Ty) {
        // Resolve object type — try VarTable if object.ty is not Tuple
        let obj_ty = if let Ty::Tuple(_) = &object.ty {
            object.ty.clone()
        } else if let almide_ir::IrExprKind::Var { id } = &object.kind {
            let vt_ty = &self.var_table.get(*id).ty;
            if let Ty::Tuple(_) = vt_ty { vt_ty.clone() } else { object.ty.clone() }
        } else {
            object.ty.clone()
        };

        // Compute offset by summing sizes of elements before `index`
        let (offset, elem_ty) = if let Ty::Tuple(elem_types) = &obj_ty {
            let off = elem_types.iter().take(index).map(|t| values::byte_size(t)).sum::<u32>();
            let ty = elem_types.get(index).cloned();
            (off, ty)
        } else {
            (0, None)
        };

        // Use result_ty if concrete, otherwise fall back to tuple element type
        let load_ty = if matches!(result_ty, Ty::Unknown | Ty::TypeVar(_)) {
            elem_ty.as_ref().unwrap_or(result_ty)
        } else {
            result_ty
        };

        self.emit_expr(object);
        self.emit_load_at(load_ty, offset);
    }

    /// Emit a field access: load from record/variant pointer + field offset.
    pub(super) fn emit_member(&mut self, object: &IrExpr, field: &str) {
        // Resolve object type for field offset calculation.
        // Priority: VarTable (for Var), then object.ty.
        // For chained member access (e.g. app.config.port), the intermediate
        // Member expr may have the parent type instead of the field result type.
        let resolved_ty = if let almide_ir::IrExprKind::Var { id } = &object.kind {
            let vt_ty = &self.var_table.get(*id).ty;
            if matches!(&object.ty, Ty::OpenRecord { .. } | Ty::TypeVar(_) | Ty::Unknown) && !matches!(vt_ty, Ty::OpenRecord { .. } | Ty::TypeVar(_) | Ty::Unknown) {
                vt_ty.clone()
            } else {
                object.ty.clone()
            }
        } else if let almide_ir::IrExprKind::Member { object: inner_obj, field: inner_field } = &object.kind {
            // Chained member: resolve the intermediate type from the inner object's fields
            let inner_ty = self.resolve_member_result_type(inner_obj, inner_field);
            if !matches!(inner_ty, Ty::Unknown) { inner_ty } else { object.ty.clone() }
        } else {
            object.ty.clone()
        };
        let mut fields = self.extract_record_fields(&resolved_ty);
        let tag_offset = self.variant_tag_offset(&resolved_ty);

        // If fields are empty and type is Unknown, try searching record_fields for a type that has this field
        if fields.is_empty() && matches!(&resolved_ty, Ty::Unknown | Ty::TypeVar(_)) {
            for (_name, rf) in &self.emitter.record_fields {
                if rf.iter().any(|(n, _)| n == field) {
                    fields = rf.clone();
                    break;
                }
            }
        }

        self.emit_expr(object);

        if let Some((field_offset, field_ty)) = values::field_offset(&fields, field) {
            let total_offset = tag_offset + field_offset;
            self.emit_load_at(&field_ty, total_offset);
        } else {
            wasm!(self.func, { unreachable; });
        }
    }

    /// Resolve the result type of a member access (obj.field) for chained access.
    fn resolve_member_result_type(&self, object: &IrExpr, field: &str) -> Ty {
        let obj_ty = if let almide_ir::IrExprKind::Var { id } = &object.kind {
            let vt_ty = &self.var_table.get(*id).ty;
            if !matches!(vt_ty, Ty::Unknown | Ty::TypeVar(_)) { vt_ty.clone() } else { object.ty.clone() }
        } else if let almide_ir::IrExprKind::Member { object: inner, field: inner_f } = &object.kind {
            self.resolve_member_result_type(inner, inner_f)
        } else {
            object.ty.clone()
        };
        let fields = self.extract_record_fields(&obj_ty);
        if let Some((_, field_ty)) = values::field_offset(&fields, field) {
            field_ty
        } else {
            Ty::Unknown
        }
    }
}
