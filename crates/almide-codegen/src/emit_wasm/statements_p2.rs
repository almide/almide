impl FuncCompiler<'_> {
    pub(super) fn is_heap_type(ty: &Ty) -> bool {
        // `Ty::Named` (declared nominal record/variant) is a heap pointer — include
        // it so a record FIELD that is itself a declared type is recursively dec'd
        // by `emit_typed_rc_dec`. Mirrors pass_perceus::is_heap_type.
        matches!(ty, Ty::String | Ty::Applied(_, _) | Ty::Record { .. } | Ty::Named(..) | Ty::Unknown)
    }

    /// EARLY-RETURN LEAK FIX: free every heap local currently live (`live_heap`), for a
    /// `Try`/`Unwrap`/`Fan` Err-path `return_`. The bare `return_` jumps PAST the Perceus
    /// terminal rc_decs, so without this the live heap locals leak on the Err path. LIFO
    /// (scope-teardown) order for deterministic bytes. Mirrors the RcDec handler per var
    /// (a captured shared cell gets a PLAIN rc_dec; everything else a typed rc_dec). The
    /// returned Err ptr is a scratch temp, not a tracked VarId, so it is never freed here.
    pub(super) fn emit_early_return_decs(&mut self) {
        let live: Vec<almide_ir::VarId> = self.live_heap.iter().rev().copied().collect();
        for var in live {
            if let Some(&local_idx) = self.var_map.get(&var.0) {
                if self.emitter.mutable_captures.contains(&var.0) {
                    wasm!(self.func, { local_get(local_idx); call(self.emitter.rt.rc_dec); });
                } else {
                    let ty = self.var_table.get(var).ty.clone();
                    self.emit_typed_rc_dec(&ty, local_idx);
                }
            }
        }
    }

    /// Perceus Rule 3: type-specialized rc_dec.
    /// For compound types, recursively rc_dec children before freeing the parent.
    /// All offsets derived from LayoutRegistry — zero magic numbers.
    pub(super) fn emit_typed_rc_dec(&mut self, ty: &Ty, local_idx: u32) {
        use almide_lang::types::TypeConstructorId;
        use super::engine::{WasmBuilder, layout::*};

        // A declared nominal record/variant (`type P = {...}`) is `Ty::Named`;
        // resolve it to its structural fields so the child-recursion below frees its
        // heap fields. Without this, Named hits the `_ => false` (childless) arm and
        // its String / nested-heap fields leak. An opaque alias with no registered
        // fields (`type H = String`) resolves to none and falls through to a plain
        // dec of the aliased heap block, which is correct.
        if let Ty::Named(..) = ty {
            let fields = self.extract_record_fields(ty);
            if !fields.is_empty() {
                let rec = Ty::Record {
                    fields: fields.into_iter()
                        .map(|(n, t)| (almide_base::intern::sym(n.as_str()), t))
                        .collect(),
                };
                self.emit_typed_rc_dec(&rec, local_idx);
                return;
            }
        }

        let rc_dec_fn = self.emitter.rt.rc_dec;
        let rc_neg = self.emitter.layout_reg.alloc_header_neg_offset(alloc::RC);

        // if ptr != null {
        wasm!(self.func, { local_get(local_idx); if_empty; });

        let has_children = match ty {
            Ty::Applied(TypeConstructorId::List, args)
            | Ty::Applied(TypeConstructorId::Set, args) =>
                args.first().map_or(false, |t| Self::is_heap_type(t)),
            Ty::Applied(TypeConstructorId::Option, args) =>
                args.first().map_or(false, |t| Self::is_heap_type(t)),
            Ty::Applied(TypeConstructorId::Result, args) =>
                args.iter().any(|t| Self::is_heap_type(t)),
            Ty::Applied(TypeConstructorId::Map, args) =>
                args.iter().any(|t| Self::is_heap_type(t)),
            Ty::Record { fields } =>
                fields.iter().any(|(_, t)| Self::is_heap_type(t)),
            Ty::Tuple(tys) =>
                tys.iter().any(|t| Self::is_heap_type(t)),
            Ty::Fn { .. } => true,
            _ => false,
        };

        if has_children {
            // if rc <= 1 (about to die) { drop children }
            {
                let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
                w.get(local_idx).i32c(rc_neg as i32).sub().emit_load(0, MemType::I32);
                w.i32c(1).raw(wasm_encoder::Instruction::I32LeU);
                w.raw(wasm_encoder::Instruction::If(wasm_encoder::BlockType::Empty));
            }

            match ty {
                // ── List/Set[HeapType]: iterate elements, rc_dec each. Set has the
                // identical [len][elem0..] layout, so the List element-walk applies ──
                Ty::Applied(TypeConstructorId::List, args)
                | Ty::Applied(TypeConstructorId::Set, args) => {
                    let elem_size = super::values::byte_size(&args[0]);
                    let elem = self.scratch.alloc_i32();
                    let idx = self.scratch.alloc_i32();
                    {
                        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
                        w.list_foreach(local_idx, elem, idx, elem_size, |w| {
                            w.get(elem).emit_load(0, MemType::I32);
                            w.if_void(
                                |w| { w.get(elem).emit_load(0, MemType::I32).call(rc_dec_fn); },
                                |_| {},
                            );
                        });
                    }
                    self.scratch.free_i32(idx);
                    self.scratch.free_i32(elem);
                }
                // ── Option[HeapType]: if Some, dec payload ──
                // The Option ABI is a NULLABLE box with the payload at offset 0
                // and no tag (none = null ptr) — see every Option constructor
                // and calls_option.rs is_some/unwrap. The tagged [TAG][PAYLOAD]
                // shape that used to live here read 4 bytes PAST the 4-byte box
                // and rc_dec'd neighbouring-allocation garbage; once that
                // garbage exceeded the heap top it bypassed the header guard
                // and trapped OOB (#470 — only surfaced on large heaps).
                Ty::Applied(TypeConstructorId::Option, args) => {
                    if Self::is_heap_type(&args[0]) {
                        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
                        w.get(local_idx).emit_load(0, MemType::I32);
                        w.if_void(
                            |w| { w.get(local_idx).emit_load(0, MemType::I32).call(rc_dec_fn); },
                            |_| {},
                        );
                    }
                }
                // ── Result[T, E]: dec matching variant's payload ──
                Ty::Applied(TypeConstructorId::Result, args) => {
                    for (i, arg) in args.iter().enumerate() {
                        if Self::is_heap_type(arg) {
                            let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
                            w.get(local_idx).field_load(RESULT, tagged::TAG);
                            w.i32c(i as i32).eq();
                            w.if_void(|w| {
                                w.get(local_idx).field_load(RESULT, tagged::PAYLOAD);
                                w.if_void(
                                    |w| { w.get(local_idx).field_load(RESULT, tagged::PAYLOAD).call(rc_dec_fn); },
                                    |_| {},
                                );
                            }, |_| {});
                        }
                    }
                }
                // ── Record: dec each heap-typed field ──
                // Fields are walked in GIVEN (declaration) order — the same
                // order construction (emit_record), member access
                // (field_offset), and spread use. The original name-sorted
                // walk computed WRONG offsets whenever alphabetical order
                // diverged from declaration order with mixed field sizes,
                // assembling fake pointers out of neighboring bytes
                // (sized_int_record_fields trap) or silently leaking.
                Ty::Record { fields } => {
                    let mut offset = 0u32;
                    for (_, field_ty) in fields {
                        if Self::is_heap_type(field_ty) {
                            let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
                            w.get(local_idx).emit_load(offset, MemType::I32);
                            w.if_void(
                                |w| { w.get(local_idx).emit_load(offset, MemType::I32).call(rc_dec_fn); },
                                |_| {},
                            );
                        }
                        offset += super::values::byte_size(field_ty);
                    }
                }
                // ── Tuple: dec each heap-typed element (positional, no name sort) ──
                Ty::Tuple(tys) => {
                    let mut offset = 0u32;
                    for elem_ty in tys {
                        if Self::is_heap_type(elem_ty) {
                            let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
                            w.get(local_idx).emit_load(offset, MemType::I32);
                            w.if_void(
                                |w| { w.get(local_idx).emit_load(offset, MemType::I32).call(rc_dec_fn); },
                                |_| {},
                            );
                        }
                        offset += super::values::byte_size(elem_ty);
                    }
                }
                // ── Map[K, V]: Swiss Table iteration, dec live entries ──
                Ty::Applied(TypeConstructorId::Map, args) => {
                    let key_ty = args.get(0);
                    let val_ty = args.get(1);
                    let key_heap = key_ty.map_or(false, |t| Self::is_heap_type(t));
                    let val_heap = val_ty.map_or(false, |t| Self::is_heap_type(t));
                    if key_heap || val_heap {
                        let key_size = key_ty.map_or(8, |t| super::values::byte_size(t));
                        let val_size = val_ty.map_or(8, |t| super::values::byte_size(t));
                        let entry_stride = key_size + val_size;
                        let entry = self.scratch.alloc_i32();
                        let cap_l = self.scratch.alloc_i32();
                        let eb = self.scratch.alloc_i32();
                        let idx = self.scratch.alloc_i32();
                        {
                            let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
                            w.map_foreach(local_idx, entry, cap_l, eb, idx, entry_stride, |w| {
                                if key_heap {
                                    w.get(entry).emit_load(0, MemType::I32);
                                    w.if_void(
                                        |w| { w.get(entry).emit_load(0, MemType::I32).call(rc_dec_fn); },
                                        |_| {},
                                    );
                                }
                                if val_heap {
                                    w.get(entry).emit_load(key_size, MemType::I32);
                                    w.if_void(
                                        |w| { w.get(entry).emit_load(key_size, MemType::I32).call(rc_dec_fn); },
                                        |_| {},
                                    );
                                }
                            });
                        }
                        self.scratch.free_i32(idx);
                        self.scratch.free_i32(eb);
                        self.scratch.free_i32(cap_l);
                        self.scratch.free_i32(entry);
                    }
                }
                // ── Fn (closure): dec env_ptr ──
                Ty::Fn { .. } => {
                    let env = self.scratch.alloc_i32();
                    {
                        let mut w = WasmBuilder::new(&mut self.func, &self.emitter.layout_reg);
                        w.get(local_idx).field_load(CLOSURE_PAIR, closure::ENV_PTR).set(env);
                        w.get(env);
                        w.if_void(|w| { w.get(env).call(rc_dec_fn); }, |_| {});
                    }
                    self.scratch.free_i32(env);
                }
                _ => {}
            }

            wasm!(self.func, { end; }); // end RC==1 check
        }
        // rc_dec the parent
        wasm!(self.func, { local_get(local_idx); call(rc_dec_fn); });
        wasm!(self.func, { end; }); // end non-null check
    }

    /// Compute drop schedule for a block: maps statement index → list of local indices to rc_dec.
    /// For each heap-typed Bind in the block, finds the last statement that references it.
    /// Variables used in the tail expression or not locally bound are excluded.
    pub(super) fn compute_block_drop_schedule(
        &self,
        stmts: &[IrStmt],
        tail: Option<&IrExpr>,
    ) -> HashMap<usize, Vec<u32>> {
        // Collect heap-typed locals bound in THIS block
        let mut block_locals: HashMap<VarId, u32> = HashMap::new(); // var → wasm local index
        for stmt in stmts {
            if let IrStmtKind::Bind { var, ty, .. } = &stmt.kind {
                if Self::is_heap_type(ty) {
                    if let Some(&idx) = self.var_map.get(&var.0) {
                        block_locals.insert(*var, idx);
                    }
                }
            }
        }
        if block_locals.is_empty() { return HashMap::new(); }

        // Find variables used in tail expression — don't drop those (they're returned)
        let mut tail_vars: std::collections::HashSet<VarId> = std::collections::HashSet::new();
        if let Some(tail) = tail {
            collect_var_refs(tail, &mut tail_vars);
        }

        // For each block-local, find the last statement index where it's referenced
        let mut last_use: HashMap<VarId, usize> = HashMap::new();
        for (i, stmt) in stmts.iter().enumerate() {
            let mut refs = std::collections::HashSet::new();
            collect_stmt_var_refs(stmt, &mut refs);
            for var in &refs {
                if block_locals.contains_key(var) {
                    last_use.insert(*var, i);
                }
            }
        }

        // Build schedule: stmt_index → [local_indices to drop]
        let mut schedule: HashMap<usize, Vec<u32>> = HashMap::new();
        for (var, stmt_idx) in &last_use {
            // Skip if used in tail or if this is the binding statement itself
            if tail_vars.contains(var) { continue; }
            if let Some(&local_idx) = block_locals.get(var) {
                // Don't drop at the bind statement — the value was just created
                let bind_idx = stmts.iter().position(|s| {
                    matches!(&s.kind, IrStmtKind::Bind { var: v, .. } if *v == *var)
                });
                if bind_idx == Some(*stmt_idx) { continue; } // only use is the bind itself
                schedule.entry(*stmt_idx).or_default().push(local_idx);
            }
        }
        schedule
    }

    /// Check if an expression writes to outer-scope mutable variables with heap types.
    /// Used by auto-scope to determine if heap_restore is safe.
    pub(super) fn expr_writes_outer_heap(&self, expr: &IrExpr) -> bool {
        struct HeapWriteScanner<'a> {
            var_table: &'a almide_ir::VarTable,
            found: bool,
        }
        impl IrVisitor for HeapWriteScanner<'_> {
            fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
                if self.found { return; }
                match &stmt.kind {
                    IrStmtKind::Assign { var, .. }
                    | IrStmtKind::MapInsert { target: var, .. }
                    | IrStmtKind::IndexAssign { target: var, .. }
                    | IrStmtKind::FieldAssign { target: var, .. } => {
                        // Escape check must be CONSERVATIVE: anything that is
                        // not provably a scalar may point into the iteration
                        // arena, and a heap_restore would free it while live.
                        // `is_heap_type` is the wrong predicate here — it
                        // misses nominal types (json.Value, user types, Bytes),
                        // which is how a `var gltf = json.null(); while {...
                        // gltf = parsed ...}` loop got its tree reclaimed and
                        // overwritten by the next allocation (#470).
                        let ty = &self.var_table.get(*var).ty;
                        let scalar = matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit);
                        if !scalar {
                            self.found = true;
                        }
                    }
                    _ => {}
                }
                walk_stmt(self, stmt);
            }
            fn visit_expr(&mut self, expr: &IrExpr) {
                if self.found { return; }
                // An in-place mutator CALL on a non-scalar loop-outer var writes heap that
                // outlives the iteration — the scanner must catch the CALL, not just Assign
                // STATEMENTS. Missing it lets the iter-scope reclamation roll the frontier
                // back over the reallocated backing; a spurious second __rc_dec at teardown
                // then hits the rc==0 sentinel (an `unreachable` trap on wasm).
                if let almide_ir::IrExprKind::RuntimeCall { symbol, args } = &expr.kind {
                    if crate::pass_closure_conversion::is_inplace_mutator(symbol.as_str()) {
                        if let Some(almide_ir::IrExprKind::Var { id }) = args.first().map(|a| &a.kind) {
                            let ty = &self.var_table.get(*id).ty;
                            let scalar = matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit);
                            if !scalar {
                                self.found = true;
                                return;
                            }
                        }
                    }
                }
                walk_expr(self, expr);
            }
        }
        let mut scanner = HeapWriteScanner { var_table: self.var_table, found: false };
        scanner.visit_expr(expr);
        scanner.found
    }

    /// Check if an expression allocates heap memory (string/list/record construction,
    /// or calls returning heap types). Used to decide if iter_scope is worthwhile.
    pub(super) fn expr_allocates_heap(&self, expr: &IrExpr) -> bool {
        struct AllocScanner { found: bool }
        impl IrVisitor for AllocScanner {
            fn visit_expr(&mut self, expr: &IrExpr) {
                if self.found { return; }
                match &expr.kind {
                    // Direct heap allocations
                    IrExprKind::LitStr { .. }
                    | IrExprKind::StringInterp { .. }
                    | IrExprKind::List { .. }
                    | IrExprKind::Record { .. }
                    | IrExprKind::MapLiteral { .. } => {
                        self.found = true;
                        return;
                    }
                    // Calls that return heap types
                    IrExprKind::Call { .. } | IrExprKind::TailCall { .. }
                    | IrExprKind::RuntimeCall { .. } => {
                        if FuncCompiler::is_heap_type(&expr.ty) {
                            self.found = true;
                            return;
                        }
                    }
                    // String concat
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        self.found = true;
                        return;
                    }
                    _ => {}
                }
                walk_expr(self, expr);
            }
            fn visit_stmt(&mut self, stmt: &almide_ir::IrStmt) {
                if self.found { return; }
                walk_stmt(self, stmt);
            }
        }
        let mut scanner = AllocScanner { found: false };
        scanner.visit_expr(expr);
        scanner.found
    }

}
