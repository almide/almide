//! Deep equality comparison for WASM codegen.
//!
//! Type-aware recursive equality for List, Option, Result, Tuple, Record, Variant.

use std::collections::HashMap;
use std::collections::BTreeMap;

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
                    wasm!(self.func, { i32_const(size as i32); call(self.emitter.rt.mem_eq); });
                } else {
                    self.emit_tuple_eq_deep(elems);
                }
            }

            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                let string_fields: Vec<(String, Ty)> = fields.iter().map(|(n, t)| (n.to_string(), t.clone())).collect();
                if fields.iter().all(|(_, t)| self.is_value_type(t)) {
                    let size = values::record_size(&string_fields);
                    wasm!(self.func, { i32_const(size as i32); call(self.emitter.rt.mem_eq); });
                } else {
                    // Field-by-field deep equality
                    self.emit_record_eq_deep(&string_fields);
                }
            }

            Ty::Named(name, type_args) => {
                // The stdlib JSON `Value` is an opaque runtime type — no
                // record_fields entry — so it used to fall through to the
                // childless-record `i32_eq` and compare POINTERS (two
                // separately-built `json.null()`s were "unequal"). Dispatch to
                // the deep structural runtime, mirroring native PartialEq.
                if name.as_str() == "Value" {
                    wasm!(self.func, { call(self.emitter.rt.value_eq); });
                    return;
                }
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
                i32_const(1); local_set(matched); // 1 until a mismatch is found
                block_empty; loop_empty;
                  local_get(i); local_get(a); i32_load(0); i32_ge_u; br_if(1);
                  // Load a[i]
                  local_get(a); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                  local_get(i); i32_const(elem_size as i32); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        // Load b[i]
        wasm!(self.func, {
                  local_get(b); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                  local_get(i); i32_const(elem_size as i32); i32_mul; i32_add;
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
                    i32_const(0); local_set(matched); br(2);
                  end;
                  local_get(i); i32_const(1); i32_add; local_set(i);
                  br(0);
                end; end;
                local_get(matched);
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
            local_set(b);
            local_set(a);
            // Same pointer → equal.
            local_get(a); local_get(b); i32_eq;
            if_i32; i32_const(1);
            else_;
              // Different size → not equal.
              local_get(a); i32_load(0); local_get(b); i32_load(0); i32_ne;
              if_i32; i32_const(0);
              else_;
                i32_const(1); local_set(result);
                local_get(a); i32_load(0); local_set(alen);
                local_get(a); i32_load(map_cap_off); local_set(acap);
        });
        self.emit_dict_entries_base(a, acap);
        wasm!(self.func, {
                local_set(aeb);
                i32_const(0); local_set(ai);
                block_empty; loop_empty;                                  // [A] a-exit, [B] a-loop
                  local_get(ai); local_get(alen); i32_ge_u; br_if(1);     // done iterating a (dense)
                  // Load this dense entry's key into the search-key register.
                  local_get(aeb); local_get(ai); i32_const(es as i32); i32_mul; i32_add;
        });
        self.emit_key_load(key_ty, 0);
        self.emit_search_key_store(key_ty, sk32, sk64);
        // Probe b for the key.
        wasm!(self.func, {
                  i32_const(0); local_set(found);
                  local_get(b); i32_load(map_cap_off); local_set(bcap);
                  local_get(bcap); i32_eqz;
                  if_empty; else_;                                        // [D] bcap==0 guard
        });
        self.emit_dict_index_base(b, bcap);
        wasm!(self.func, { local_set(bib); });
        self.emit_dict_entries_base(b, bcap);
        wasm!(self.func, { local_set(beb); });
        self.emit_search_key_load(key_ty, sk32, sk64);
        self.emit_hash_key(key_ty);
        self.emit_h1_h2(bcap, bidx, h2);
        wasm!(self.func, {
                    block_empty; loop_empty;                             // [E] probe-block, [F] probe-loop
                      local_get(b); i32_const(map_tags_off); i32_add; local_get(bidx); i32_add; i32_load8_u(0); local_set(tg);
                      local_get(tg); i32_eqz; br_if(1);                  // empty slot → key absent
                      local_get(tg); local_get(h2); i32_eq;
                      if_empty;                                          // [G] tag matches
                        // bei = index[bidx] - 1 (1-based pointer into dense entries)
                        local_get(bib); local_get(bidx); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
                        i32_load(0); i32_const(1); i32_sub; local_set(bei);
                        local_get(beb); local_get(bei); i32_const(es as i32); i32_mul; i32_add;
        });
        self.emit_key_load(key_ty, 0);
        self.emit_search_key_load(key_ty, sk32, sk64);
        self.emit_key_eq(key_ty);
        wasm!(self.func, {
                        if_empty; i32_const(1); local_set(found); br(3); end;   // found → exit probe-block
                      end;
                      local_get(bidx); i32_const(1); i32_add;
                      local_get(bcap); i32_const(1); i32_sub; i32_and;
                      local_set(bidx); br(0);
                    end; end;                                            // close F, E
                  end;                                                   // close D
                  // Key absent → maps differ.
                  local_get(found); i32_eqz;
                  if_empty; i32_const(0); local_set(result); br(2); end; // exit a-loop+block
        });
        // Compare the values (skip for valueless maps).
        if vs > 0 {
            wasm!(self.func, {
                  local_get(aeb); local_get(ai); i32_const(es as i32); i32_mul; i32_add; i32_const(ks as i32); i32_add;
            });
            self.emit_load_at(val_ty, 0);
            wasm!(self.func, {
                  local_get(beb); local_get(bei); i32_const(es as i32); i32_mul; i32_add; i32_const(ks as i32); i32_add;
            });
            self.emit_load_at(val_ty, 0);
            self.emit_eq_typed(val_ty);
            wasm!(self.func, {
                  i32_eqz;
                  if_empty; i32_const(0); local_set(result); br(2); end; // values differ → exit
            });
        }
        wasm!(self.func, {
                  local_get(ai); i32_const(1); i32_add; local_set(ai); br(0);
                end; end;                                                // close B, A
                local_get(result);
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
            local_set(b);
            local_set(a);
            local_get(a); local_get(b); i32_eq;
            if_i32; i32_const(1);
            else_;
              local_get(a); i32_load(0); local_get(b); i32_load(0); i32_ne;
              if_i32; i32_const(0);
              else_;
                i32_const(1); local_set(result);
                i32_const(0); local_set(ai);
                block_empty; loop_empty;                                 // [A] exit, [B] a-loop
                  local_get(ai); local_get(a); i32_load(0); i32_ge_u; br_if(1);
                  local_get(a); i32_const(data_off); i32_add; local_get(ai); i32_const(es as i32); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        wasm!(self.func, {
                  local_set(ea);
                  i32_const(0); local_set(found);
                  i32_const(0); local_set(bj);
                  block_empty; loop_empty;                               // [C] scan-block, [D] scan-loop
                    local_get(bj); local_get(b); i32_load(0); i32_ge_u; br_if(1);
                    local_get(b); i32_const(data_off); i32_add; local_get(bj); i32_const(es as i32); i32_mul; i32_add;
        });
        self.emit_load_at(elem_ty, 0);
        wasm!(self.func, { local_get(ea); });
        self.emit_eq_typed(elem_ty);
        wasm!(self.func, {
                    if_empty; i32_const(1); local_set(found); br(2); end; // found → exit scan-block
                    local_get(bj); i32_const(1); i32_add; local_set(bj); br(0);
                  end; end;                                              // close D, C
                  local_get(found); i32_eqz;
                  if_empty; i32_const(0); local_set(result); br(2); end; // absent → exit a-loop+block
                  local_get(ai); i32_const(1); i32_add; local_set(ai); br(0);
                end; end;                                                // close B, A
                local_get(result);
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
        });
        // Ty::Unit has no representation — skip loading and treat as equal.
        if matches!(ok_ty, Ty::Unit) {
            wasm!(self.func, { i32_const(1); });
        } else {
            wasm!(self.func, { local_get(a); });
            self.emit_load_at(ok_ty, 4);
            wasm!(self.func, { local_get(b); });
            self.emit_load_at(ok_ty, 4);
            let ok_clone = ok_ty.clone();
            self.emit_eq_typed(&ok_clone);
        }
        wasm!(self.func, {
              else_;
                // tag==1 (err): compare err values
        });
        if matches!(err_ty, Ty::Unit) {
            wasm!(self.func, { i32_const(1); });
        } else {
            wasm!(self.func, { local_get(a); });
            self.emit_load_at(err_ty, 4);
            wasm!(self.func, { local_get(b); });
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
            local_set(b); // b
            local_set(a); // a
        });
        // AND every field's deep eq. NOT a `return_` short-circuit — that returned
        // from the ENCLOSING function and corrupted its contract on a mismatch
        // (e.g. a tuple with an unequal String element). Equality has no side
        // effects, so evaluating all fields and AND-ing is equivalent and safe.
        if elems.is_empty() {
            wasm!(self.func, { i32_const(1); });
        }
        let mut offset: u32 = 0;
        for (i, elem_ty) in elems.iter().enumerate() {
            let elem_size = values::byte_size(elem_ty);
            wasm!(self.func, { local_get(a); });
            self.emit_load_at(elem_ty, offset);
            wasm!(self.func, { local_get(b); });
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
            local_set(b);
            local_set(a);
        });
        // AND every field's deep eq (see emit_tuple_eq_deep — `return_` corrupted
        // the enclosing function on a mismatch).
        if fields.is_empty() {
            wasm!(self.func, { i32_const(1); });
        }
        let mut offset: u32 = 0;
        for (i, (_, field_ty)) in fields.iter().enumerate() {
            let field_size = values::byte_size(field_ty);
            wasm!(self.func, { local_get(a); });
            self.emit_load_at(field_ty, offset);
            wasm!(self.func, { local_get(b); });
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
            local_set(b);
            local_set(a);
            // tags equal? → if so compute payload eq, else 0. (No `return_`: it
            // returned 0 from the ENCLOSING function and corrupted its contract.)
            local_get(a); i32_load(0);
            local_get(b); i32_load(0);
            i32_eq;
            if_i32;
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
                    if i > 0 {
                        wasm!(self.func, { i32_and; });
                    }
                    offset += field_size;
                }
                if case.fields.is_empty() {
                    wasm!(self.func, { i32_const(1); });
                }
            } else {
                wasm!(self.func, { i32_const(1); });
            }
        }

        // Close the tags-equal `if_i32`: tags differ → not equal.
        wasm!(self.func, {
            else_;
            i32_const(0);
            end;
        });

        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }
}

include!("equality_p2.rs");
include!("equality_p3.rs");
