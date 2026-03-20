//! Collection construction and access emission — records, lists, tuples, maps.

use crate::ir::IrExpr;
use crate::types::Ty;
use wasm_encoder::{BlockType, Instruction, MemArg};

use super::FuncCompiler;
use super::values;

impl FuncCompiler<'_> {
    /// Emit a record/variant construction: allocate memory, store fields.
    /// For variants (detected via name + type), prepends a tag i32 before fields.
    pub(super) fn emit_record(&mut self, name: Option<&str>, fields: &[(String, IrExpr)], result_ty: &Ty) {
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
        self.func.instruction(&Instruction::I32Const(total_size as i32));
        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));
        let result_scratch = self.match_i32_base + self.match_depth;
        self.func.instruction(&Instruction::LocalSet(result_scratch));

        // Evaluate base and store ptr
        self.emit_expr(base);
        let base_scratch = result_scratch + 1;
        self.func.instruction(&Instruction::LocalSet(base_scratch));

        // Copy all bytes from base to result (including tag if variant)
        // Byte-by-byte copy loop
        // Use i64 scratch as counter
        let counter = self.match_i64_base + self.match_depth;
        self.func.instruction(&Instruction::I64Const(0));
        self.func.instruction(&Instruction::LocalSet(counter));
        self.func.instruction(&Instruction::Block(BlockType::Empty));
        self.func.instruction(&Instruction::Loop(BlockType::Empty));
        // break if counter >= total_size
        self.func.instruction(&Instruction::LocalGet(counter));
        self.func.instruction(&Instruction::I64Const(total_size as i64));
        self.func.instruction(&Instruction::I64GeU);
        self.func.instruction(&Instruction::BrIf(1));
        // dst[i] = src[i]
        self.func.instruction(&Instruction::LocalGet(result_scratch));
        self.func.instruction(&Instruction::LocalGet(counter));
        self.func.instruction(&Instruction::I32WrapI64);
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::LocalGet(base_scratch));
        self.func.instruction(&Instruction::LocalGet(counter));
        self.func.instruction(&Instruction::I32WrapI64);
        self.func.instruction(&Instruction::I32Add);
        self.func.instruction(&Instruction::I32Load8U(MemArg { offset: 0, align: 0, memory_index: 0 }));
        self.func.instruction(&Instruction::I32Store8(MemArg { offset: 0, align: 0, memory_index: 0 }));
        // counter++
        self.func.instruction(&Instruction::LocalGet(counter));
        self.func.instruction(&Instruction::I64Const(1));
        self.func.instruction(&Instruction::I64Add);
        self.func.instruction(&Instruction::LocalSet(counter));
        self.func.instruction(&Instruction::Br(0));
        self.func.instruction(&Instruction::End);
        self.func.instruction(&Instruction::End);

        // Overwrite specified fields
        for (field_name, field_expr) in overrides {
            if let Some((offset, _)) = values::field_offset(&all_fields, field_name) {
                let total_offset = tag_offset + offset;
                self.func.instruction(&Instruction::LocalGet(result_scratch));
                self.emit_expr(field_expr);
                self.emit_store_at(&field_expr.ty, total_offset);
            }
        }

        // Return result ptr
        self.func.instruction(&Instruction::LocalGet(result_scratch));
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

        self.func.instruction(&Instruction::I32Const(total as i32));
        self.func.instruction(&Instruction::Call(self.emitter.rt.alloc));

        let scratch = self.match_i32_base + self.match_depth;
        self.func.instruction(&Instruction::LocalSet(scratch));

        // Store length
        self.func.instruction(&Instruction::LocalGet(scratch));
        self.func.instruction(&Instruction::I32Const(n as i32));
        self.func.instruction(&Instruction::I32Store(MemArg {
            offset: 0, align: 2, memory_index: 0,
        }));

        // Store each element
        for (i, elem) in elements.iter().enumerate() {
            let offset = 4 + (i as u32) * elem_size;
            self.func.instruction(&Instruction::LocalGet(scratch));
            self.emit_expr(elem);
            self.emit_store_at(&elem.ty, offset);
        }

        self.func.instruction(&Instruction::LocalGet(scratch));
    }

    /// Emit index access: list_ptr + 4 + index * elem_size
    pub(super) fn emit_index_access(&mut self, object: &IrExpr, index: &IrExpr, result_ty: &Ty) {
        let elem_size = values::byte_size(result_ty);

        self.emit_expr(object); // list ptr
        self.func.instruction(&Instruction::I32Const(4)); // skip len
        self.func.instruction(&Instruction::I32Add);

        // Add index * elem_size
        self.emit_expr(index);
        // Index might be i64 (Int), convert to i32
        if matches!(&index.ty, Ty::Int) {
            self.func.instruction(&Instruction::I32WrapI64);
        }
        self.func.instruction(&Instruction::I32Const(elem_size as i32));
        self.func.instruction(&Instruction::I32Mul);
        self.func.instruction(&Instruction::I32Add);

        // Load element
        self.emit_load_at(result_ty, 0);
    }

    /// Emit a tuple construction: allocate memory, store each element sequentially.
    pub(super) fn emit_tuple(&mut self, elements: &[IrExpr]) {
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
}
