//! IrExpr → WASM instruction emission.

use almide_ir::{BinOp, IrExpr, IrExprKind, UnOp};
use almide_lang::types::Ty;
use wasm_encoder::{Instruction, ValType};

use super::FuncCompiler;
use super::values;
use super::wasm_macro::wasm;

#[derive(Clone, Copy)]
pub(super) enum CmpKind {
    Lt,
    Gt,
    Lte,
    Gte,
}

impl FuncCompiler<'_> {
    /// Emit WASM instructions for an IR expression.
    /// Leaves the result value on the WASM stack (nothing for Unit).
    pub fn emit_expr(&mut self, expr: &IrExpr) {
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
                    // Try name-based lookup FIRST: cross-module synthetic Vars
                    // (ALMIDE_RT_<MOD>_<NAME>) must resolve by name because their
                    // VarIds can collide with unrelated globals after unification.
                    let name = if (id.0 as usize) < self.var_table.len() { self.var_table.get(*id).name.as_str() } else { "" };
                    self.emitter.top_let_globals_by_name.get(name)
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
                        self.var_map.iter()
                            .filter(|(vid, _)| (**vid as usize) < self.var_table.len() && self.var_table.get(almide_ir::VarId(**vid)).name == name)
                            .max_by_key(|(vid, _)| {
                                let vid_vt = values::ty_to_valtype(&self.var_table.get(almide_ir::VarId(**vid)).ty);
                                if vid_vt == target_vt { 1u8 } else { 0u8 }
                            })
                            .map(|(_, lidx)| *lidx)
                    } else { None };
                    if let Some(local_idx) = found {
                        wasm!(self.func, { local_get(local_idx); });
                    } else {
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
                // Perceus last-use drop is now handled by PerceusPass (IR-level RcDec nodes)
                for stmt in stmts {
                    self.emit_stmt(stmt);
                }
                if let Some(e) = tail {
                    self.emit_expr(e);
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

                // Restore heap at iteration end
                if let Some(sl) = iter_scope_local {
                    wasm!(self.func, { local_get(sl); global_set(self.emitter.heap_ptr_global); });
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
            IrExprKind::RuntimeCall { symbol, args } => {
                // Resolved runtime call from @intrinsic. Preferred path:
                // look up the mangled symbol in `func_map` and emit
                // `call(idx)` after each arg. Fallback: the WASM runtime
                // fn may not be registered yet (migration in progress).
                // Decode the symbol back to (module, func) and route
                // through the legacy `emit_<m>_call` dispatcher so the
                // inline-emitted variant (`int.abs` as i64 ops, etc.)
                // keeps working until the runtime fn lands.
                let sym = symbol.as_str();
                // mem.save / mem.restore: direct runtime calls
                if sym == "almide_rt_mem_save" {
                    wasm!(self.func, { call(self.emitter.rt.heap_save); i64_extend_i32_u; });
                } else if sym == "almide_rt_mem_restore" {
                    self.emit_expr(&args[0]);
                    wasm!(self.func, { i32_wrap_i64; call(self.emitter.rt.heap_restore); });
                } else if let Some(&idx) = self.emitter.func_map.get(sym) {
                    for a in args { self.emit_expr(a); }
                    wasm!(self.func, { call(idx); });
                } else if let Some((module, func)) = self.emitter.intrinsic_symbol_to_fn.get(sym).cloned() {
                    // Preferred: use the Almide (module, fn) that declared
                    // the `@intrinsic` — the symbol may rename the fn
                    // (e.g. `map.map` → `almide_rt_map_map_values`).
                    if !self.dispatch_runtime_fallback(&module, &func, args, &expr.ty) {
                        panic!(
                            "[ICE] emit_wasm: RuntimeCall `{}` declared by `{}.{}` \
                             — no WASM runtime fn and no legacy dispatcher arm. \
                             Register the runtime fn or add a dispatch arm.",
                            sym, module, func
                        );
                    }
                } else if let Some(rest) = sym.strip_prefix("almide_rt_") {
                    // Legacy fallback: decode module/fn from the mangled
                    // symbol name. Used when the runtime symbol matches the
                    // Almide fn name 1:1 and the bundled `@intrinsic` map
                    // hasn't claimed it.
                    if let Some(underscore) = rest.find('_') {
                        let module = &rest[..underscore];
                        let func = &rest[underscore + 1..];
                        if !self.dispatch_runtime_fallback(module, func, args, &expr.ty) {
                            panic!(
                                "[ICE] emit_wasm: RuntimeCall `{}` — no WASM \
                                 runtime fn and no legacy dispatcher fallback. \
                                 Register the runtime fn or add a dispatch arm \
                                 for `{}.{}`.",
                                sym, module, func
                            );
                        }
                    } else {
                        panic!(
                            "[ICE] emit_wasm: RuntimeCall symbol `{}` has no \
                             recoverable (module, func) prefix for fallback dispatch.",
                            sym
                        );
                    }
                } else {
                    panic!(
                        "[ICE] emit_wasm: RuntimeCall symbol `{}` lacks the \
                         `almide_rt_` prefix — cannot look up in func_map or \
                         derive fallback dispatch.",
                        sym
                    );
                }
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

            // ── Map ──
            IrExprKind::EmptyMap => {
                // Empty hash map: [len=0][cap=0]
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(super::list_layout::MAP_HEADER_SIZE);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    local_get(scratch); i32_const(0); i32_store(0); // len = 0
                    local_get(scratch); i32_const(0); i32_store(super::list_layout::MAP_CAP_OFFSET as u32); // cap = 0
                    local_get(scratch);
                });
                self.scratch.free_i32(scratch);
            }
            IrExprKind::MapLiteral { entries } => {
                // Map literal: build hash table from entries.
                // Allocate hash table with capacity = next power of 2 >= n * 2 (min 16).
                let n = entries.len() as u32;
                if n == 0 {
                    // Empty map
                    let scratch = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        i32_const(super::list_layout::MAP_HEADER_SIZE);
                        call(self.emitter.rt.alloc);
                        local_set(scratch);
                        local_get(scratch); i32_const(0); i32_store(0);
                        local_get(scratch); i32_const(0); i32_store(super::list_layout::MAP_CAP_OFFSET as u32);
                        local_get(scratch);
                    });
                    self.scratch.free_i32(scratch);
                } else {
                    let ks = if let Some((k, _)) = entries.first() { values::byte_size(&k.ty) } else { 4 };
                    let vs = if let Some((_, v)) = entries.first() { values::byte_size(&v.ty) } else { 4 };
                    let entry_size = ks + vs; // no tag per entry
                    let key_ty = if let Some((k, _)) = entries.first() { k.ty.clone() } else { Ty::String };
                    let mut cap = 16u32;
                    while cap < n * 2 { cap *= 2; }
                    // Swiss Table: [header][tags: cap bytes][entries: cap * entry_size]
                    let total = super::list_layout::MAP_HEADER_SIZE as u32 + cap + cap * entry_size;

                    let map = self.scratch.alloc_i32();
                    let idx = self.scratch.alloc_i32();
                    let eb = self.scratch.alloc_i32(); // entries_base
                    let h2_lit = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        i32_const(total as i32);
                        call(self.emitter.rt.alloc);
                        local_set(map);
                        local_get(map); i32_const(n as i32); i32_store(0);
                        local_get(map); i32_const(cap as i32); i32_store(super::list_layout::MAP_CAP_OFFSET as u32);
                        // entries_base = map + 8 + cap
                        local_get(map); i32_const(super::list_layout::MAP_TAGS_OFFSET); i32_add;
                        i32_const(cap as i32); i32_add; local_set(eb);
                    });

                    for (key, val) in entries {
                        self.emit_expr(key);
                        let sk = if matches!(key_ty, Ty::Int) {
                            let sk = self.scratch.alloc_i64();
                            wasm!(self.func, { local_tee(sk); });
                            sk
                        } else {
                            let sk = self.scratch.alloc_i32();
                            wasm!(self.func, { local_tee(sk); });
                            sk
                        };
                        self.emit_hash_key(&key_ty);
                        // Split into h1 + h2
                        let ht = self.scratch.alloc_i32();
                        wasm!(self.func, {
                            local_tee(ht);
                            i32_const(cap as i32 - 1); i32_and; local_set(idx);
                            local_get(ht);
                            i32_const(25); i32_shr_u; i32_const(0x7F); i32_and;
                            local_tee(h2_lit);
                            i32_eqz;
                            if_empty; i32_const(1); local_set(h2_lit); end;
                        });
                        self.scratch.free_i32(ht);
                        // Probe for empty tag
                        wasm!(self.func, {
                            block_empty; loop_empty;
                              local_get(map); i32_const(super::list_layout::MAP_TAGS_OFFSET); i32_add;
                              local_get(idx); i32_add; i32_load8_u(0);
                              i32_eqz; br_if(1);
                              local_get(idx); i32_const(1); i32_add;
                              i32_const(cap as i32 - 1); i32_and;
                              local_set(idx); br(0);
                            end; end;
                            // Store tag (1 byte)
                            local_get(map); i32_const(super::list_layout::MAP_TAGS_OFFSET); i32_add;
                            local_get(idx); i32_add; local_get(h2_lit); i32_store8(0);
                            // Store key at entries_base + idx * entry_size
                            local_get(eb); local_get(idx); i32_const(entry_size as i32); i32_mul; i32_add;
                        });
                        if matches!(key_ty, Ty::Int) {
                            wasm!(self.func, { local_get(sk); i64_store(0); });
                            self.scratch.free_i64(sk);
                        } else {
                            wasm!(self.func, { local_get(sk); i32_store(0); });
                            self.scratch.free_i32(sk);
                        }
                        // Store value at entries_base + idx * entry_size + ks
                        wasm!(self.func, {
                            local_get(eb); local_get(idx); i32_const(entry_size as i32); i32_mul; i32_add;
                            i32_const(ks as i32); i32_add;
                        });
                        self.emit_expr(val);
                        self.emit_store_at(&val.ty, 0);
                    }
                    wasm!(self.func, { local_get(map); });
                    self.scratch.free_i32(h2_lit);
                    self.scratch.free_i32(eb);
                    self.scratch.free_i32(idx);
                    self.scratch.free_i32(map);
                }
            }

            // ── Option/Result ──
            IrExprKind::OptionSome { expr: inner } => {
                // Resolve inner type: if Unknown, infer from outer Option type or inner expr
                let inner_ty = if matches!(inner.ty, Ty::Unknown) {
                    if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Option, args) = &expr.ty {
                        let candidate = args.first().cloned().unwrap_or(Ty::Unknown);
                        if !matches!(candidate, Ty::Unknown) { candidate }
                        else { self.infer_type_from_expr(inner) }
                    } else { self.infer_type_from_expr(inner) }
                } else { inner.ty.clone() };
                let inner_size = values::byte_size(&inner_ty);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(inner_size as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    local_get(scratch);
                });
                self.emit_expr(inner);
                self.emit_store_at(&inner_ty, 0);
                wasm!(self.func, { local_get(scratch); });
                self.scratch.free_i32(scratch);
            }
            IrExprKind::OptionNone => {
                wasm!(self.func, { i32_const(0); });
            }

            // ── Result ok/err ──
            IrExprKind::ResultOk { expr: inner } => {
                // ok(x) = [tag:0, value]
                // Resolve inner type: if Unknown, try to infer from the outer Result type or expr
                let inner_ty = if matches!(inner.ty, Ty::Unknown) {
                    self.resolve_result_inner_ty(expr, true)
                } else { inner.ty.clone() };
                let inner_size = values::byte_size(&inner_ty);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const((4 + inner_size) as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    // tag = 0
                    local_get(scratch);
                    i32_const(0);
                    i32_store(0);
                });
                if values::ty_to_valtype(&inner_ty).is_some() {
                    wasm!(self.func, { local_get(scratch); });
                    self.emit_expr(inner);
                    self.emit_store_at(&inner_ty, 4);
                } else {
                    // Unit or zero-sized: still emit for side effects
                    self.emit_expr(inner);
                }
                wasm!(self.func, { local_get(scratch); });
                self.scratch.free_i32(scratch);
            }
            IrExprKind::ResultErr { expr: inner } => {
                // err(e) = [tag:1, value]
                let inner_ty = if matches!(inner.ty, Ty::Unknown) {
                    self.resolve_result_inner_ty(expr, false)
                } else { inner.ty.clone() };
                let inner_size = values::byte_size(&inner_ty);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const((4 + inner_size) as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    // tag = 1
                    local_get(scratch);
                    i32_const(1);
                    i32_store(0);
                });
                if values::ty_to_valtype(&inner_ty).is_some() {
                    wasm!(self.func, { local_get(scratch); });
                    self.emit_expr(inner);
                    self.emit_store_at(&inner_ty, 4);
                } else {
                    // Unit or zero-sized: still emit for side effects
                    self.emit_expr(inner);
                }
                wasm!(self.func, { local_get(scratch); });
                self.scratch.free_i32(scratch);
            }

            // ── Fan block (sequential fallback — no parallelism in WASM) ──
            IrExprKind::Fan { exprs } => {
                if exprs.len() == 1 {
                    // Single expr: emit with auto-unwrap if Result
                    self.emit_expr(&exprs[0]);
                    if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _) = &exprs[0].ty {
                        let scratch = self.scratch.alloc_i32();
                        wasm!(self.func, {
                            local_set(scratch);
                            local_get(scratch); i32_load(0); i32_const(0); i32_ne;
                            if_empty; local_get(scratch); return_; end;
                            local_get(scratch);
                        });
                        self.emit_load_at(&expr.ty, 4);
                        self.scratch.free_i32(scratch);
                    }
                } else {
                    // Fan with multiple exprs → Tuple of unwrapped results
                    // Each expr returns Result[T, E]. Unwrap each, build tuple of T values.
                    let elem_types: Vec<Ty> = if let Ty::Tuple(tys) = &expr.ty {
                        tys.clone()
                    } else {
                        exprs.iter().map(|e| e.ty.clone()).collect()
                    };
                    let total_size: u32 = elem_types.iter().map(|t| values::byte_size(t)).sum();
                    let tuple_scratch = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        i32_const(total_size as i32);
                        call(self.emitter.rt.alloc);
                        local_set(tuple_scratch);
                    });
                    let mut offset = 0u32;
                    for (i, e) in exprs.iter().enumerate() {
                        let elem_ty = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                        let elem_size = values::byte_size(&elem_ty);
                        // Fan exprs are typically effect fn calls → Result[T, E]
                        // Auto-unwrap: if err, return Result early; if ok, store unwrapped value
                        let is_result = matches!(&e.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _));
                        if is_result {
                            self.emit_expr(e);
                            let res_scratch = self.scratch.alloc_i32();
                            wasm!(self.func, {
                                local_set(res_scratch);
                                local_get(res_scratch); i32_load(0); i32_const(0); i32_ne;
                                if_empty; local_get(res_scratch); return_; end;
                                local_get(tuple_scratch);
                                local_get(res_scratch);
                            });
                            self.emit_load_at(&elem_ty, 4);
                            self.emit_store_at(&elem_ty, offset);
                            self.scratch.free_i32(res_scratch);
                        } else {
                            // Non-Result: push tuple_ptr, emit expr, store
                            wasm!(self.func, { local_get(tuple_scratch); });
                            self.emit_expr(e);
                            self.emit_store_at(&elem_ty, offset);
                        }
                        offset += elem_size;
                    }
                    wasm!(self.func, { local_get(tuple_scratch); });
                    self.scratch.free_i32(tuple_scratch);
                }
            }

            // ── Try (auto-unwrap Result in effect fn) ──
            IrExprKind::Try { expr: inner } => {
                // Evaluate inner (returns Result ptr: [tag:i32][value])
                // If tag == 0 (ok): unwrap → push value
                // If tag != 0 (err): return the Result as-is
                self.emit_expr(inner);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(scratch);
                    // Check tag
                    local_get(scratch);
                    i32_load(0);
                    i32_const(0);
                    i32_ne;
                    if_empty;
                    // Err: return the Result ptr
                    local_get(scratch);
                    return_;
                    end;
                });
                // Ok: load the unwrapped value (skip for Unit — nothing to load)
                if !matches!(&expr.ty, Ty::Unit) {
                    wasm!(self.func, { local_get(scratch); });
                    self.emit_load_at(&expr.ty, 4);
                }
                self.scratch.free_i32(scratch);
            }

            // ── Unwrap (propagate err on failure) ──
            IrExprKind::Unwrap { expr: inner } => {
                let is_option = inner.ty.is_option();
                self.emit_expr(inner);
                let scratch = self.scratch.alloc_i32();
                if is_option {
                    // Option: ptr==0 → None (propagate as err), ptr!=0 → Some (payload at ptr)
                    wasm!(self.func, {
                        local_set(scratch);
                        local_get(scratch);
                        i32_eqz;
                        if_empty;
                        // None → build an err Result and return it
                        i32_const(0);
                        return_;
                        end;
                    });
                    // Some: load payload from ptr
                    if !matches!(&expr.ty, Ty::Unit) {
                        wasm!(self.func, { local_get(scratch); });
                        self.emit_load_at(&expr.ty, 0);
                    }
                } else {
                    // Result: [tag:i32, payload]. tag==0 → Ok, tag!=0 → Err
                    wasm!(self.func, {
                        local_set(scratch);
                        local_get(scratch);
                        i32_load(0);
                        i32_const(0);
                        i32_ne;
                        if_empty;
                        // Err path: propagate the Result pointer (early return)
                        local_get(scratch);
                        return_;
                        end;
                    });
                    if !matches!(&expr.ty, Ty::Unit) {
                        wasm!(self.func, { local_get(scratch); });
                        self.emit_load_at(&expr.ty, 4);
                    }
                }
                self.scratch.free_i32(scratch);
            }

            // ── UnwrapOr (fallback on err/none) ──
            IrExprKind::UnwrapOr { expr: inner, fallback } => {
                let is_option = inner.ty.is_option();
                self.emit_expr(inner);
                let scratch = self.scratch.alloc_i32();
                let bt = values::block_type(&expr.ty);
                if is_option {
                    // Option: ptr==0 → fallback, ptr!=0 → load payload from ptr
                    wasm!(self.func, {
                        local_set(scratch);
                        local_get(scratch);
                        i32_eqz;
                    });
                    self.func.instruction(&Instruction::If(bt));
                    self.emit_expr(fallback);
                    wasm!(self.func, { else_; local_get(scratch); });
                    self.emit_load_at(&expr.ty, 0);
                    wasm!(self.func, { end; });
                } else {
                    // Result: tag==0 → ok (load payload at +4), tag!=0 → fallback
                    wasm!(self.func, {
                        local_set(scratch);
                        local_get(scratch);
                        i32_load(0);
                        i32_eqz;
                    });
                    self.func.instruction(&Instruction::If(bt));
                    wasm!(self.func, { local_get(scratch); });
                    self.emit_load_at(&expr.ty, 4);
                    wasm!(self.func, { else_; });
                    self.emit_expr(fallback);
                    wasm!(self.func, { end; });
                }
                self.scratch.free_i32(scratch);
            }

            // ── ToOption (Result → Option, Option passthrough) ──
            IrExprKind::ToOption { expr: inner } => {
                if inner.ty.is_option() {
                    // Option → Option: identity
                    self.emit_expr(inner);
                } else {
                    // Result → Option: ok(v) → some(v), err(_) → none (ptr=0)
                    self.emit_expr(inner);
                    let scratch = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        local_set(scratch);
                        local_get(scratch);
                        i32_load(0);
                        i32_eqz;
                        if_i32;
                    });
                    // Ok: allocate Some ptr and copy payload
                    let inner_ty = inner.ty.result_ok_ty().unwrap_or(Ty::Unknown);
                    let inner_size = values::byte_size(&inner_ty);
                    if inner_size > 0 {
                        wasm!(self.func, {
                            i32_const(inner_size as i32);
                            call(self.emitter.rt.alloc);
                        });
                        let some_scratch = self.scratch.alloc_i32();
                        wasm!(self.func, { local_tee(some_scratch); });
                        wasm!(self.func, { local_get(scratch); });
                        self.emit_load_at(&inner_ty, 4);
                        self.emit_store_at(&inner_ty, 0);
                        wasm!(self.func, { local_get(some_scratch); });
                        self.scratch.free_i32(some_scratch);
                    } else {
                        // Unit payload — Some is just a non-zero ptr
                        wasm!(self.func, {
                            i32_const(1);
                        });
                    }
                    wasm!(self.func, {
                        else_;
                        // Err: return 0 (None)
                        i32_const(0);
                        end;
                    });
                    self.scratch.free_i32(scratch);
                }
            }

            // ── Map index access: m[key] → Option[V] ──
            IrExprKind::MapAccess { object, key } => {
                let fake_args = vec![(**object).clone(), (**key).clone()];
                self.emit_map_call("get", &fake_args);
            }

            // ── Optional chaining: expr?.field → None if expr is None, else Some(expr.field) ──
            IrExprKind::OptionalChain { expr: inner, field } => {
                // inner is Option<RecordType> — ptr: 0=None, nonzero=Some wrapper
                // Some wrapper layout: [payload_ptr:i32] where payload_ptr → record
                self.emit_expr(inner);
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(scratch);
                    local_get(scratch);
                    i32_eqz;
                    if_i32;
                    // None path → propagate None (ptr=0)
                    i32_const(0);
                    else_;
                });
                // Some path: dereference Some wrapper to get the actual record pointer
                let payload_ty = inner.ty.option_inner().unwrap_or_else(|| inner.ty.clone());
                let payload_size = values::byte_size(&payload_ty);
                if payload_size > 0 {
                    wasm!(self.func, { local_get(scratch); i32_load(0); local_set(scratch); });
                }
                let fields = self.extract_record_fields(&payload_ty);
                let tag_offset = self.variant_tag_offset(&payload_ty);
                if let Some((field_offset, field_ty)) = values::field_offset(&fields, field) {
                    let total_offset = tag_offset + field_offset;
                    let field_size = values::byte_size(&field_ty);
                    if field_size > 0 {
                        // Allocate Some wrapper for the field value
                        wasm!(self.func, { i32_const(field_size as i32); call(self.emitter.rt.alloc); });
                        let some_ptr = self.scratch.alloc_i32();
                        wasm!(self.func, { local_tee(some_ptr); local_get(scratch); });
                        self.emit_load_at(&field_ty, total_offset);
                        self.emit_store_at(&field_ty, 0);
                        wasm!(self.func, { local_get(some_ptr); });
                        self.scratch.free_i32(some_ptr);
                    } else {
                        // Unit field → Some is just a non-zero ptr
                        wasm!(self.func, { i32_const(1); });
                    }
                } else {
                    wasm!(self.func, { unreachable; });
                }
                wasm!(self.func, { end; });
                self.scratch.free_i32(scratch);
            }

            // ── Range → materialize as List[Int] ──
            // In Almide, 0..n has type List[Int]. For-in optimizes this to a loop counter,
            // but anywhere else a Range appears as a value, it must produce a list pointer.
            IrExprKind::Range { start, end, inclusive } => {
                let s = self.scratch.alloc_i64();
                let e = self.scratch.alloc_i64();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(start);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(end);
                wasm!(self.func, { local_set(e); });
                // len = max(0, end - start [+ 1 if inclusive])
                wasm!(self.func, {
                    local_get(e); local_get(s); i64_sub;
                });
                if *inclusive {
                    wasm!(self.func, { i64_const(1); i64_add; });
                }
                wasm!(self.func, {
                    i64_const(0); i64_gt_s;
                    if_i32;
                      local_get(e); local_get(s); i64_sub;
                });
                if *inclusive {
                    wasm!(self.func, { i64_const(1); i64_add; });
                }
                wasm!(self.func, {
                      i32_wrap_i64;
                    else_;
                      i32_const(0);
                    end;
                    local_set(len);
                    // alloc: 8 + len * 8 (header: [len:i32][cap:i32])
                    i32_const(8); local_get(len); i32_const(8); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    local_get(dst); local_get(len); i32_store(4); // cap = len
                    // fill elements
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(super::list_layout::DATA_OFFSET); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      // value = start + i
                      local_get(s); local_get(i); i64_extend_i32_u; i64_add;
                      i64_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i64(e);
                self.scratch.free_i64(s);
            }

            // ── Codegen-specific nodes (pass-through or ignore) ──
            IrExprKind::Clone { expr: inner } | IrExprKind::Deref { expr: inner }
            | IrExprKind::ToVec { expr: inner } => {
                self.emit_expr(inner);
            }

            // ── Unsupported ──
            _ => {
                wasm!(self.func, { unreachable; });
            }
        }
    }

    pub(super) fn emit_binop(&mut self, op: BinOp, left: &IrExpr, right: &IrExpr) {
        // BinOp is already reconciled with operand types by ConcretizeTypes pass.
        // Pick WASM arithmetic width from the operand's valtype. All
        // sized integer variants (Int8/Int16/Int32/UInt8/UInt16/UInt32)
        // lower to `i32`; `UInt64` and canonical `Int` stay `i64`. For
        // unsigned div/mod the distinction matters (div_u vs div_s),
        // tracked via `is_unsigned_int`.
        let is_i32_int = matches!(
            left.ty,
            Ty::Int8 | Ty::Int16 | Ty::Int32
                | Ty::UInt8 | Ty::UInt16 | Ty::UInt32
        );
        let is_unsigned_int = matches!(
            left.ty,
            Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
        );
        let is_f32 = matches!(left.ty, Ty::Float32);

        match op {
            // ── Arithmetic ──
            BinOp::AddInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_i32_int {
                    wasm!(self.func, { i32_add; });
                } else {
                    wasm!(self.func, { i64_add; });
                }
            }
            BinOp::SubInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_i32_int {
                    wasm!(self.func, { i32_sub; });
                } else {
                    wasm!(self.func, { i64_sub; });
                }
            }
            BinOp::MulInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_i32_int {
                    wasm!(self.func, { i32_mul; });
                } else {
                    wasm!(self.func, { i64_mul; });
                }
            }
            BinOp::DivInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                let instr = match (is_i32_int, is_unsigned_int) {
                    (true, true) => wasm_encoder::Instruction::I32DivU,
                    (true, false) => wasm_encoder::Instruction::I32DivS,
                    (false, true) => wasm_encoder::Instruction::I64DivU,
                    (false, false) => wasm_encoder::Instruction::I64DivS,
                };
                self.func.instruction(&instr);
            }
            BinOp::ModInt => {
                self.emit_expr(left);
                self.emit_expr(right);
                let instr = match (is_i32_int, is_unsigned_int) {
                    (true, true) => wasm_encoder::Instruction::I32RemU,
                    (true, false) => wasm_encoder::Instruction::I32RemS,
                    (false, true) => wasm_encoder::Instruction::I64RemU,
                    (false, false) => wasm_encoder::Instruction::I64RemS,
                };
                self.func.instruction(&instr);
            }
            BinOp::AddFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_f32 {
                    self.func.instruction(&wasm_encoder::Instruction::F32Add);
                } else {
                    wasm!(self.func, { f64_add; });
                }
            }
            BinOp::SubFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_f32 {
                    self.func.instruction(&wasm_encoder::Instruction::F32Sub);
                } else {
                    wasm!(self.func, { f64_sub; });
                }
            }
            BinOp::MulFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_f32 {
                    self.func.instruction(&wasm_encoder::Instruction::F32Mul);
                } else {
                    wasm!(self.func, { f64_mul; });
                }
            }
            BinOp::DivFloat => {
                self.emit_expr(left);
                self.emit_expr(right);
                if is_f32 {
                    self.func.instruction(&wasm_encoder::Instruction::F32Div);
                } else {
                    wasm!(self.func, { f64_div; });
                }
            }
            BinOp::ModFloat => {
                // WASM has no f64.rem; compute via: a - trunc(a/b) * b
                self.emit_expr(left);
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { f64_div; });
                self.func.instruction(&Instruction::F64Trunc);
                self.emit_expr(right);
                wasm!(self.func, {
                    f64_mul;
                    f64_sub;
                });
            }

            // ── Comparison (type-dispatched via operand type) ──
            BinOp::Eq => {
                // Peephole: x % (power-of-2) == 0 → (x & (n-1)) == 0
                // Safe because for any sign of x: x%n==0 ⟺ x&(n-1)==0
                let modint_zero = Self::extract_mod_pow2_eq_zero(left, right)
                    .or_else(|| Self::extract_mod_pow2_eq_zero(right, left));
                if let Some((mod_expr, mask)) = modint_zero {
                    self.emit_expr(mod_expr);
                    wasm!(self.func, { i64_const(mask); i64_and; i64_eqz; });
                } else {
                    self.emit_eq(left, right, false);
                }
            }
            BinOp::Neq => {
                // Peephole: x % (power-of-2) != 0 → (x & (n-1)) != 0
                let modint_zero = Self::extract_mod_pow2_eq_zero(left, right)
                    .or_else(|| Self::extract_mod_pow2_eq_zero(right, left));
                if let Some((mod_expr, mask)) = modint_zero {
                    self.emit_expr(mod_expr);
                    wasm!(self.func, { i64_const(mask); i64_and; i64_const(0); i64_ne; });
                } else {
                    self.emit_eq(left, right, true);
                }
            }
            BinOp::Lt => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Lt);
            }
            BinOp::Gt => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Gt);
            }
            BinOp::Lte => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Lte);
            }
            BinOp::Gte => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Gte);
            }

            // ── Logical ──
            BinOp::And => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { i32_and; });
            }
            BinOp::Or => {
                self.emit_expr(left);
                self.emit_expr(right);
                wasm!(self.func, { i32_or; });
            }

            // ── String concatenation ──
            BinOp::ConcatStr => {
                self.emit_concat_str(left, right);
            }

            // ── List concatenation ──
            BinOp::ConcatList => {
                self.emit_expr(left);
                self.emit_expr(right);
                // Determine element size from left/right types or VarTable
                let extract_elem = |ty: &Ty| -> Option<u32> {
                    if let Ty::Applied(_, args) = ty {
                        args.first()
                            .filter(|t| !t.is_unresolved())
                            .map(|t| values::byte_size(t))
                    } else { None }
                };
                let var_elem = |expr: &IrExpr| -> Option<u32> {
                    if let almide_ir::IrExprKind::Var { id } = &expr.kind {
                        extract_elem(&self.var_table.get(*id).ty)
                    } else { None }
                };
                let elem_size = extract_elem(&left.ty)
                    .or_else(|| extract_elem(&right.ty))
                    .or_else(|| var_elem(left))
                    .or_else(|| var_elem(right))
                    .unwrap_or(8);
                wasm!(self.func, {
                    i32_const(elem_size as i32);
                    call(self.emitter.rt.concat_list);
                });
            }

            // ── Matrix operations (WASM stub — not yet optimized) ──
            BinOp::MulMatrix | BinOp::AddMatrix | BinOp::SubMatrix | BinOp::ScaleMatrix => {
                // Matrix ops in WASM: call the corresponding stdlib function via module dispatch
                let func_name = match op {
                    BinOp::MulMatrix => "mul",
                    BinOp::AddMatrix => "add",
                    BinOp::SubMatrix => "sub",
                    BinOp::ScaleMatrix => "scale",
                    _ => unreachable!(),
                };
                let target = almide_ir::CallTarget::Module {
                    module: almide_base::intern::sym("matrix"),
                    func: almide_base::intern::sym(func_name),
                    def_id: None,
                };
                self.emit_call(&target, &[left.clone(), right.clone()], &Ty::Matrix);
            }

            BinOp::PowInt => {
                // Integer power: base^exp via mem scratch (no locals needed)
                // mem[0]=base, mem[8]=result, counter on stack via block/loop
                self.emit_expr(left);
                self.emit_expr(right);
                // Use i32 scratch for counter, i64 scratch for result/base
                let base_s = self.scratch.alloc_i64();
                let result_s = self.scratch.alloc_i64();
                let counter_s = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_wrap_i64;
                    local_set(counter_s);
                    local_set(base_s);
                    i64_const(1);
                    local_set(result_s);
                    block_empty;
                    loop_empty;
                    local_get(counter_s);
                    i32_eqz;
                    br_if(1);
                    local_get(result_s);
                    local_get(base_s);
                    i64_mul;
                    local_set(result_s);
                    local_get(counter_s);
                    i32_const(1);
                    i32_sub;
                    local_set(counter_s);
                    br(0);
                    end;
                    end;
                    local_get(result_s);
                });
                self.scratch.free_i32(counter_s);
                self.scratch.free_i64(result_s);
                self.scratch.free_i64(base_s);
            }
            BinOp::PowFloat => {
                // Float power: check if exp == 0.5 → sqrt, else integer loop
                let base_s = self.scratch.alloc_i64();
                let result_s = self.scratch.alloc_i64();
                let counter_s = self.scratch.alloc_i32();
                self.emit_expr(left);
                wasm!(self.func, { i64_reinterpret_f64; local_set(base_s); });
                self.emit_expr(right);
                // Check if exp == 0.5
                wasm!(self.func, {
                    f64_const(0.5);
                    f64_eq;
                    if_f64;
                    local_get(base_s);
                    f64_reinterpret_i64;
                    f64_sqrt;
                    else_;
                });
                // Integer loop for non-0.5 exponent
                self.emit_expr(right);
                wasm!(self.func, {
                    i64_trunc_f64_s;
                    i32_wrap_i64;
                    local_set(counter_s);
                    f64_const(1.0);
                    i64_reinterpret_f64;
                    local_set(result_s);
                    block_empty;
                    loop_empty;
                    local_get(counter_s);
                    i32_eqz;
                    br_if(1);
                    local_get(result_s);
                    f64_reinterpret_i64;
                    local_get(base_s);
                    f64_reinterpret_i64;
                    f64_mul;
                    i64_reinterpret_f64;
                    local_set(result_s);
                    local_get(counter_s);
                    i32_const(1);
                    i32_sub;
                    local_set(counter_s);
                    br(0);
                    end;
                    end;
                    local_get(result_s);
                    f64_reinterpret_i64;
                    end;
                });
                self.scratch.free_i32(counter_s);
                self.scratch.free_i64(result_s);
                self.scratch.free_i64(base_s);
            }
        }
    }

}

/// Collect type parameter names from a type (Named("X", []) where X is a single-letter or TypeVar).
pub(super) fn collect_type_param_names<'a>(ty: &'a Ty, names: &mut Vec<&'a str>) {
    match ty {
        Ty::Named(name, args) if args.is_empty() && name.len() <= 2 && name.chars().next().map_or(false, |c| c.is_uppercase()) => {
            if !names.contains(&name.as_str()) {
                names.push(name.as_str());
            }
        }
        Ty::TypeVar(name) => {
            if !names.contains(&name.as_str()) {
                names.push(name.as_str());
            }
        }
        Ty::Applied(_, args) => { for a in args { collect_type_param_names(a, names); } }
        Ty::Tuple(elems) => { for e in elems { collect_type_param_names(e, names); } }
        Ty::Fn { params, ret } => {
            for p in params { collect_type_param_names(p, names); }
            collect_type_param_names(ret, names);
        }
        _ => {}
    }
}

/// Substitute type parameters in a type. Named("T", []) → type_args[index of "T"].
pub(super) fn substitute_type_params(ty: &Ty, generic_names: &[&str], type_args: &[Ty]) -> Ty {
    match ty {
        Ty::Named(name, args) if args.is_empty() => {
            // Check if this is a type parameter name
            if let Some(idx) = generic_names.iter().position(|&g| g == name.as_str()) {
                if let Some(concrete) = type_args.get(idx) {
                    return concrete.clone();
                }
            }
            // Also check TypeVar style
            ty.clone()
        }
        Ty::TypeVar(name) => {
            if let Some(idx) = generic_names.iter().position(|&g| g == name.as_str()) {
                if let Some(concrete) = type_args.get(idx) {
                    return concrete.clone();
                }
            }
            ty.clone()
        }
        // Recursively substitute in all other type constructors
        _ => ty.map_children(&|child| substitute_type_params(child, generic_names, type_args)),
    }
}

impl FuncCompiler<'_> {
    /// Resolve the inner type of a ResultOk/ResultErr when inner.ty is Unknown.
    /// Tries: 1) outer expr.ty Result[T,E] args, 2) inner expr IR kind inference.
    pub(super) fn resolve_result_inner_ty(&self, expr: &IrExpr, is_ok: bool) -> Ty {
        use almide_lang::types::constructor::TypeConstructorId;
        // Try from outer Result type
        if let Ty::Applied(TypeConstructorId::Result, args) = &expr.ty {
            let candidate = if is_ok {
                args.first().cloned().unwrap_or(Ty::Unknown)
            } else {
                args.get(1).cloned().unwrap_or(Ty::Unknown)
            };
            if !matches!(candidate, Ty::Unknown) {
                return candidate;
            }
        }
        // Fall back to inferring from inner expr
        let inner = match &expr.kind {
            IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e } => e,
            _ => return Ty::Int,
        };
        self.infer_type_from_expr(inner)
    }

    /// Best-effort type inference from IR expression structure.
    pub(super) fn infer_type_from_expr(&self, expr: &IrExpr) -> Ty {
        if !matches!(expr.ty, Ty::Unknown) {
            return expr.ty.clone();
        }
        match &expr.kind {
            IrExprKind::LitInt { .. } => Ty::Int,
            IrExprKind::LitFloat { .. } => Ty::Float,
            IrExprKind::LitBool { .. } => Ty::Bool,
            IrExprKind::LitStr { .. } => Ty::String,
            IrExprKind::BinOp { op, left, .. } => {
                match op {
                    BinOp::AddInt | BinOp::SubInt | BinOp::MulInt | BinOp::DivInt | BinOp::ModInt
                    | BinOp::PowInt => Ty::Int,
                    BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat | BinOp::DivFloat
                    | BinOp::ModFloat | BinOp::PowFloat => Ty::Float,
                    BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte
                    | BinOp::And | BinOp::Or => Ty::Bool,
                    BinOp::ConcatStr => Ty::String,
                    BinOp::MulMatrix | BinOp::AddMatrix | BinOp::SubMatrix | BinOp::ScaleMatrix => Ty::Matrix,
                    BinOp::ConcatList => {
                        let lt = self.infer_type_from_expr(left);
                        lt
                    }
                }
            }
            IrExprKind::UnOp { op, .. } => {
                match op {
                    UnOp::NegInt => Ty::Int,
                    UnOp::NegFloat => Ty::Float,
                    UnOp::Not => Ty::Bool,
                }
            }
            IrExprKind::Var { id } => {
                self.var_table.get(*id).ty.clone()
            }
            _ => Ty::Int, // conservative fallback
        }
    }

    /// Try to emit an inverted condition + br_if, avoiding a redundant i32_eqz.
    /// Returns true if successfully handled, false if caller should fall back.
    pub(super) fn try_emit_inverted_br_if(&mut self, cond: &IrExpr, br_depth: u32) -> bool {
        match &cond.kind {
            // k != 0 → emit k; i64.eqz; br_if (break when k == 0)
            IrExprKind::BinOp { op: BinOp::Neq, left, right } => {
                // Special case: x != 0 → i64.eqz
                if matches!(&right.kind, IrExprKind::LitInt { value: 0 }) && matches!(&left.ty, Ty::Int) {
                    self.emit_expr(left);
                    wasm!(self.func, { i64_eqz; br_if(br_depth); });
                    return true;
                }
                // General: x != y → emit eq, br_if (break when equal)
                self.emit_eq(left, right, false); // emit eq (no negate)
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            // x < y → emit x, y, ge_s, br_if (break when x >= y)
            IrExprKind::BinOp { op: BinOp::Lt, left, right } => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Gte);
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            // x > y → emit x, y, le_s, br_if
            IrExprKind::BinOp { op: BinOp::Gt, left, right } => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Lte);
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            // x <= y → emit x, y, gt_s, br_if
            IrExprKind::BinOp { op: BinOp::Lte, left, right } => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Gt);
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            // x >= y → emit x, y, lt_s, br_if
            IrExprKind::BinOp { op: BinOp::Gte, left, right } => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Lt);
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            // x == y → emit neq, br_if
            IrExprKind::BinOp { op: BinOp::Eq, left, right } => {
                self.emit_eq(left, right, true); // emit neq
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            // not(x) → emit x, br_if (no inversion needed)
            IrExprKind::UnOp { op: UnOp::Not, operand } => {
                self.emit_expr(operand);
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            _ => false,
        }
    }

}

/// Check if an expression tree references a specific variable.
fn expr_references_var(expr: &almide_ir::IrExpr, var: almide_ir::VarId) -> bool {
    match &expr.kind {
        IrExprKind::Var { id } => *id == var,
        IrExprKind::BinOp { left, right, .. } => expr_references_var(left, var) || expr_references_var(right, var),
        IrExprKind::UnOp { operand, .. } => expr_references_var(operand, var),
        IrExprKind::Call { args, .. } => args.iter().any(|a| expr_references_var(a, var)),
        IrExprKind::Member { object, .. } => expr_references_var(object, var),
        IrExprKind::If { cond, then, else_ } => expr_references_var(cond, var) || expr_references_var(then, var) || expr_references_var(else_, var),
        _ => false,
    }
}

impl FuncCompiler<'_> {
    /// Detect and emit optimized while loop for string append:
    ///   while i < N { s = s + "x"; i = i + 1 }
    /// Hoists len/cap into locals for zero-reload tight loop.
    fn try_emit_string_append_loop(&mut self, cond: &IrExpr, body: &[almide_ir::IrStmt]) -> bool {
        use almide_ir::{IrStmtKind, BinOp, VarId};

        // Match body: exactly 2 statements
        if body.len() != 2 { return false; }

        // Statement 0: s = s + LitStr(1-char)
        let (str_var, byte_val) = if let IrStmtKind::Assign { var, value } = &body[0].kind {
            if let IrExprKind::BinOp { op: BinOp::ConcatStr, left, right } = &value.kind {
                if let (IrExprKind::Var { id }, IrExprKind::LitStr { value: lit }) = (&left.kind, &right.kind) {
                    if *id == *var && lit.len() == 1 {
                        (*var, lit.as_bytes()[0])
                    } else { return false; }
                } else { return false; }
            } else { return false; }
        } else { return false; };

        // Statement 1: i = i + 1
        let counter_var = if let IrStmtKind::Assign { var, value } = &body[1].kind {
            if let IrExprKind::BinOp { op: BinOp::AddInt, left, right } = &value.kind {
                if let (IrExprKind::Var { id }, IrExprKind::LitInt { value: 1 }) = (&left.kind, &right.kind) {
                    if *id == *var { *var } else { return false; }
                } else { return false; }
            } else { return false; }
        } else { return false; };

        // Guard: condition must not reference the string variable (its len is hoisted into a local)
        if expr_references_var(cond, str_var) { return false; }

        // Get local indices
        let str_local = match self.var_map.get(&str_var.0) { Some(&v) => v, None => return false };
        let counter_local = match self.var_map.get(&counter_var.0) { Some(&v) => v, None => return false };

        // Emit optimized loop with hoisted len/cap
        let s = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let cap = self.scratch.alloc_i32();

        // Hoist: load len and cap from string header
        wasm!(self.func, {
            local_get(str_local); local_tee(s);
            i32_load(0); local_set(len);
            local_get(s);
            i32_load(super::list_layout::STRING_CAP_OFFSET as u32);
            local_set(cap);
            // Loop
            block_empty; loop_empty;
        });
        let _g3 = self.depth_push();
        let break_depth = _g3.saved();
        let _g4 = self.depth_push(); // for loop_empty above (we're inside block+loop)

        // Condition check
        self.emit_expr(cond);
        wasm!(self.func, {
            i32_eqz;
            br_if(self.depth - break_depth - 1);
        });

        // Fast path: len < cap → inline byte store (NO memory read for len/cap)
        wasm!(self.func, {
            local_get(len); local_get(cap); i32_lt_u;
            if_empty;
              local_get(s);
              i32_const(super::list_layout::STRING_DATA_OFFSET);
              i32_add;
              local_get(len); i32_add;
              i32_const(byte_val as i32);
              i32_store8(0);
              local_get(len); i32_const(1); i32_add; local_set(len);
            else_;
              // Slow: write len back, grow, reload s/cap
              local_get(s); local_get(len); i32_store(0);
              // new_cap = max(cap*2, 16)
              local_get(cap); i32_const(1); i32_shl; local_tee(cap);
              i32_const(16); i32_lt_u;
              if_empty; i32_const(16); local_set(cap); end;
              // Alloc
              local_get(cap);
              i32_const(super::list_layout::STRING_DATA_OFFSET);
              i32_add;
              call(self.emitter.rt.alloc); local_tee(s);
              // Copy old data
              i32_const(super::list_layout::STRING_DATA_OFFSET); i32_add;
              local_get(str_local);
              i32_const(super::list_layout::STRING_DATA_OFFSET); i32_add;
              local_get(len);
              memory_copy;
              // Write cap
              local_get(s); local_get(cap);
              i32_store(super::list_layout::STRING_CAP_OFFSET as u32);
              // Update str local
              local_get(s); local_set(str_local);
              // Write byte
              local_get(s);
              i32_const(super::list_layout::STRING_DATA_OFFSET);
              i32_add;
              local_get(len); i32_add;
              i32_const(byte_val as i32);
              i32_store8(0);
              local_get(len); i32_const(1); i32_add; local_set(len);
            end;
            // i++
            local_get(counter_local);
            i64_const(1); i64_add;
            local_set(counter_local);
        });

        // Continue
        wasm!(self.func, { br(0); });

        self.depth_pop(_g4);
        self.depth_pop(_g3);
        wasm!(self.func, { end; end; });

        // Write final len back to memory
        wasm!(self.func, {
            local_get(s); local_get(len); i32_store(0);
        });

        self.scratch.free_i32(cap);
        self.scratch.free_i32(len);
        self.scratch.free_i32(s);
        true
    }

    /// Check if `maybe_mod` is `x % n` with power-of-2 n and `maybe_zero` is `0`.
    /// Returns `(x_expr, n-1)` for emitting `x & (n-1)` instead.
    fn extract_mod_pow2_eq_zero<'b>(maybe_mod: &'b IrExpr, maybe_zero: &'b IrExpr) -> Option<(&'b IrExpr, i64)> {
        if let IrExprKind::LitInt { value: 0 } = &maybe_zero.kind {
            if let IrExprKind::BinOp { op: BinOp::ModInt, left, right } = &maybe_mod.kind {
                if let IrExprKind::LitInt { value: n } = &right.kind {
                    let n = *n;
                    if n > 0 && (n as u64).is_power_of_two() {
                        return Some((left, n - 1));
                    }
                }
            }
        }
        None
    }
}
