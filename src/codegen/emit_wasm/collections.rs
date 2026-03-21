//! Collection construction and access emission — records, lists, tuples, maps.

use crate::ir::IrExpr;
use crate::types::Ty;
use wasm_encoder::MemArg;

use super::FuncCompiler;
use super::values;

impl FuncCompiler<'_> {
    /// Emit a record/variant construction: allocate memory, store fields.
    /// For variants (detected via name + type), prepends a tag i32 before fields.
    pub(super) fn emit_record(&mut self, name: Option<&str>, fields: &[(String, IrExpr)], result_ty: &Ty) {
        // Check if this is a variant constructor
        let tag = self.resolve_variant_tag(name, result_ty);
        let tag_size: u32 = if tag.is_some() { 4 } else { 0 };

        // Compute field types and total size (including defaults)
        let explicit_field_size: u32 = fields.iter()
            .map(|(_, expr)| values::byte_size(&expr.ty))
            .sum();
        let default_field_size: u32 = if let Some(ctor_name) = name {
            let explicit_names: std::collections::HashSet<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
            self.emitter.default_fields.iter()
                .filter(|((cn, _), _)| cn == ctor_name)
                .filter(|((_, fn_name), _)| !explicit_names.contains(fn_name.as_str()))
                .map(|((_, _), expr)| {
                    match (&expr.ty, &expr.kind) {
                        (Ty::Unknown, crate::ir::IrExprKind::LitInt { .. }) => values::byte_size(&Ty::Int),
                        (Ty::Unknown, crate::ir::IrExprKind::LitFloat { .. }) => values::byte_size(&Ty::Float),
                        _ => values::byte_size(&expr.ty),
                    }
                })
                .sum()
        } else { 0 };
        let total_size = tag_size + explicit_field_size + default_field_size;

        // Allocate
        wasm!(self.func, {
            i32_const(total_size as i32);
            call(self.emitter.rt.alloc);
        });

        let scratch = self.match_i32_base + self.match_depth;
        self.match_depth += 1;
        wasm!(self.func, { local_set(scratch); });

        // Write tag if variant
        if let Some(tag_val) = tag {
            wasm!(self.func, {
                local_get(scratch);
                i32_const(tag_val as i32);
                i32_store(0);
            });
        }

        // Store each explicit field (offset starts after tag)
        let explicit_names: std::collections::HashSet<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
        let mut offset = tag_size;
        for (_, field_expr) in fields {
            let field_size = values::byte_size(&field_expr.ty);
            wasm!(self.func, { local_get(scratch); });
            self.emit_expr(field_expr);
            self.emit_store_at(&field_expr.ty, offset);
            offset += field_size;
        }

        // Fill in default fields that were not explicitly provided
        if let Some(ctor_name) = name {
            let defaults: Vec<(String, crate::ir::IrExpr)> = self.emitter.default_fields.iter()
                .filter(|((cn, _), _)| cn == ctor_name)
                .filter(|((_, fn_name), _)| !explicit_names.contains(fn_name.as_str()))
                .map(|((_, fn_name), expr)| (fn_name.clone(), expr.clone()))
                .collect();
            for (_, default_expr) in &defaults {
                // Use correct field type: infer from expr kind when ty is Unknown
                let field_ty = match (&default_expr.ty, &default_expr.kind) {
                    (Ty::Unknown, crate::ir::IrExprKind::LitInt { .. }) => Ty::Int,
                    (Ty::Unknown, crate::ir::IrExprKind::LitFloat { .. }) => Ty::Float,
                    (Ty::Unknown, crate::ir::IrExprKind::LitBool { .. }) => Ty::Bool,
                    (Ty::Unknown, crate::ir::IrExprKind::LitStr { .. }) => Ty::String,
                    _ => default_expr.ty.clone(),
                };
                let field_size = values::byte_size(&field_ty);
                wasm!(self.func, { local_get(scratch); });
                self.emit_expr(default_expr);
                self.emit_store_at(&field_ty, offset);
                offset += field_size;
            }
        }

        // Also recompute total_size to include defaults
        // (The allocation above may be too small — need to fix)

        self.match_depth -= 1;
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
    pub(super) fn emit_spread_record(&mut self, base: &IrExpr, overrides: &[(String, IrExpr)], result_ty: &Ty) {
        let all_fields = self.extract_record_fields(result_ty);
        let tag_offset = self.variant_tag_offset(result_ty);
        let total_size = tag_offset + values::record_size(&all_fields);

        // Allocate new record
        wasm!(self.func, {
            i32_const(total_size as i32);
            call(self.emitter.rt.alloc);
        });
        let result_scratch = self.match_i32_base + self.match_depth;
        wasm!(self.func, { local_set(result_scratch); });

        // Evaluate base and store ptr
        self.emit_expr(base);
        let base_scratch = result_scratch + 1;
        wasm!(self.func, { local_set(base_scratch); });

        // Copy all bytes from base to result (including tag if variant)
        let counter = self.match_i64_base + self.match_depth;
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
            if let Some((offset, _)) = values::field_offset(&all_fields, field_name) {
                let total_offset = tag_offset + offset;
                wasm!(self.func, { local_get(result_scratch); });
                self.emit_expr(field_expr);
                self.emit_store_at(&field_expr.ty, total_offset);
            }
        }

        // Return result ptr
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

        let scratch = self.match_i32_base + self.match_depth;
        self.match_depth += 1;
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

        self.match_depth -= 1;
        wasm!(self.func, { local_get(scratch); });
    }

    /// Emit index access: list_ptr + 4 + index * elem_size
    pub(super) fn emit_index_access(&mut self, object: &IrExpr, index: &IrExpr, result_ty: &Ty) {
        let elem_size = values::byte_size(result_ty);

        self.emit_expr(object); // list ptr
        wasm!(self.func, {
            i32_const(4);
            i32_add;
        });

        // Add index * elem_size
        self.emit_expr(index);
        // Index might be i64 (Int), convert to i32
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
            .map(|(i, e)| (format!("_{}", i), e.ty.clone()))
            .collect();
        let total_size = values::record_size(&element_types);

        wasm!(self.func, {
            i32_const(total_size as i32);
            call(self.emitter.rt.alloc);
        });

        let scratch = self.match_i32_base + self.match_depth;
        self.match_depth += 1;
        wasm!(self.func, { local_set(scratch); });

        let mut offset = 0u32;
        for elem in elements {
            let size = values::byte_size(&elem.ty);
            wasm!(self.func, { local_get(scratch); });
            self.emit_expr(elem);
            self.emit_store_at(&elem.ty, offset);
            offset += size;
        }

        self.match_depth -= 1;
        wasm!(self.func, { local_get(scratch); });
    }

    /// Emit a tuple index access: load from tuple pointer + element offset.
    pub(super) fn emit_tuple_index(&mut self, object: &IrExpr, index: usize, result_ty: &Ty) {
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
    pub(super) fn emit_member(&mut self, object: &IrExpr, field: &str) {
        let fields = self.extract_record_fields(&object.ty);
        let tag_offset = self.variant_tag_offset(&object.ty);

        self.emit_expr(object);

        if let Some((field_offset, field_ty)) = values::field_offset(&fields, field) {
            let total_offset = tag_offset + field_offset;
            self.emit_load_at(&field_ty, total_offset);
        } else {
            wasm!(self.func, { unreachable; });
        }
    }
}
