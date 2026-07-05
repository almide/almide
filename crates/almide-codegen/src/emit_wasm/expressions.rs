//! IrExpr → WASM instruction emission.

use almide_ir::{BinOp, IrExpr, IrExprKind, UnOp};
use almide_lang::types::Ty;
use wasm_encoder::{Instruction, ValType};

use super::FuncCompiler;
use super::values;
use crate::pass_closure_conversion::is_inplace_mutator;

#[derive(Clone, Copy)]
pub(super) enum CmpKind {
    Lt,
    Gt,
    Lte,
    Gte,
}

impl FuncCompiler<'_> {
    /// Resolve a VarId to its wasm GLOBAL, the one rule shared by every
    /// consumer (value read, closure capture): id-keyed first, then the
    /// module_origin-reconstructed `ALMIDE_RT_<MOD>_<NAME>` key, then the
    /// bare VarTable name. Lowering produces CLEAN names + module_origin
    /// (the #486-era rework) — any consumer doing only one of these lookups
    /// re-creates the #500 silent-zero class.
    pub(super) fn lookup_global(&self, id: almide_ir::VarId) -> Option<(u32, ValType)> {
        if let Some(&entry) = self.emitter.top_let_globals.get(&id.0) {
            return Some(entry);
        }
        // §4 Stage 2: alias-resolve a synthetic cross-module use-site id to
        // its declaration id — VarId-keyed, no name reconstruction.
        if let Some(decl) = self.emitter.global_alias.get(&id.0) {
            if let Some(&entry) = self.emitter.top_let_globals.get(decl) {
                return Some(entry);
            }
        }
        if (id.0 as usize) < self.var_table.len() {
            let vi = self.var_table.get(id);
            if let Some(origin) = vi.module_origin.as_deref() {
                let key = format!(
                    "ALMIDE_RT_{}_{}",
                    origin.to_uppercase().replace('.', "_"),
                    vi.name.as_str().to_uppercase(),
                );
                if let Some(&entry) = self.emitter.top_let_globals_by_name.get(&key) {
                    return Some(entry);
                }
            }
            if let Some(&entry) = self.emitter.top_let_globals_by_name.get(vi.name.as_str()) {
                return Some(entry);
            }
        }
        None
    }

    /// Emit `expr` as a value that is about to be MOVE-STORED into a constructor
    /// or value-builder (a record field, list/tuple element, Option/Result
    /// payload, or a `value.str/array/object` box). These builders take the raw
    /// pointer by reference without copying, so if `expr` yields a borrowed heap
    /// ALIAS — a field of a borrowed param (`self.name`), an extracted element,
    /// or a local that owns it elsewhere — the container must acquire its OWN
    /// reference. Otherwise freeing the container later double-frees a value its
    /// source still holds. `__rc_inc` is stack-neutral (ptr → ptr) and a runtime
    /// no-op on data-section constants; gating on `yields_borrowed_alias` keeps
    /// us from leaking fresh heap values (an extra ref no owner ever releases).
    pub(super) fn emit_stored_field(&mut self, expr: &IrExpr) {
        self.emit_expr(expr);
        if crate::pass_perceus::is_heap_type(&expr.ty)
            && crate::pass_perceus::yields_borrowed_alias(expr)
        {
            wasm!(self.func, { call(self.emitter.rt.rc_inc); });
        }
    }

    /// Emit WASM instructions for an IR expression.
    /// Leaves the result value on the WASM stack (nothing for Unit).
    pub fn emit_expr(&mut self, expr: &IrExpr) {
        // Disjoint sub-match groups (chain order is irrelevant — every arm
        // matches exactly one group). Each returns `true` once it has emitted.
        if self.emit_expr_g2(expr) { return; }
        if self.emit_expr_g3(expr) { return; }
        match &expr.kind {
            // ── Literals ──
            IrExprKind::LitInt { value } => {
                // Pick the WASM numeric instruction from the literal's
                // ty (Sized Numeric Types Stage 1b). Narrow signed /
                // unsigned ints ride in `i32_const`; `UInt64` uses
                // `i64_const` like the canonical `Int`.
                match &expr.ty {
                    Ty::Int8 | Ty::Int16 | Ty::Int32
                    | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 => {
                        wasm!(self.func, { i32_const(*value as i32); });
                    }
                    _ => {
                        wasm!(self.func, { i64_const(*value); });
                    }
                }
            }
            IrExprKind::LitFloat { value } => {
                if matches!(expr.ty, Ty::Float32) {
                    self.func.instruction(&wasm_encoder::Instruction::F32Const(
                        (*value as f32).into(),
                    ));
                } else {
                    wasm!(self.func, { f64_const(*value); });
                }
            }
            IrExprKind::LitBool { value } => {
                wasm!(self.func, { i32_const(*value as i32); });
            }
            IrExprKind::LitStr { value } => {
                let offset = self.emitter.intern_string(value);
                wasm!(self.func, { i32_const(offset as i32); });
            }
            IrExprKind::Unit => {
                // Unit produces no value on the stack
            }

            // ── Variables ──
            IrExprKind::Var { id } => {
                // A Unit-typed var has NO WASM representation (ty_to_valtype
                // = None): loading its physical local would push a value the
                // type system says does not exist. The §2 matrix gate caught
                // exactly that — a Unit tail var inside result_ok left two
                // values on the stack (invalid module); wasm-opt had been
                // silently repairing it on machines that have it.
                if matches!(expr.ty, Ty::Unit) { return; }
                // DefId-based resolution (highest priority): direct cross-package global lookup
                if let Some(def_id) = expr.def_id {
                    if let Some(&(global_idx, _)) = self.emitter.def_globals.get(&def_id.0) {
                        wasm!(self.func, { global_get(global_idx); });
                        return;
                    }
                }
                if let Some(&local_idx) = self.var_map.get(&id.0) {
                    if self.emitter.mutable_captures.contains(&id.0) {
                        // Mutable capture: local holds cell ptr, deref to get value
                        wasm!(self.func, { local_get(local_idx); });
                        self.emit_load_at(&expr.ty, 0);
                    } else {
                        wasm!(self.func, { local_get(local_idx); });
                    }
                } else if let Some(&(global_idx, _)) = {
                    // Name-based lookup FIRST: cross-module synthetic Vars
                    // must resolve by name because their VarIds can collide
                    // with unrelated globals after unification. Lowering
                    // produces a CLEAN uppercase name + module_origin (the
                    // #486-era rework) — the prefixed ALMIDE_RT_<MOD>_<NAME>
                    // key must be reconstructed from module_origin here, or
                    // a cross-module global read silently missed every key
                    // and fell to the typed-zero fallback (#500).
                    if (id.0 as usize) < self.var_table.len() {
                        let vi = self.var_table.get(*id);
                        let by_origin = vi.module_origin.as_deref().and_then(|origin| {
                            let key = format!(
                                "ALMIDE_RT_{}_{}",
                                origin.to_uppercase().replace('.', "_"),
                                vi.name.as_str().to_uppercase(),
                            );
                            self.emitter.top_let_globals_by_name.get(&key)
                        });
                        by_origin.or_else(|| self.emitter.top_let_globals_by_name.get(vi.name.as_str()))
                    } else { None }
                } {
                    wasm!(self.func, { global_get(global_idx); });
                } else if let Some(&(global_idx, _)) = self.emitter.top_let_globals.get(&id.0) {
                    wasm!(self.func, { global_get(global_idx); });
                } else {
                    // VarId not in var_map — try name-based lookup as fallback
                    // (handles VarId mismatch between lowering passes)
                    let name = if (id.0 as usize) < self.var_table.len() { &self.var_table.get(*id).name } else { "" };
                    let found = if !name.is_empty() {
                        let target_vt = values::ty_to_valtype(&expr.ty);
                        // Find var_map entry with matching name, prefer matching WASM type
                        // Deterministic selection (#524): prefer a valtype
                        // match, tie-break by SMALLEST VarId — the former
                        // bare max_by_key over HashMap iteration made the
                        // chosen local (and thus the emitted bytes) depend
                        // on hash order, violating host-determinism.
                        self.var_map.iter()
                            .filter(|(vid, _)| (**vid as usize) < self.var_table.len() && self.var_table.get(almide_ir::VarId(**vid)).name == name)
                            .min_by_key(|(vid, _)| {
                                let vid_vt = values::ty_to_valtype(&self.var_table.get(almide_ir::VarId(**vid)).ty);
                                (if vid_vt == target_vt { 0u8 } else { 1u8 }, **vid)
                            })
                            .map(|(_, lidx)| *lidx)
                    } else { None };
                    if let Some(local_idx) = found {
                        wasm!(self.func, { local_get(local_idx); });
                    } else {
                        // A module-origin var that reached this point missed
                        // every global key — that is a compiler bug, and the
                        // old typed-zero fallback turned it into SILENT WRONG
                        // OUTPUT (a cross-module `var` read printed 0 / empty,
                        // #500). Refuse loudly instead.
                        if (id.0 as usize) < self.var_table.len() {
                            let vi = self.var_table.get(*id);
                            if vi.module_origin.is_some() {
                                panic!(
                                    "[ICE] cross-module global `{}` (origin {:?}) resolved to no wasm global —                                      storage registration and lookup disagree (#500 class)",
                                    vi.name.as_str(), vi.module_origin
                                );
                            }
                        }
                        // Truly not in scope — push typed zero
                        match values::ty_to_valtype(&expr.ty) {
                            Some(ValType::I64) => { wasm!(self.func, { i64_const(0); }); }
                            Some(ValType::F64) => { wasm!(self.func, { f64_const(0.0); }); }
                            Some(ValType::I32) => { wasm!(self.func, { i32_const(0); }); }
                            _ => {}
                        }
                    }
                }
            }

            // ── Function reference (used as value) → closure [wrapper_table_idx, 0] ──
            IrExprKind::FnRef { name } => {
                self.emit_fn_ref_closure(name);
            }

            // ── Lambda → closure [table_idx, env_ptr] ──
            IrExprKind::Lambda { params, body, lambda_id } => {
                self.emit_lambda_closure(params, body, *lambda_id);
            }

            // ── ClosureCreate (from closure conversion pass) ──
            IrExprKind::ClosureCreate { func_name, captures } => {
                self.emit_closure_create(func_name, captures);
            }

            // ── EnvLoad (read captured var from env pointer) ──
            IrExprKind::EnvLoad { env_var, index } => {
                let offset = (*index) * 8;
                if let Some(&local_idx) = self.var_map.get(&env_var.0) {
                    wasm!(self.func, { local_get(local_idx); });
                } else {
                    // env_var should be local 0 in lifted functions (first param)
                    wasm!(self.func, { local_get(0); });
                }
                // Load value from env at offset
                match super::values::ty_to_valtype(&expr.ty) {
                    Some(wasm_encoder::ValType::I64) => {
                        self.func.instruction(&wasm_encoder::Instruction::I64Load(
                            wasm_encoder::MemArg { offset: offset as u64, align: 3, memory_index: 0 }
                        ));
                    }
                    Some(wasm_encoder::ValType::F64) => {
                        self.func.instruction(&wasm_encoder::Instruction::F64Load(
                            wasm_encoder::MemArg { offset: offset as u64, align: 3, memory_index: 0 }
                        ));
                    }
                    _ => {
                        self.func.instruction(&wasm_encoder::Instruction::I32Load(
                            wasm_encoder::MemArg { offset: offset as u64, align: 2, memory_index: 0 }
                        ));
                    }
                }
            }

            // ── Binary operators ──
            IrExprKind::BinOp { op, left, right } => {
                self.emit_binop(*op, left, right);
            }

            // ── Unary operators ──
            IrExprKind::UnOp { op, operand } => {
                match op {
                    UnOp::NegInt => {
                        wasm!(self.func, { i64_const(0); });
                        self.emit_expr(operand);
                        wasm!(self.func, { i64_sub; });
                    }
                    UnOp::NegFloat => {
                        self.emit_expr(operand);
                        wasm!(self.func, { f64_neg; });
                    }
                    UnOp::Not => {
                        self.emit_expr(operand);
                        wasm!(self.func, { i32_eqz; });
                    }
                }
            }

            // ── If/else ──
            IrExprKind::If { cond, then, else_ } => {
                self.emit_expr(cond);
                let bt = values::block_type(&expr.ty);
                self.func.instruction(&Instruction::If(bt));
                let _g0 = self.depth_push();
                self.emit_expr(then);
                wasm!(self.func, { else_; });
                self.emit_expr(else_);
                self.depth_pop(_g0);
                wasm!(self.func, { end; });
            }

            // ── Block ──
            IrExprKind::Block { stmts, expr: tail } => {
                for stmt in stmts {
                    self.emit_stmt(stmt);
                }
                if let Some(e) = tail {
                    self.emit_expr(e);
                    // Perceus inserts Ret(var) in void blocks — drop if needed.
                    if values::ty_to_valtype(&expr.ty).is_none()
                        && values::ty_to_valtype(&e.ty).is_some()
                    {
                        wasm!(self.func, { drop; });
                    }
                }
            }

            // ── While loop ──
            IrExprKind::While { cond, body } => {
                // Peephole: while i < N { s = s + "x"; i = i + 1 }
                // Hoist len/cap into locals for zero-reload tight loop.
                if self.try_emit_string_append_loop(cond, body) {
                    return;
                }

                // Phase 2a: iteration-level region scope.
                // If the loop body doesn't assign heap values to outer-scope
                // variables, scope each iteration to reclaim temporaries.
                let loop_body_expr = almide_ir::IrExpr {
                    kind: almide_ir::IrExprKind::Block {
                        stmts: body.to_vec(),
                        expr: None,
                    },
                    ty: almide_lang::types::Ty::Unit,
                    span: None,
                    def_id: None,
                };
                // Only enable iter_scope when the loop body actually allocates
                // heap memory (string/list/record/map construction or calls
                // returning heap types). Pure-int loops like fib skip the
                // save/restore entirely.
                //
                // Under real frees the restore must ALSO reset the free list
                // (emitted below): the region invariant ("no heap value
                // escapes the iteration") must cover ALLOCATOR STATE too.
                // Rolling back only the bump pointer left freed nodes above
                // the new frontier; the next iteration's bumps re-handed out
                // addresses the free list still referenced — observed as a
                // string's len field reading a free-list next pointer.
                let iter_scope = !body.is_empty()
                    && !self.expr_writes_outer_heap(&loop_body_expr)
                    && self.expr_allocates_heap(&loop_body_expr);
                let iter_scope_local = if iter_scope {
                    Some(self.scratch.alloc_i32())
                } else { None };

                wasm!(self.func, { block_empty; });
                let _g3 = self.depth_push();
                let break_depth = _g3.saved();

                wasm!(self.func, { loop_empty; });
                let _g4 = self.depth_push();
                let continue_depth = _g4.saved();

                self.loop_stack.push(super::LoopLabels { break_depth, continue_depth });

                // if !cond, break — try to invert condition to avoid i32_eqz
                if !self.try_emit_inverted_br_if(cond, self.depth - break_depth - 1) {
                    self.emit_expr(cond);
                    wasm!(self.func, {
                        i32_eqz;
                        br_if(self.depth - break_depth - 1);
                    });
                }

                // Save heap at iteration start
                if let Some(sl) = iter_scope_local {
                    wasm!(self.func, { global_get(self.emitter.heap_ptr_global); local_set(sl); });
                }

                // body
                for stmt in body {
                    self.emit_stmt(stmt);
                }

                // Restore heap at iteration end. The free list is reset
                // with it: every node on it points into the region being
                // rolled back (nothing escaped — the iter_scope precondition),
                // so forgetting them wholesale is the only sound option.
                if let Some(sl) = iter_scope_local {
                    wasm!(self.func, {
                        local_get(sl); global_set(self.emitter.heap_ptr_global);
                        i32_const(0); global_set(self.emitter.free_list_global);
                    });
                }

                // continue (jump to loop start)
                wasm!(self.func, { br(self.depth - continue_depth - 1); });

                self.loop_stack.pop();
                self.depth_pop(_g4);
                wasm!(self.func, { end; }); // end loop
                self.depth_pop(_g3);
                wasm!(self.func, { end; }); // end block
                if let Some(sl) = iter_scope_local {
                    self.scratch.free_i32(sl);
                }
            }

            // ── For-in loop ──
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.emit_for_in(*var, var_tuple.as_deref(), iterable, body);
            }

            IrExprKind::Break => {
                if let Some(labels) = self.loop_stack.last() {
                    let relative = self.depth - labels.break_depth - 1;
                    wasm!(self.func, { br(relative); });
                }
            }

            IrExprKind::Continue => {
                if let Some(labels) = self.loop_stack.last() {
                    let relative = self.depth - labels.continue_depth - 1;
                    wasm!(self.func, { br(relative); });
                }
            }

            // ── Function calls ──
            IrExprKind::Call { target, args, .. } => {
                self.emit_call(target, args, &expr.ty);
            }
            IrExprKind::TailCall { target, args } => {
                self.emit_tail_call(target, args, &expr.ty);
            }

            // ── String interpolation ──
            IrExprKind::StringInterp { parts } => {
                self.emit_string_interp(parts);
            }

            // ── Match ──
            IrExprKind::Match { subject, arms } => {
                self.emit_match(subject, arms, &expr.ty);
            }

            // ── Record/Variant construction ──
            IrExprKind::Record { name, fields, .. } => {
                self.emit_record(name.as_deref(), fields, &expr.ty);
            }

            // ── Spread record ──
            IrExprKind::SpreadRecord { base, fields } => {
                self.emit_spread_record(base, fields, &expr.ty);
            }

            // ── Tuple construction ──
            IrExprKind::Tuple { elements } => {
                self.emit_tuple(elements);
            }

            // ── Field access ──
            IrExprKind::Member { object, field } => {
                self.emit_member(object, field);
            }

            // ── Tuple index access ──
            IrExprKind::TupleIndex { object, index } => {
                self.emit_tuple_index(object, *index, &expr.ty);
            }

            // ── List construction ──
            IrExprKind::List { elements } => {
                self.emit_list(elements, &expr.ty);
            }

            // ── Index access (list[i]) ──
            IrExprKind::IndexAccess { object, index } => {
                self.emit_index_access(object, index, &expr.ty);
            }

            // ── Codegen-specific nodes (pass-through or ignore) ──
            IrExprKind::Clone { expr: inner } | IrExprKind::Deref { expr: inner }
            | IrExprKind::ToVec { expr: inner } => {
                self.emit_expr(inner);
            }

            // ── Unsupported ──
            other => {
                panic!(
                    "[ICE] emit_wasm: no emission for IrExprKind::{:?} — \
                     add an arm or lower it in a pass (completeness §5: a \
                     silent runtime trap is never an acceptable fallback)",
                    std::mem::discriminant(other)
                );
            }
        }
    }

}

// Method groups + helpers split out to keep every file < 1000 lines.
// These are `include!`d (not `mod`) so they extend the SAME `expressions`
// module and the SAME `impl FuncCompiler<'_>` — they share this file's
// imports and the `CmpKind` enum. emit_expr chains into emit_expr_g2/g3.
include!("expressions_g2.rs");
include!("expressions_g3.rs");
include!("expressions_p2.rs");
include!("expressions_p3.rs");
