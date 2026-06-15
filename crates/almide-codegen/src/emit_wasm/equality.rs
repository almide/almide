//! Deep equality comparison for WASM codegen.
//!
//! Type-aware recursive equality for List, Option, Result, Tuple, Record, Variant.

use crate::emit_wasm::engine::{Imm32, Imm64, Local};
use std::collections::HashMap;
use std::collections::BTreeMap;

// Named constants for raw WASM immediate values used in this module.
/// Position of the sign bit in an i64 (used by the f64 total-order key transform).
const I64_SIGN_BIT_POS: i64 = 63;
/// Bytes of a variant's discriminant tag, stored at offset 0 ahead of the
/// payload. A variant value is laid out `[tag: i32][payload…]`, padded to the
/// max payload across the type's constructors so `mem_eq` can compare any two
/// by a single fixed-width span.
pub(super) const VARIANT_TAG_SIZE: u32 = 4;

use super::FuncCompiler;
use super::VariantCase;
use super::values;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
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
        use almide_lang::types::constructor::TypeConstructorId;
        match ty {
            Ty::Int => { wasm!(self.func, { i64_eq; }); }
            Ty::Float => { wasm!(self.func, { f64_eq; }); }
            // Sized Numeric Types (Stage 1c): narrow ints ride in i32
            // at the WASM level, so equality uses `i32.eq`. `UInt64`
            // stays i64. `Float32` uses `f32.eq` via the Instruction
            // API (macro lacks an `f32_eq` rule).
            Ty::Int8 | Ty::Int16 | Ty::Int32
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 => {
                wasm!(self.func, { i32_eq; });
            }
            Ty::Int64 | Ty::UInt64 => { wasm!(self.func, { i64_eq; }); }
            Ty::Float32 => {
                self.func.instruction(&wasm_encoder::Instruction::F32Eq);
            }
            Ty::Float64 => { wasm!(self.func, { f64_eq; }); }
            Ty::Bool => { wasm!(self.func, { i32_eq; }); }
            Ty::String | Ty::Bytes => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }

            Ty::Applied(TypeConstructorId::List, args) => {
                let elem_ty = args.first().cloned().unwrap_or(Ty::Int);
                // If elem is a value type (no pointers), use byte comparison
                if self.is_value_type(&elem_ty) {
                    let elem_size = values::byte_size(&elem_ty);
                    wasm!(self.func, {
                        i32_const(Imm32(elem_size as i32));
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

            // Map/Set are structural: two maps are equal iff same size and every
            // (k,v) of one is present with an equal value in the other; sets the
            // same on keys. WASM previously fell through to `i32_eq` (pointer
            // identity) so two structurally-equal maps built independently
            // compared unequal — a real divergence from native.
            Ty::Applied(TypeConstructorId::Map, args) => {
                let key_ty = args.first().cloned().unwrap_or(Ty::Int);
                let val_ty = args.get(1).cloned().unwrap_or(Ty::Int);
                self.emit_map_eq_deep(&key_ty, &val_ty);
            }

            Ty::Applied(TypeConstructorId::Set, args) => {
                let elem_ty = args.first().cloned().unwrap_or(Ty::Int);
                self.emit_set_eq_deep(&elem_ty);
            }

            Ty::Tuple(elems) => {
                if elems.iter().all(|t| self.is_value_type(t)) {
                    let size: u32 = elems.iter().map(|t| values::byte_size(t)).sum();
                    wasm!(self.func, { i32_const(Imm32(size as i32)); call(self.emitter.rt.mem_eq); });
                } else {
                    self.emit_tuple_eq_deep(elems);
                }
            }

            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                let string_fields: Vec<(String, Ty)> = fields.iter().map(|(n, t)| (n.to_string(), t.clone())).collect();
                if fields.iter().all(|(_, t)| self.is_value_type(t)) {
                    let size = values::record_size(&string_fields);
                    wasm!(self.func, { i32_const(Imm32(size as i32)); call(self.emitter.rt.mem_eq); });
                } else {
                    // Field-by-field deep equality
                    self.emit_record_eq_deep(&string_fields);
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
                        wasm!(self.func, { i32_const(Imm32(size as i32)); call(self.emitter.rt.mem_eq); });
                    }
                } else {
                    let fields = self.emitter.record_fields.get(name.as_str()).cloned().unwrap_or_default();
                    if fields.iter().any(|(_, ft)| !self.is_value_type(ft)) {
                        self.emit_record_eq_deep(&fields);
                    } else {
                        let size = values::record_size(&fields);
                        if size > 0 {
                            wasm!(self.func, { i32_const(Imm32(size as i32)); call(self.emitter.rt.mem_eq); });
                        } else {
                            wasm!(self.func, { i32_eq; });
                        }
                    }
                }
            }

            Ty::Variant { name, cases, .. } => {
                let has_pointers = cases.iter().any(|c| {
                    match &c.payload {
                        almide_lang::types::VariantPayload::Tuple(ts) => ts.iter().any(|t| !self.is_value_type(t)),
                        almide_lang::types::VariantPayload::Record(fs) => fs.iter().any(|(_, t)| !self.is_value_type(t)),
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
                                almide_lang::types::VariantPayload::Tuple(ts) =>
                                    ts.iter().enumerate().map(|(j, t)| (format!("_{}", j), t.clone())).collect(),
                                almide_lang::types::VariantPayload::Record(fs) =>
                                    fs.iter().map(|(n, t)| (n.to_string(), t.clone())).collect(),
                                _ => vec![],
                            };
                            super::VariantCase { name: c.name.to_string(), tag: i as u32, fields }
                        }).collect();
                        self.emit_variant_eq_deep(&case_infos, &[]);
                    }
                } else {
                    let max_payload: u32 = cases.iter()
                        .map(|c| match &c.payload {
                            almide_lang::types::VariantPayload::Unit => 0,
                            almide_lang::types::VariantPayload::Tuple(ts) => ts.iter().map(|t| values::byte_size(t)).sum(),
                            almide_lang::types::VariantPayload::Record(fs) => fs.iter().map(|(_, t)| values::byte_size(t)).sum(),
                        })
                        .max().unwrap_or(0);
                    let size = 4 + max_payload;
                    wasm!(self.func, { i32_const(Imm32(size as i32)); call(self.emitter.rt.mem_eq); });
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
        use almide_ir::IrExprKind;
        match &expr.kind {
            IrExprKind::TupleIndex { object, index } => {
                // Try to get the tuple type from the object, then extract element type
                let obj_ty = if object.ty.is_unresolved() {
                    // Try VarTable
                    if let IrExprKind::Var { id } = &object.kind {
                        if (id.0 as usize) < self.var_table.len() {
                            let info = self.var_table.get(*id);
                            if !info.ty.is_unresolved() {
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
                    if !info.ty.is_unresolved() {
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
        let matched = self.scratch.alloc_i32();
        let elem_size = values::byte_size(elem_ty);
        wasm!(self.func, {
            local_set(Local(b)); // b
            local_set(Local(a)); // a
            // Same pointer → true
            local_get(Local(a)); local_get(Local(b)); i32_eq;
            if_i32; i32_const(Imm32(1));
            else_;
              // Different lengths → false
              local_get(Local(a)); i32_load(0);
              local_get(Local(b)); i32_load(0);
              i32_ne;
              if_i32; i32_const(Imm32(0));
              else_;
                // Compare element by element
                i32_const(Imm32(0)); local_set(Local(i)); // i
                i32_const(Imm32(1)); local_set(Local(matched)); // 1 until a mismatch is found
                block_empty; loop_empty;
                  local_get(Local(i)); local_get(Local(a)); i32_load(0); i32_ge_u; br_if(1);
                  // Load a[i]
                  local_get(Local(a)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32)); i32_add;
                  local_get(Local(i)); i32_const(Imm32(elem_size as i32)); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        // Load b[i]
        wasm!(self.func, {
                  local_get(Local(b)); i32_const(Imm32(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32)); i32_add;
                  local_get(Local(i)); i32_const(Imm32(elem_size as i32)); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        // Compare elements (recursive)
        let elem_ty_clone = elem_ty.clone();
        self.emit_eq_typed(&elem_ty_clone);
        wasm!(self.func, {
                  i32_eqz; // not equal?
                  if_empty;
                    // Mismatch: set result 0 and break out of the loop+block.
                    // (NOT `return_` — that returned 0 from the ENCLOSING function,
                    // corrupting its contract whenever a List/Tuple/Record of
                    // unequal heap elements like String was compared.)
                    i32_const(Imm32(0)); local_set(Local(matched)); br(2);
                  end;
                  local_get(Local(i)); i32_const(Imm32(1)); i32_add; local_set(Local(i));
                  br(0);
                end; end;
                local_get(Local(matched));
              end;
            end;
        });
        self.scratch.free_i32(matched);
        self.scratch.free_i32(i);
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    /// Structural map equality (compact-ordered-dict): [a_ptr, b_ptr] → i32.
    /// Equal iff same size and every (k,v) of `a` is present in `b` with an
    /// equal value. Walks a's dense entries in insertion order and probes b's
    /// COD index for each key, then compares values recursively.
    fn emit_map_eq_deep(&mut self, key_ty: &Ty, val_ty: &Ty) {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let map_cap_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
        let map_tags_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
        let ks = values::byte_size(key_ty);
        let vs = values::byte_size(val_ty);
        let es = ks + vs;

        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let acap = self.scratch.alloc_i32();
        let alen = self.scratch.alloc_i32();
        let aeb = self.scratch.alloc_i32();
        let ai = self.scratch.alloc_i32();
        let sk32 = self.scratch.alloc_i32();
        let sk64 = self.scratch.alloc_i64();
        let bcap = self.scratch.alloc_i32();
        let bib = self.scratch.alloc_i32();
        let beb = self.scratch.alloc_i32();
        let bidx = self.scratch.alloc_i32();
        let bei = self.scratch.alloc_i32();
        let h2 = self.scratch.alloc_i32();
        let tg = self.scratch.alloc_i32();
        let found = self.scratch.alloc_i32();

        wasm!(self.func, {
            local_set(Local(b));
            local_set(Local(a));
            // Same pointer → equal.
            local_get(Local(a)); local_get(Local(b)); i32_eq;
            if_i32; i32_const(Imm32(1));
            else_;
              // Different size → not equal.
              local_get(Local(a)); i32_load(0); local_get(Local(b)); i32_load(0); i32_ne;
              if_i32; i32_const(Imm32(0));
              else_;
                i32_const(Imm32(1)); local_set(Local(result));
                local_get(Local(a)); i32_load(0); local_set(Local(alen));
                local_get(Local(a)); i32_load(map_cap_off); local_set(Local(acap));
        });
        self.emit_dict_entries_base(a, acap);
        wasm!(self.func, {
                local_set(Local(aeb));
                i32_const(Imm32(0)); local_set(Local(ai));
                block_empty; loop_empty;                                  // [A] a-exit, [B] a-loop
                  local_get(Local(ai)); local_get(Local(alen)); i32_ge_u; br_if(1);     // done iterating a (dense)
                  // Load this dense entry's key into the search-key register.
                  local_get(Local(aeb)); local_get(Local(ai)); i32_const(Imm32(es as i32)); i32_mul; i32_add;
        });
        self.emit_key_load(key_ty, 0);
        self.emit_search_key_store(key_ty, sk32, sk64);
        // Probe b for the key.
        wasm!(self.func, {
                  i32_const(Imm32(0)); local_set(Local(found));
                  local_get(Local(b)); i32_load(map_cap_off); local_set(Local(bcap));
                  local_get(Local(bcap)); i32_eqz;
                  if_empty; else_;                                        // [D] bcap==0 guard
        });
        self.emit_dict_index_base(b, bcap);
        wasm!(self.func, { local_set(Local(bib)); });
        self.emit_dict_entries_base(b, bcap);
        wasm!(self.func, { local_set(Local(beb)); });
        self.emit_search_key_load(key_ty, sk32, sk64);
        self.emit_hash_key(key_ty);
        self.emit_h1_h2(bcap, bidx, h2);
        wasm!(self.func, {
                    block_empty; loop_empty;                             // [E] probe-block, [F] probe-loop
                      local_get(Local(b)); i32_const(Imm32(map_tags_off)); i32_add; local_get(Local(bidx)); i32_add; i32_load8_u(0); local_set(Local(tg));
                      local_get(Local(tg)); i32_eqz; br_if(1);                  // empty slot → key absent
                      local_get(Local(tg)); local_get(Local(h2)); i32_eq;
                      if_empty;                                          // [G] tag matches
                        // bei = index[bidx] - 1 (1-based pointer into dense entries)
                        local_get(Local(bib)); local_get(Local(bidx)); i32_const(Imm32(lm::INDEX_SLOT_SIZE as i32)); i32_mul; i32_add;
                        i32_load(0); i32_const(Imm32(1)); i32_sub; local_set(Local(bei));
                        local_get(Local(beb)); local_get(Local(bei)); i32_const(Imm32(es as i32)); i32_mul; i32_add;
        });
        self.emit_key_load(key_ty, 0);
        self.emit_search_key_load(key_ty, sk32, sk64);
        self.emit_key_eq(key_ty);
        wasm!(self.func, {
                        if_empty; i32_const(Imm32(1)); local_set(Local(found)); br(3); end;   // found → exit probe-block
                      end;
                      local_get(Local(bidx)); i32_const(Imm32(1)); i32_add;
                      local_get(Local(bcap)); i32_const(Imm32(1)); i32_sub; i32_and;
                      local_set(Local(bidx)); br(0);
                    end; end;                                            // close F, E
                  end;                                                   // close D
                  // Key absent → maps differ.
                  local_get(Local(found)); i32_eqz;
                  if_empty; i32_const(Imm32(0)); local_set(Local(result)); br(2); end; // exit a-loop+block
        });
        // Compare the values (skip for valueless maps).
        if vs > 0 {
            wasm!(self.func, {
                  local_get(Local(aeb)); local_get(Local(ai)); i32_const(Imm32(es as i32)); i32_mul; i32_add; i32_const(Imm32(ks as i32)); i32_add;
            });
            self.emit_load_at(val_ty, 0);
            wasm!(self.func, {
                  local_get(Local(beb)); local_get(Local(bei)); i32_const(Imm32(es as i32)); i32_mul; i32_add; i32_const(Imm32(ks as i32)); i32_add;
            });
            self.emit_load_at(val_ty, 0);
            self.emit_eq_typed(val_ty);
            wasm!(self.func, {
                  i32_eqz;
                  if_empty; i32_const(Imm32(0)); local_set(Local(result)); br(2); end; // values differ → exit
            });
        }
        wasm!(self.func, {
                  local_get(Local(ai)); i32_const(Imm32(1)); i32_add; local_set(Local(ai)); br(0);
                end; end;                                                // close B, A
                local_get(Local(result));
              end;                                                       // close size-if
            end;                                                         // close sameptr-if
        });
        self.scratch.free_i32(found);
        self.scratch.free_i32(tg);
        self.scratch.free_i32(h2);
        self.scratch.free_i32(bei);
        self.scratch.free_i32(bidx);
        self.scratch.free_i32(beb);
        self.scratch.free_i32(bib);
        self.scratch.free_i32(bcap);
        self.scratch.free_i64(sk64);
        self.scratch.free_i32(sk32);
        self.scratch.free_i32(ai);
        self.scratch.free_i32(aeb);
        self.scratch.free_i32(alen);
        self.scratch.free_i32(acap);
        self.scratch.free_i32(result);
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    /// Structural set equality (dense list, insertion order): [a_ptr, b_ptr] → i32.
    /// Sets are unsorted, so equal sets may differ in memory layout — compare by
    /// size + containment (every element of `a` is in `b`) rather than byte-eq.
    fn emit_set_eq_deep(&mut self, elem_ty: &Ty) {
        use super::engine::layout::{LIST, list as ll};
        let data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let es = values::byte_size(elem_ty);
        let ea_vt = values::ty_to_valtype(elem_ty).unwrap_or(ValType::I32);

        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let ai = self.scratch.alloc_i32();
        let bj = self.scratch.alloc_i32();
        let found = self.scratch.alloc_i32();
        let ea = self.scratch.alloc(ea_vt);

        wasm!(self.func, {
            local_set(Local(b));
            local_set(Local(a));
            local_get(Local(a)); local_get(Local(b)); i32_eq;
            if_i32; i32_const(Imm32(1));
            else_;
              local_get(Local(a)); i32_load(0); local_get(Local(b)); i32_load(0); i32_ne;
              if_i32; i32_const(Imm32(0));
              else_;
                i32_const(Imm32(1)); local_set(Local(result));
                i32_const(Imm32(0)); local_set(Local(ai));
                block_empty; loop_empty;                                 // [A] exit, [B] a-loop
                  local_get(Local(ai)); local_get(Local(a)); i32_load(0); i32_ge_u; br_if(1);
                  local_get(Local(a)); i32_const(Imm32(data_off)); i32_add; local_get(Local(ai)); i32_const(Imm32(es as i32)); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        wasm!(self.func, {
                  local_set(Local(ea));
                  i32_const(Imm32(0)); local_set(Local(found));
                  i32_const(Imm32(0)); local_set(Local(bj));
                  block_empty; loop_empty;                               // [C] scan-block, [D] scan-loop
                    local_get(Local(bj)); local_get(Local(b)); i32_load(0); i32_ge_u; br_if(1);
                    local_get(Local(b)); i32_const(Imm32(data_off)); i32_add; local_get(Local(bj)); i32_const(Imm32(es as i32)); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        wasm!(self.func, { local_get(Local(ea)); });
        self.emit_eq_typed(elem_ty);
        wasm!(self.func, {
                    if_empty; i32_const(Imm32(1)); local_set(Local(found)); br(2); end; // found → exit scan-block
                    local_get(Local(bj)); i32_const(Imm32(1)); i32_add; local_set(Local(bj)); br(0);
                  end; end;                                              // close D, C
                  local_get(Local(found)); i32_eqz;
                  if_empty; i32_const(Imm32(0)); local_set(Local(result)); br(2); end; // absent → exit a-loop+block
                  local_get(Local(ai)); i32_const(Imm32(1)); i32_add; local_set(Local(ai)); br(0);
                end; end;                                                // close B, A
                local_get(Local(result));
              end;
            end;
        });
        self.scratch.free(ea, ea_vt);
        self.scratch.free_i32(found);
        self.scratch.free_i32(bj);
        self.scratch.free_i32(ai);
        self.scratch.free_i32(result);
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    /// Deep option equality: [a_ptr, b_ptr] → i32
    fn emit_option_eq_deep(&mut self, inner_ty: &Ty) {
        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(Local(b)); // b
            local_set(Local(a)); // a
            // Both none → true
            local_get(Local(a)); i32_eqz; local_get(Local(b)); i32_eqz; i32_and;
            if_i32; i32_const(Imm32(1));
            else_;
              // One none → false
              local_get(Local(a)); i32_eqz; local_get(Local(b)); i32_eqz; i32_or;
              if_i32; i32_const(Imm32(0));
              else_;
                // Both some: compare inner values
                local_get(Local(a));
        });
        self.emit_load_at(inner_ty, 0);
        wasm!(self.func, { local_get(Local(b)); });
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
            local_set(Local(b)); // b
            local_set(Local(a)); // a
            // Tags must match
            local_get(Local(a)); i32_load(0);
            local_get(Local(b)); i32_load(0);
            i32_ne;
            if_i32; i32_const(Imm32(0));
            else_;
              // Same tag. If tag==0 (ok): compare ok values
              local_get(Local(a)); i32_load(0); i32_eqz;
              if_i32;
        });
        // Ty::Unit has no representation — skip loading and treat as equal.
        if matches!(ok_ty, Ty::Unit) {
            wasm!(self.func, { i32_const(Imm32(1)); });
        } else {
            wasm!(self.func, { local_get(Local(a)); });
            self.emit_load_at(ok_ty, 4);
            wasm!(self.func, { local_get(Local(b)); });
            self.emit_load_at(ok_ty, 4);
            let ok_clone = ok_ty.clone();
            self.emit_eq_typed(&ok_clone);
        }
        wasm!(self.func, {
              else_;
                // tag==1 (err): compare err values
        });
        if matches!(err_ty, Ty::Unit) {
            wasm!(self.func, { i32_const(Imm32(1)); });
        } else {
            wasm!(self.func, { local_get(Local(a)); });
            self.emit_load_at(err_ty, 4);
            wasm!(self.func, { local_get(Local(b)); });
            self.emit_load_at(err_ty, 4);
            let err_clone = err_ty.clone();
            self.emit_eq_typed(&err_clone);
        }
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
            local_set(Local(b)); // b
            local_set(Local(a)); // a
        });
        // AND every field's deep eq. NOT a `return_` short-circuit — that returned
        // from the ENCLOSING function and corrupted its contract on a mismatch
        // (e.g. a tuple with an unequal String element). Equality has no side
        // effects, so evaluating all fields and AND-ing is equivalent and safe.
        if elems.is_empty() {
            wasm!(self.func, { i32_const(Imm32(1)); });
        }
        let mut offset: u32 = 0;
        for (i, elem_ty) in elems.iter().enumerate() {
            let elem_size = values::byte_size(elem_ty);
            wasm!(self.func, { local_get(Local(a)); });
            self.emit_load_at(elem_ty, offset);
            wasm!(self.func, { local_get(Local(b)); });
            self.emit_load_at(elem_ty, offset);
            let elem_clone = elem_ty.clone();
            self.emit_eq_typed(&elem_clone);
            if i > 0 {
                wasm!(self.func, { i32_and; });
            }
            offset += elem_size;
        }
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    /// Deep record equality: [a_ptr, b_ptr] → i32
    fn emit_record_eq_deep(&mut self, fields: &[(std::string::String, Ty)]) {
        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(Local(b));
            local_set(Local(a));
        });
        // AND every field's deep eq (see emit_tuple_eq_deep — `return_` corrupted
        // the enclosing function on a mismatch).
        if fields.is_empty() {
            wasm!(self.func, { i32_const(Imm32(1)); });
        }
        let mut offset: u32 = 0;
        for (i, (_, field_ty)) in fields.iter().enumerate() {
            let field_size = values::byte_size(field_ty);
            wasm!(self.func, { local_get(Local(a)); });
            self.emit_load_at(field_ty, offset);
            wasm!(self.func, { local_get(Local(b)); });
            self.emit_load_at(field_ty, offset);
            let field_clone = field_ty.clone();
            self.emit_eq_typed(&field_clone);
            if i > 0 {
                wasm!(self.func, { i32_and; });
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
            local_set(Local(b));
            local_set(Local(a));
            // tags equal? → if so compute payload eq, else 0. (No `return_`: it
            // returned 0 from the ENCLOSING function and corrupted its contract.)
            local_get(Local(a)); i32_load(0);
            local_get(Local(b)); i32_load(0);
            i32_eq;
            if_i32;
        });

        if cases.is_empty() || cases.iter().all(|c| c.fields.is_empty()) {
            // All unit variants — tags matched, so equal
            wasm!(self.func, { i32_const(Imm32(1)); });
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
                    wasm!(self.func, { local_get(Local(a)); });
                    self.emit_load_at(field_ty, offset);
                    wasm!(self.func, { local_get(Local(b)); });
                    self.emit_load_at(field_ty, offset);
                    let ft = field_ty.clone();
                    self.emit_eq_typed(&ft);
                    if i > 0 {
                        wasm!(self.func, { i32_and; });
                    }
                    offset += field_size;
                }
                if case.fields.is_empty() {
                    wasm!(self.func, { i32_const(Imm32(1)); });
                }
            } else {
                wasm!(self.func, { i32_const(Imm32(1)); });
            }
        }

        // Close the tags-equal `if_i32`: tags differ → not equal.
        wasm!(self.func, {
            else_;
            i32_const(Imm32(0));
            end;
        });

        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    pub(super) fn emit_cmp_instruction(&mut self, ty: &Ty, kind: CmpKind) {
        match (ty, kind) {
            (Ty::Int | Ty::Int64, CmpKind::Lt) => { wasm!(self.func, { i64_lt_s; }); }
            (Ty::Int | Ty::Int64, CmpKind::Gt) => { wasm!(self.func, { i64_gt_s; }); }
            (Ty::Int | Ty::Int64, CmpKind::Lte) => { wasm!(self.func, { i64_le_s; }); }
            (Ty::Int | Ty::Int64, CmpKind::Gte) => { wasm!(self.func, { i64_ge_s; }); }
            (Ty::UInt64, cmp_kind) => {
                use wasm_encoder::Instruction;
                self.func.instruction(&match cmp_kind {
                    CmpKind::Lt => Instruction::I64LtU,
                    CmpKind::Gt => Instruction::I64GtU,
                    CmpKind::Lte => Instruction::I64LeU,
                    CmpKind::Gte => Instruction::I64GeU,
                });
            }
            // Narrow sized ints ride in WASM i32. Sign is preserved by
            // the upstream `i32_load<N>_s/u`; at compare time, treat
            // signed variants as signed and unsigned as unsigned.
            // Bool rides in WASM i32 as 0/1; `false < true` (unsigned).
            (Ty::Bool, CmpKind::Lt) => { wasm!(self.func, { i32_lt_u; }); }
            (Ty::Bool, CmpKind::Gt) => { wasm!(self.func, { i32_gt_u; }); }
            (Ty::Bool, CmpKind::Lte) => { wasm!(self.func, { i32_le_u; }); }
            (Ty::Bool, CmpKind::Gte) => { wasm!(self.func, { i32_ge_u; }); }
            (Ty::Int8 | Ty::Int16 | Ty::Int32, CmpKind::Lt) => { wasm!(self.func, { i32_lt_s; }); }
            (Ty::Int8 | Ty::Int16 | Ty::Int32, CmpKind::Gt) => { wasm!(self.func, { i32_gt_s; }); }
            (Ty::Int8 | Ty::Int16 | Ty::Int32, CmpKind::Lte) => { wasm!(self.func, { i32_le_s; }); }
            (Ty::Int8 | Ty::Int16 | Ty::Int32, CmpKind::Gte) => { wasm!(self.func, { i32_ge_s; }); }
            (Ty::UInt8 | Ty::UInt16 | Ty::UInt32, CmpKind::Lt) => { wasm!(self.func, { i32_lt_u; }); }
            (Ty::UInt8 | Ty::UInt16 | Ty::UInt32, CmpKind::Gt) => { wasm!(self.func, { i32_gt_u; }); }
            (Ty::UInt8 | Ty::UInt16 | Ty::UInt32, CmpKind::Lte) => { wasm!(self.func, { i32_le_u; }); }
            (Ty::UInt8 | Ty::UInt16 | Ty::UInt32, CmpKind::Gte) => { wasm!(self.func, { i32_ge_u; }); }
            (Ty::Float | Ty::Float64, CmpKind::Lt) => { wasm!(self.func, { f64_lt; }); }
            (Ty::Float | Ty::Float64, CmpKind::Gt) => { wasm!(self.func, { f64_gt; }); }
            (Ty::Float | Ty::Float64, CmpKind::Lte) => { wasm!(self.func, { f64_le; }); }
            (Ty::Float | Ty::Float64, CmpKind::Gte) => { wasm!(self.func, { f64_ge; }); }
            (Ty::Float32, cmp_kind) => {
                use wasm_encoder::Instruction;
                self.func.instruction(&match cmp_kind {
                    CmpKind::Lt => Instruction::F32Lt,
                    CmpKind::Gt => Instruction::F32Gt,
                    CmpKind::Lte => Instruction::F32Le,
                    CmpKind::Gte => Instruction::F32Ge,
                });
            }
            (Ty::String, CmpKind::Lt) => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(Imm32(0)); i32_lt_s; });
            }
            (Ty::String, CmpKind::Gt) => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(Imm32(0)); i32_gt_s; });
            }
            (Ty::String, CmpKind::Lte) => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(Imm32(0)); i32_le_s; });
            }
            (Ty::String, CmpKind::Gte) => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); i32_const(Imm32(0)); i32_ge_s; });
            }
            // Variant (enum-like) comparison: each value is a pointer to a heap
            // block whose first i32 is the discriminant tag. For `Ord`-derived
            // variants, `Low < Medium` means `Low.tag < Medium.tag`, so we load
            // both tags and compare them as unsigned i32s.
            //
            // Stack on entry: [left_ptr, right_ptr]. We must load tags from both
            // pointers and compare, preserving WASM's strict stack discipline.
            // Use scratch locals to hold the pointers since WASM has no swap op.
            (Ty::Named(name, _), cmp_kind) if self.emitter.variant_info.contains_key(name.as_str()) => {
                let right = self.scratch.alloc_i32();
                let left = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(Local(right));
                    local_set(Local(left));
                    local_get(Local(left)); i32_load(0);
                    local_get(Local(right)); i32_load(0);
                });
                match cmp_kind {
                    CmpKind::Lt => { wasm!(self.func, { i32_lt_u; }); }
                    CmpKind::Gt => { wasm!(self.func, { i32_gt_u; }); }
                    CmpKind::Lte => { wasm!(self.func, { i32_le_u; }); }
                    CmpKind::Gte => { wasm!(self.func, { i32_ge_u; }); }
                }
                self.scratch.free_i32(left);
                self.scratch.free_i32(right);
            }
            // TypeVar/Unknown (#517): the AllTypesConcrete hard gate refuses
            // any build whose expression types are unresolved, and comparison
            // operands ARE expressions — nothing live can reach this arm. The
            // former runtime trap could bury a future gate hole as a silent
            // late crash; fail the build instead.
            (ty, _) if ty.is_unresolved() => {
                panic!(
                    "[ICE] emit_wasm: comparison on unresolved type `{:?}` reached emission — \
                     the AllTypesConcrete gate should have refused this build (#517)",
                    ty
                );
            }
            (l, _) => {
                // A RESOLVED type with no comparison emission is a
                // compiler bug — fail the build (§5: no silent traps).
                panic!(
                    "[ICE] emit_wasm: no equality/comparison emission for type `{:?}` — \
                     add an arm or reject upstream in the checker",
                    l
                );
            }
        }
    }

    /// Emit a type-directed *total order* comparison. Consumes `[a, b]` from the
    /// stack and leaves an i32 three-way result: negative if `a < b`, zero if
    /// `a == b`, positive if `a > b` — matching the native oracle, Rust's
    /// derived `Ord` (`xs.iter().min()` / `.sort()`).
    ///
    /// This is the ordering twin of [`emit_eq_typed`]: every WASM stdlib that
    /// needs a total order (`list.min`/`list.max`, and by routing `list.sort`)
    /// goes through this one emitter, so an unsupported element type is an
    /// emit-time ICE — never a silent wrong comparison. Recursive for the
    /// compound types (lexicographic for Tuple/List, `None < Some` for Option,
    /// `Ok < Err` for Result, tag-then-field for variants).
    ///
    /// For pointer-shaped leaves (`String`) the inputs are i32 pointers; for
    /// value leaves (`Int`, `Float`, `Bool`, sized ints) they are the scalar
    /// values themselves. Compound inputs are i32 heap pointers.
    pub(super) fn emit_ord_cmp3(&mut self, ty: &Ty) {
        use almide_lang::types::constructor::TypeConstructorId;
        match ty {
            // String pointers: the runtime cmp already returns sign(-/0/+).
            Ty::String | Ty::Bytes => {
                wasm!(self.func, { call(self.emitter.rt.string.cmp); });
            }
            // Scalar leaves: derive sign as `(a > b) - (a < b)`. The two
            // comparisons each consume the operand pair, so stash them in
            // typed scratch first and reload for each test.
            Ty::Int | Ty::Int64 | Ty::UInt64
            | Ty::Int8 | Ty::Int16 | Ty::Int32
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32
            | Ty::Bool => {
                self.emit_scalar_ord_sign(ty);
            }
            // Float total order (C-055): `f64` is not totally ordered by the
            // native `<`/`>` (NaN compares false on every axis, `-0.0 == +0.0`),
            // so list min/max/sort/sort_by-float-key route through IEEE-754
            // totalOrder — the ordering twin of native `f64::total_cmp`. Map
            // each f64 to a monotone i64 KEY (sign-magnitude bit flip) and take
            // the signed `(a > b) - (a < b)` sign on the keys; NaN lands at the
            // top, `-NaN` at the bottom, `-0.0 < +0.0`.
            Ty::Float | Ty::Float64 | Ty::Float32 => {
                self.emit_float_total_order_sign();
            }
            // Variant comparison reduces to tag order, then field order. The
            // existing variant `emit_cmp_instruction(Lt/Gt)` only compares
            // tags; for `Ord`-derived enums with payloads we must also tie-break
            // on the fields. We special-case the two builtin variant shapes
            // (Option/Result) below and fall back to tag-only for user enums,
            // which is exactly what native derives when every case is unit and
            // is the conservative choice otherwise.
            Ty::Applied(TypeConstructorId::Option, args) => {
                let inner = args.first().cloned().unwrap_or(Ty::Int);
                self.emit_option_ord_cmp3(&inner);
            }
            Ty::Applied(TypeConstructorId::Result, args) => {
                let ok = args.first().cloned().unwrap_or(Ty::Int);
                let err = args.get(1).cloned().unwrap_or(Ty::String);
                self.emit_result_ord_cmp3(&ok, &err);
            }
            Ty::Applied(TypeConstructorId::List, args) => {
                let elem = args.first().cloned().unwrap_or(Ty::Int);
                self.emit_list_ord_cmp3(&elem);
            }
            Ty::Tuple(elems) => {
                let elems = elems.clone();
                self.emit_tuple_ord_cmp3(&elems);
            }
            // User variants: order by discriminant tag (unsigned). Matches the
            // native derive for all-unit enums; payload tie-break is not yet
            // modelled, so we ICE if a payload-carrying user variant reaches an
            // ordering site (it cannot today — min/max/sort restrict to the
            // shapes above) rather than silently mis-order.
            Ty::Named(name, _) if self.emitter.variant_info.contains_key(name.as_str()) => {
                let cases = self.emitter.variant_info.get(name.as_str()).cloned().unwrap_or_default();
                let has_payload = cases.iter().any(|c| !c.fields.is_empty());
                if has_payload {
                    panic!(
                        "[ICE] emit_wasm: total-order comparison of payload-carrying \
                         variant `{}` is not modelled — extend emit_ord_cmp3",
                        name
                    );
                }
                // [a_ptr, b_ptr] → sign(a.tag - b.tag), tags are i32 at offset 0.
                let b = self.scratch.alloc_i32();
                let a = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(Local(b)); local_set(Local(a));
                    local_get(Local(a)); i32_load(0);
                    local_get(Local(b)); i32_load(0); i32_gt_u;
                    local_get(Local(a)); i32_load(0);
                    local_get(Local(b)); i32_load(0); i32_lt_u;
                    i32_sub;
                });
                self.scratch.free_i32(a);
                self.scratch.free_i32(b);
            }
            _ => panic!(
                "[ICE] emit_wasm: no total-order comparison for element type \
                 `{:?}` — extend emit_ord_cmp3",
                ty
            ),
        }
    }

    /// Scalar `sign(a - b)` for a value-typed leaf already on the stack as
    /// `[a, b]`. Spills both operands to a typed scratch slot so the two
    /// directional comparisons can reload them (WASM stack has no `dup`).
    fn emit_scalar_ord_sign(&mut self, ty: &Ty) {
        let vt = values::ty_to_valtype(ty).unwrap_or(ValType::I32);
        let b = self.scratch.alloc(vt);
        let a = self.scratch.alloc(vt);
        wasm!(self.func, { local_set(Local(b)); local_set(Local(a)); });
        // (a > b)
        wasm!(self.func, { local_get(Local(a)); local_get(Local(b)); });
        self.emit_cmp_instruction(ty, CmpKind::Gt);
        // (a < b)
        wasm!(self.func, { local_get(Local(a)); local_get(Local(b)); });
        self.emit_cmp_instruction(ty, CmpKind::Lt);
        // sign = (a>b) - (a<b)  ∈ {-1, 0, 1}
        wasm!(self.func, { i32_sub; });
        self.scratch.free(a, vt);
        self.scratch.free(b, vt);
    }

    /// IEEE-754 totalOrder sign for two f64s on the stack as `[a, b]`. Maps
    /// each f64 to a monotone i64 KEY, then takes the signed key sign
    /// `(ka > kb) - (ka < kb)` ∈ {-1, 0, 1}. The KEY transform is the exact
    /// twin of `f64::total_cmp` (so native == wasm byte-for-byte):
    ///
    /// ```text
    /// key = bits ^ ((bits >>_s 63) >>_u 1)
    /// ```
    ///
    /// A non-negative bit pattern is unchanged; a negative one has its lower
    /// 63 bits flipped (sign bit kept), which makes more-negative values sort
    /// below less-negative ones and places `-0.0` just below `+0.0`. NaN keeps
    /// its sign-extremal position. See C-055.
    fn emit_float_total_order_sign(&mut self) {
        let kb = self.scratch.alloc_i64();
        let ka = self.scratch.alloc_i64();
        // [a, b] → key(b) into kb, key(a) into ka.
        self.emit_f64_total_order_key();
        wasm!(self.func, { local_set(Local(kb)); });
        self.emit_f64_total_order_key();
        wasm!(self.func, { local_set(Local(ka)); });
        // sign = (ka > kb) - (ka < kb)
        wasm!(self.func, {
            local_get(Local(ka)); local_get(Local(kb)); i64_gt_s;
            local_get(Local(ka)); local_get(Local(kb)); i64_lt_s;
            i32_sub;
        });
        self.scratch.free_i64(ka);
        self.scratch.free_i64(kb);
    }

    /// `[f64]` → `[i64 total-order key]`. `key = bits ^ ((bits >>_s 63) >>_u 1)`,
    /// the same transform `f64::total_cmp` applies. (`>>_s` = arithmetic, `>>_u`
    /// = logical, per the Rust std source.) Also used by `sort_by` with a Float
    /// key, which pre-transforms each key so the parallel key array can ride the
    /// plain i64 storage/compare/swap path (C-055).
    pub(super) fn emit_f64_total_order_key(&mut self) {
        let bits = self.scratch.alloc_i64();
        wasm!(self.func, {
            i64_reinterpret_f64; local_set(Local(bits));
            local_get(Local(bits));
            // mask = (bits >>_s 63) >>_u 1
            local_get(Local(bits)); i64_const(Imm64(I64_SIGN_BIT_POS)); i64_shr_s; i64_const(Imm64(1)); i64_shr_u;
            i64_xor;
        });
        self.scratch.free_i64(bits);
    }

    /// Option ordering: `None < Some(_)`; two `Some` recurse on the inner type.
    /// Inputs `[a_ptr, b_ptr]` are nullable pointers (`0` == none).
    fn emit_option_ord_cmp3(&mut self, inner_ty: &Ty) {
        let b = self.scratch.alloc_i32();
        let a = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(Local(b)); local_set(Local(a));
            // a == none?
            local_get(Local(a)); i32_eqz;
            if_i32;
              // a none: none < some, none == none
              local_get(Local(b)); i32_eqz; if_i32; i32_const(Imm32(0)); else_; i32_const(Imm32(-1)); end;
            else_;
              // a some
              local_get(Local(b)); i32_eqz;
              if_i32; i32_const(Imm32(1)); // some > none
              else_;
                // both some: recurse on inner (loaded at offset 0)
                local_get(Local(a));
        });
        self.emit_load_at(inner_ty, 0);
        wasm!(self.func, { local_get(Local(b)); });
        self.emit_load_at(inner_ty, 0);
        let inner = inner_ty.clone();
        self.emit_ord_cmp3(&inner);
        wasm!(self.func, {
              end;
            end;
        });
        self.scratch.free_i32(a);
        self.scratch.free_i32(b);
    }

    /// Result ordering: derived `Ord` puts `Ok` before `Err` (variant index 0
    /// vs 1). Inputs `[a_ptr, b_ptr]` are pointers to `[tag:i32][payload]`,
    /// tag 0 == ok, tag 4-offset payload.
    fn emit_result_ord_cmp3(&mut self, ok_ty: &Ty, err_ty: &Ty) {
        let b = self.scratch.alloc_i32();
        let a = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(Local(b)); local_set(Local(a));
            // tags differ → order by tag (ok=0 < err=1)
            local_get(Local(a)); i32_load(0);
            local_get(Local(b)); i32_load(0);
            i32_ne;
            if_i32;
              local_get(Local(a)); i32_load(0);
              local_get(Local(b)); i32_load(0); i32_gt_u;
              local_get(Local(a)); i32_load(0);
              local_get(Local(b)); i32_load(0); i32_lt_u;
              i32_sub;
            else_;
              // same tag: recurse on the matching payload
              local_get(Local(a)); i32_load(0); i32_eqz;
              if_i32;
        });
        // ok payload at offset 4
        if matches!(ok_ty, Ty::Unit) {
            wasm!(self.func, { i32_const(Imm32(0)); });
        } else {
            wasm!(self.func, { local_get(Local(a)); });
            self.emit_load_at(ok_ty, 4);
            wasm!(self.func, { local_get(Local(b)); });
            self.emit_load_at(ok_ty, 4);
            let ok = ok_ty.clone();
            self.emit_ord_cmp3(&ok);
        }
        wasm!(self.func, { else_; });
        // err payload at offset 4
        if matches!(err_ty, Ty::Unit) {
            wasm!(self.func, { i32_const(Imm32(0)); });
        } else {
            wasm!(self.func, { local_get(Local(a)); });
            self.emit_load_at(err_ty, 4);
            wasm!(self.func, { local_get(Local(b)); });
            self.emit_load_at(err_ty, 4);
            let err = err_ty.clone();
            self.emit_ord_cmp3(&err);
        }
        wasm!(self.func, {
              end;
            end;
        });
        self.scratch.free_i32(a);
        self.scratch.free_i32(b);
    }

    /// Tuple ordering: lexicographic over fields, left to right. Inputs
    /// `[a_ptr, b_ptr]` are pointers to the packed field layout.
    fn emit_tuple_ord_cmp3(&mut self, elems: &[Ty]) {
        let res = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let a = self.scratch.alloc_i32();
        wasm!(self.func, { local_set(Local(b)); local_set(Local(a)); i32_const(Imm32(0)); local_set(Local(res)); });
        let mut offset = 0u32;
        // A single block we break out of on the first non-equal field.
        wasm!(self.func, { block_empty; });
        for (i, ety) in elems.iter().enumerate() {
            wasm!(self.func, { local_get(Local(a)); });
            self.emit_load_at(ety, offset);
            wasm!(self.func, { local_get(Local(b)); });
            self.emit_load_at(ety, offset);
            let ety_c = ety.clone();
            self.emit_ord_cmp3(&ety_c);
            wasm!(self.func, { local_set(Local(res)); });
            // if res != 0, break (last field need not test — falls through).
            if i + 1 < elems.len() {
                wasm!(self.func, { local_get(Local(res)); br_if(0); });
            }
            offset += values::byte_size(ety);
        }
        wasm!(self.func, { end; local_get(Local(res)); });
        self.scratch.free_i32(a);
        self.scratch.free_i32(b);
        self.scratch.free_i32(res);
    }

    /// List ordering: lexicographic over elements, then shorter list first on a
    /// common prefix (matching `Vec`'s derived `Ord`). Inputs `[a_ptr, b_ptr]`
    /// point to `[len:i32][cap:i32][data...]`.
    fn emit_list_ord_cmp3(&mut self, elem_ty: &Ty) {
        use super::engine::layout::{LIST, list as ll};
        let data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let es = values::byte_size(elem_ty) as i32;
        let res = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let alen = self.scratch.alloc_i32();
        let blen = self.scratch.alloc_i32();
        let minlen = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let a = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(Local(b)); local_set(Local(a));
            i32_const(Imm32(0)); local_set(Local(res));
            local_get(Local(a)); i32_load(0); local_set(Local(alen));
            local_get(Local(b)); i32_load(0); local_set(Local(blen));
            local_get(Local(alen)); local_get(Local(blen)); i32_lt_u;
            if_i32; local_get(Local(alen)); else_; local_get(Local(blen)); end;
            local_set(Local(minlen));
            i32_const(Imm32(0)); local_set(Local(i));
            block_empty; loop_empty;                                  // [A] exit, [B] loop
              local_get(Local(i)); local_get(Local(minlen)); i32_ge_u; br_if(1);    // i >= minlen → break
              local_get(Local(a)); i32_const(Imm32(data_off)); i32_add; local_get(Local(i)); i32_const(Imm32(es)); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        wasm!(self.func, {
              local_get(Local(b)); i32_const(Imm32(data_off)); i32_add; local_get(Local(i)); i32_const(Imm32(es)); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        let elem_c = elem_ty.clone();
        self.emit_ord_cmp3(&elem_c);
        wasm!(self.func, {
              local_set(Local(res));
              local_get(Local(res)); br_if(1);                               // non-equal element → break with res
              local_get(Local(i)); i32_const(Imm32(1)); i32_add; local_set(Local(i));
              br(0);
            end; end;                                                 // close B, A
            // Common prefix equal: order by length (sign(alen - blen)).
            local_get(Local(res)); i32_eqz;
            if_i32;
              local_get(Local(alen)); local_get(Local(blen)); i32_gt_u;
              local_get(Local(alen)); local_get(Local(blen)); i32_lt_u;
              i32_sub;
            else_;
              local_get(Local(res));
            end;
        });
        self.scratch.free_i32(a);
        self.scratch.free_i32(b);
        self.scratch.free_i32(minlen);
        self.scratch.free_i32(blen);
        self.scratch.free_i32(alen);
        self.scratch.free_i32(i);
        self.scratch.free_i32(res);
    }

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
                return VARIANT_TAG_SIZE + max_payload; // tag + max payload
            }
        }
        VARIANT_TAG_SIZE // fallback: tag only
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
