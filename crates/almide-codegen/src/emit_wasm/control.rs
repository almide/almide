//! Control flow emission — for-in loops and match expressions.

use almide_ir::{IrExpr, IrExprKind, IrMatchArm, IrPattern, IrStmt};
use almide_lang::types::Ty;
use wasm_encoder::{Instruction, ValType};

use super::FuncCompiler;
use super::values;
use super::wasm_macro::wasm;

impl FuncCompiler<'_> {
    /// Emit a for...in loop. Currently supports Range iterables only.
    pub(super) fn emit_for_in(&mut self, var: almide_ir::VarId, var_tuple: Option<&[almide_ir::VarId]>, iterable: &IrExpr, body: &[IrStmt]) {
        match &iterable.kind {
            IrExprKind::Range { start, end, inclusive } => {
                let loop_var = self.var_map[&var.0];

                // Initialize loop variable to start
                self.emit_expr(start);
                wasm!(self.func, { local_set(loop_var); });

                // block $break { loop $loop { check; block $continue { body }; i++; br $loop } }
                wasm!(self.func, { block_empty; });
                let break_guard = self.depth_push();

                wasm!(self.func, { loop_empty; });
                let loop_guard = self.depth_push();

                // Break condition
                wasm!(self.func, { local_get(loop_var); });
                self.emit_expr(end);
                if *inclusive {
                    wasm!(self.func, { i64_gt_s; });
                } else {
                    wasm!(self.func, { i64_ge_s; });
                }
                wasm!(self.func, { br_if(self.depth - break_guard.saved() - 1); });

                // Inner block for continue target
                wasm!(self.func, { block_empty; });
                let continue_guard = self.depth_push();

                self.loop_stack.push(super::LoopLabels { break_depth: break_guard.saved(), continue_depth: continue_guard.saved() });

                // Body
                for stmt in body {
                    self.emit_stmt(stmt);
                }

                self.loop_stack.pop();
                self.depth_pop(continue_guard);
                wasm!(self.func, { end; }); // end continue block

                // Increment: var += 1 (always runs, even after continue)
                wasm!(self.func, {
                    local_get(loop_var);
                    i64_const(1);
                    i64_add;
                    local_set(loop_var);
                });

                // Loop back
                wasm!(self.func, { br(self.depth - loop_guard.saved() - 1); });

                self.depth_pop(loop_guard);
                wasm!(self.func, { end; }); // end loop
                self.depth_pop(break_guard);
                wasm!(self.func, { end; }); // end break block
            }
            _ => {
                // Detect Map iterable
                let is_map = matches!(
                    &iterable.ty,
                    Ty::Applied(almide_lang::types::TypeConstructorId::Map, _)
                );

                if is_map {
                    self.emit_for_in_map(var, var_tuple, iterable, body);
                } else {
                    self.emit_for_in_list(var, var_tuple, iterable, body);
                }
            }
        }
    }

    /// Emit for-in over a list (or set). Layout: [len:i32][cap:i32][data @ 8...]
    fn emit_for_in_list(&mut self, var: almide_ir::VarId, var_tuple: Option<&[almide_ir::VarId]>, iterable: &IrExpr, body: &[IrStmt]) {
        let list_scratch = self.scratch.alloc_i32();
        let idx_scratch = self.scratch.alloc_i32();
        let loop_var = self.var_map[&var.0];

        let elem_ty = self.var_table.get(var).ty.clone();
        let entry_size = values::byte_size(&elem_ty);

        self.emit_expr(iterable);
        wasm!(self.func, { local_set(list_scratch); });
        wasm!(self.func, { i32_const(0); local_set(idx_scratch); });

        // block $break { loop $loop { ... } }
        wasm!(self.func, { block_empty; });
        let break_guard = self.depth_push();
        wasm!(self.func, { loop_empty; });
        let loop_guard = self.depth_push();

        // Break if idx >= len
        wasm!(self.func, {
            local_get(idx_scratch);
            local_get(list_scratch); i32_load(0);
            i32_ge_u;
            br_if(self.depth - break_guard.saved() - 1);
        });

        // Load element from data[idx]
        wasm!(self.func, {
            local_get(list_scratch);
            i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32);
            i32_add;
            local_get(idx_scratch);
            i32_const(entry_size as i32);
            i32_mul;
            i32_add;
        });
        self.emit_load_at(&elem_ty, 0);
        wasm!(self.func, { local_set(loop_var); });

        // Tuple destructure (for list of tuples)
        if let Some(tuple_vars) = var_tuple {
            if let Ty::Tuple(elem_types) = &elem_ty {
                // #524: offset advance unconditional (a missing local must
                // not shift every later field's load).
                let mut field_offset = 0u32;
                for (i, &tv) in tuple_vars.iter().enumerate() {
                    let ft = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                    if let Some(&local_idx) = self.var_map.get(&tv.0) {
                        wasm!(self.func, { local_get(loop_var); });
                        self.emit_load_at(&ft, field_offset);
                        wasm!(self.func, { local_set(local_idx); });
                    }
                    field_offset += values::byte_size(&ft);
                }
            }
        }

        // block $continue { body }
        wasm!(self.func, { block_empty; });
        let continue_guard = self.depth_push();
        self.loop_stack.push(super::LoopLabels { break_depth: break_guard.saved(), continue_depth: continue_guard.saved() });

        for stmt in body { self.emit_stmt(stmt); }

        self.loop_stack.pop();
        self.depth_pop(continue_guard);
        wasm!(self.func, { end; }); // end continue block

        // idx++; br $loop
        wasm!(self.func, {
            local_get(idx_scratch); i32_const(1); i32_add; local_set(idx_scratch);
        });
        wasm!(self.func, { br(self.depth - loop_guard.saved() - 1); });

        self.depth_pop(loop_guard);
        wasm!(self.func, { end; }); // end loop
        self.depth_pop(break_guard);
        wasm!(self.func, { end; }); // end break block
        self.scratch.free_i32(idx_scratch);
        self.scratch.free_i32(list_scratch);
    }

    /// Emit for-in over a Map (compact-ordered-dict).
    /// Layout: [len:i32][cap:i32][tags: cap bytes][index: cap×4][entries: cap × (key+val)]
    /// Walks the dense entries[0..len] in insertion order (no tag scan).
    fn emit_for_in_map(&mut self, var: almide_ir::VarId, var_tuple: Option<&[almide_ir::VarId]>, iterable: &IrExpr, body: &[IrStmt]) {

        let (key_ty, val_ty, key_size, val_size) = if let Ty::Applied(_, args) = &iterable.ty {
            let kt = args.first().cloned().unwrap_or(Ty::String);
            let vt = args.get(1).cloned().unwrap_or(Ty::Int);
            let ks = values::byte_size(&kt);
            let vs = values::byte_size(&vt);
            (kt, vt, ks, vs)
        } else {
            (Ty::String, Ty::Int, 4u32, 4u32)
        };
        let entry_size = key_size + val_size;
        let map_cap_off = self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::CAP);

        let m = self.scratch.alloc_i32();      // map ptr
        let cap = self.scratch.alloc_i32();     // capacity (to derive the entries base)
        let len = self.scratch.alloc_i32();     // entry count (dense walk bound)
        let eb = self.scratch.alloc_i32();      // dense entries base
        let idx = self.scratch.alloc_i32();     // dense entry index (0..len)

        self.emit_expr(iterable);
        wasm!(self.func, {
            local_set(m);
            local_get(m); i32_load(0); local_set(len);
            local_get(m); i32_load(map_cap_off); local_set(cap);
        });
        self.emit_dict_entries_base(m, cap);
        wasm!(self.func, { local_set(eb); i32_const(0); local_set(idx); });

        // block $break { loop $loop { ... } }
        wasm!(self.func, { block_empty; });
        let break_guard = self.depth_push();
        wasm!(self.func, { loop_empty; });
        let loop_guard = self.depth_push();

        // Break if idx >= len (dense entries are all occupied — no tag skip)
        wasm!(self.func, {
            local_get(idx); local_get(len); i32_ge_u;
            br_if(self.depth - break_guard.saved() - 1);
        });

        // Dense entry: load key/value into tuple vars
        if let Some(tuple_vars) = var_tuple {
            // Load key
            if let Some(&k_local) = tuple_vars.first().and_then(|tv| self.var_map.get(&tv.0)) {
                wasm!(self.func, {
                    local_get(eb);
                    local_get(idx); i32_const(entry_size as i32); i32_mul; i32_add;
                });
                self.emit_load_at(&key_ty, 0);
                wasm!(self.func, { local_set(k_local); });
            }

            // Load value
            if let Some(&v_local) = tuple_vars.get(1).and_then(|tv| self.var_map.get(&tv.0)) {
                wasm!(self.func, {
                    local_get(eb);
                    local_get(idx); i32_const(entry_size as i32); i32_mul; i32_add;
                });
                self.emit_load_at(&val_ty, key_size);
                wasm!(self.func, { local_set(v_local); });
            }
        }

        // block $continue { body }
        wasm!(self.func, { block_empty; });
        let continue_guard = self.depth_push();
        self.loop_stack.push(super::LoopLabels { break_depth: break_guard.saved(), continue_depth: continue_guard.saved() });

        for stmt in body { self.emit_stmt(stmt); }

        self.loop_stack.pop();
        self.depth_pop(continue_guard);
        wasm!(self.func, { end; }); // end continue block

        // idx++; br $loop
        wasm!(self.func, {
            local_get(idx); i32_const(1); i32_add; local_set(idx);
        });
        wasm!(self.func, { br(self.depth - loop_guard.saved() - 1); });

        self.depth_pop(loop_guard);
        wasm!(self.func, { end; }); // end loop
        self.depth_pop(break_guard);
        wasm!(self.func, { end; }); // end break block

        self.scratch.free_i32(idx);
        self.scratch.free_i32(eb);
        self.scratch.free_i32(len);
        self.scratch.free_i32(cap);
        self.scratch.free_i32(m);
    }

    /// Emit a match arm body, wrapping in Ok() if the arm returns a naked value
    /// but the match result type is Result.
    fn emit_match_arm_body(&mut self, body: &IrExpr, result_ty: &Ty) {
        use almide_lang::types::constructor::TypeConstructorId;
        let result_is_result = matches!(result_ty, Ty::Applied(TypeConstructorId::Result, _));
        let body_is_result = matches!(&body.ty, Ty::Applied(TypeConstructorId::Result, _));
        let body_is_err = matches!(&body.kind, IrExprKind::ResultErr { .. });

        // Also check: if any sibling arm returns Result but result_ty says Int,
        // wrap this arm in Ok() too (type checker sometimes infers Int instead of Result).
        let needs_wrap = if result_is_result && !body_is_result && !body_is_err {
            true
        } else if !result_is_result && !body_is_result && body_is_err {
            // err() in arm but result_ty is not Result — shouldn't happen, but guard
            false
        } else {
            false
        };

        if needs_wrap {
            // Naked value in Result-typed match arm → wrap in Ok()
            let wrapped = IrExpr {
                kind: IrExprKind::ResultOk { expr: Box::new(body.clone()) },
                ty: result_ty.clone(),
                span: body.span, def_id: None,
            };
            self.emit_expr(&wrapped);
        } else {
            self.emit_expr(body);
        }
    }

    /// Emit a match expression as a chain of if-else checks.
    ///
    /// Strategy: store subject in a scratch local, then for each arm:
    /// - Literal pattern: compare subject to literal, branch if equal
    /// - Wildcard: unconditional (last arm)
    /// - Bind: store subject in the bound variable's local, unconditional
    pub(super) fn emit_match(&mut self, subject: &IrExpr, arms: &[IrMatchArm], result_ty: &Ty) {
        let subject_ty = self.resolve_subject_type(subject, arms);

        // Resolve result_ty: trust arm body types over expr.ty when they disagree.
        // Checker's Union-Find can contaminate match result vars (e.g., binding to Int
        // when the actual result is Either[String, Int] = i32 pointer).
        // If any arm returns ResultErr/ResultOk, the match result must be Result (i32 pointer).
        // The type checker sometimes infers the non-Result arm type (e.g., Int) instead.
        let has_result_arm = arms.iter().any(|a| matches!(&a.body.ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _)));
        let has_result_err = arms.iter().any(|a| matches!(&a.body.kind,
            IrExprKind::ResultErr { .. }));

        let resolved_result = if (has_result_arm || has_result_err) && !matches!(result_ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _)) {
            // Promote to Result type from the arm that has it
            arms.iter()
                .find(|a| matches!(&a.body.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _)))
                .map(|a| a.body.ty.clone())
                .unwrap_or_else(|| result_ty.clone())
        } else if !arms.is_empty() {
            let arm_vts: Vec<Option<ValType>> = arms.iter().map(|a| values::ty_to_valtype(&a.body.ty)).collect();
            let result_vt = values::ty_to_valtype(result_ty);
            if let Some(first_vt) = arm_vts[0] {
                if arm_vts.iter().all(|vt| *vt == Some(first_vt)) && result_vt != Some(first_vt) {
                    arms[0].body.ty.clone()
                } else {
                    result_ty.clone()
                }
            } else {
                result_ty.clone()
            }
        } else {
            result_ty.clone()
        };
        let result_ty = &resolved_result;

        self.emit_expr(subject);

        let is_i64 = matches!(values::ty_to_valtype(&subject_ty), Some(ValType::I64));
        let scratch = if is_i64 {
            self.scratch.alloc_i64()
        } else {
            self.scratch.alloc_i32()
        };

        wasm!(self.func, { local_set(scratch); });

        self.emit_match_arms(arms, scratch, &subject_ty, result_ty, 0);

        if is_i64 {
            self.scratch.free_i64(scratch);
        } else {
            self.scratch.free_i32(scratch);
        }
    }

    /// Resolve the actual subject type, fixing IR type inference gaps.
    /// If the subject Var has the wrong type but patterns indicate a container type, fix it.
    pub(super) fn resolve_subject_type(&self, subject: &IrExpr, arms: &[IrMatchArm]) -> Ty {
        let ty = &subject.ty;
        // If patterns are Ok/Err/Some/None but subject type isn't a container, look up VarTable
        let has_container_pattern = arms.iter().any(|a| matches!(
            &a.pattern,
            IrPattern::Ok { .. } | IrPattern::Err { .. } | IrPattern::Some { .. } | IrPattern::None
        ));
        if has_container_pattern && !matches!(ty, Ty::Applied(_, _)) {
            // Try to get the real type from VarTable
            if let IrExprKind::Var { id } = &subject.kind {
                let info = self.var_table.get(*id);
                if matches!(&info.ty, Ty::Applied(_, _)) {
                    return info.ty.clone();
                }
            }
        }
        ty.clone()
    }
}

include!("control_p2.rs");
include!("control_p3.rs");
