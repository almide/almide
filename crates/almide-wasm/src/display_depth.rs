// ── display.rs, the hold-depth budget ──
//
// include!-spliced into `display.rs` at module level (the 800-line file
// discipline). The imports are display.rs's own.

impl Emitter<'_> {
    /// `got`'s display at the cursor. A structural container whose inlined
    /// holds would exceed what the i32 pool has left (a list holds three per
    /// level, so eight nested lists under an `assert_eq` walled) becomes a
    /// call to its outlined `$displayty_<ety>` helper on a fresh pool.
    /// Containers ignore `nested` (their children are always nested), so
    /// the helper is keyed by type and use-site IR alone.
    fn emit_display_at(
        &mut self,
        got: SliceTy,
        nested: bool,
        ir: Option<&Ty>,
        path: &mut Vec<u32>,
    ) -> Result<(), EmitError> {
        // A runtime-backed type shows its DECLARED repr (#4204), at any depth:
        // its self-hosted carrier (`HttpRequest` is a `List[String]`) holds
        // what the repr hides, so the carrier's own display never runs.
        if let Some(i) = self.declared_repr_fn(ir) {
            return self.emit_declared_repr(i, None);
        }
        let container = matches!(
            got,
            SliceTy::List(_) | SliceTy::Set(_) | SliceTy::Map(..) | SliceTy::Option(_) | SliceTy::Result(..) | SliceTy::Tuple(_)
        );
        if !container
            || self.hold_i32_depth + self.display_hold_need(got, path) <= crate::emitter::HOLD_I32_POOL
        {
            return self.emit_display_level(got, nested, ir, path);
        }
        let ety = self.types.intern(got).index() as u32;
        let irk = self.work.display_ir_key_any(ir);
        if matches!(self.work.display_ty_bodies.borrow().get(&(ety, irk)), Some(crate::work::DisplayBuild::Failed)) {
            return unsup("display-helper-failed");
        }
        let idx = self.work.helper(Helper::DisplayTy { ety, irk });
        self.f.instructions().local_get(self.cursor_local).call(idx).local_set(self.cursor_local);
        Ok(())
    }

    /// Whether the display of a NON-recursive named type should become a
    /// call to its `$display_<ti>` helper instead of inlining: its inlined
    /// shape would hold more i32 slots than the pool has left (a chain of
    /// records each holding nested lists — the display twin of #3450).
    /// Anonymous record shapes stay inline: their leaves read the IR type
    /// from the use site, which a helper keyed by `ti` would not see.
    fn display_outlines(&self, ti: u32, path: &mut Vec<u32>) -> bool {
        !self.types.name_of(ti).is_empty()
            && self.hold_i32_depth + self.display_hold_need(SliceTy::Named(ti), path)
                > crate::emitter::HOLD_I32_POOL
    }

    /// How many i32 holds inlining `ty`'s display keeps live at its deepest
    /// point (mirrors `emit_display_at` and its splits; a Named on `path`
    /// is already a call and holds none).
    fn display_hold_need(&self, ty: SliceTy, path: &mut Vec<u32>) -> u32 {
        match ty {
            SliceTy::Option(h) => 1 + self.display_hold_need(self.types.el(h), path),
            SliceTy::Result(o, e) => {
                1 + self.display_hold_need(self.types.el(o), path).max(self.display_hold_need(self.types.el(e), path))
            }
            SliceTy::List(h) | SliceTy::Set(h) => 3 + self.display_hold_need(self.types.el(h), path),
            SliceTy::Map(k, v) => {
                3 + self.display_hold_need(self.types.el(k), path).max(self.display_hold_need(self.types.el(v), path))
            }
            SliceTy::Tuple(id) => {
                let fs: Vec<SliceTy> = self.types.tuple_def(id).fields.iter().map(|(t, _)| *t).collect();
                1 + self.display_fields_need(&fs, path)
            }
            SliceTy::Value => 1,
            SliceTy::Named(ti) if !path.contains(&ti) => {
                let fs: Vec<SliceTy> = match &self.types.def(ti) {
                    NamedDef::Record(r) => r.fields.iter().map(|f| f.ty).collect(),
                    NamedDef::Variant(v) => v.cases.iter().flat_map(|c| c.fields.iter().map(|f| f.ty)).collect(),
                    NamedDef::Excluded => return 0,
                };
                path.push(ti);
                let need = 1 + self.display_fields_need(&fs, path);
                path.pop();
                need
            }
            _ => 0,
        }
    }

    fn display_fields_need(&self, fields: &[SliceTy], path: &mut Vec<u32>) -> u32 {
        fields.iter().map(|t| self.display_hold_need(*t, path)).max().unwrap_or(0)
    }

    /// The linked repr function of the runtime-backed type `ir` names
    /// (#4204), from its row: `stdlib_info::runtime_backed_repr_surface`.
    /// `None` for every other type, for a user type that shadows the bare
    /// name, and for a runtime-backed type whose module declares no repr.
    fn declared_repr_fn(&self, ir: Option<&Ty>) -> Option<usize> {
        let Some(Ty::Named(name, args)) = ir else { return None };
        if !args.is_empty() || self.types.by_name.contains_key(name.as_str()) {
            return None;
        }
        let surface = almide_types::stdlib_info::runtime_backed_repr_surface(name.as_str())?;
        self.resolve_qualified(&surface)
    }

    /// The runtime-backed value on the stack, appended through its declared
    /// repr `i` (a `(T) -> String` that borrows its argument). The text it
    /// hands over is copied into the line and released here — the
    /// `emit_float_display` discipline.
    /// `src`: the frame variable the value was read from, when it was — the
    /// share an owning repr takes is that variable's, moved into the callee
    /// (`am`, the call-argument convention).
    fn emit_declared_repr(&mut self, i: usize, src: Option<u32>) -> Result<(), EmitError> {
        let info = &self.table.infos[i];
        if info.refuse.is_some() || info.ret != Some(STR) || info.params.len() != 1 {
            return unsup("interp-part:declared-repr-impl");
        }
        // The display walk only lends the value: a callee that owns its
        // param releases it at exit, so it gets its own credit first (the
        // RC-3 callee-owned convention, `lower_conv_arg`).
        let owned = info.param_owned.first().copied().unwrap_or(true);
        let idx = info.wasm_index;
        if owned {
            self.rc_inc_top();
            if let (Some(l), Some(w)) = (src, self.witness.as_mut())
                && !w.arg_share_move(l)
            {
                w.poison();
            }
        }
        self.calls.insert(i);
        self.witness_discard();
        // The repr builds its text with interpolations of its own: publish
        // this line's cursor so that build starts past what is already here
        // (the `lower_interp_part` rule).
        self.f
            .instructions()
            .local_get(self.cursor_local)
            .global_set(G_LINE_CURSOR)
            .call(idx)
            .local_set(self.tmp_i32_local);
        self.f
            .instructions()
            .local_get(self.cursor_local)
            .local_get(self.tmp_i32_local)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(self.tmp_i32_local)
            .i32_load(len_memarg())
            .call(F_APPEND_COPY)
            .local_set(self.cursor_local);
        let dec = self.dec_fn_of(STR);
        self.f.instructions().local_get(self.tmp_i32_local).call(dec);
        Ok(())
    }
}

/// Build every registered `DisplayTy` body not built yet; true when one was
/// (a body may register more, so the caller's fixed point runs again).
fn build_display_ty_helpers(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
    all_calls: &mut std::collections::HashSet<usize>,
) -> Result<bool, EmitError> {
    let todo: Vec<(u32, u32)> = {
        let bodies = work.display_ty_bodies.borrow();
        work.helpers
            .borrow()
            .iter()
            .filter_map(|h| match h {
                Helper::DisplayTy { ety, irk } if !bodies.contains_key(&(*ety, *irk)) => Some((*ety, *irk)),
                _ => None,
            })
            .collect()
    };
    for (ety, irk) in &todo {
        // #2758: the block is lent (param 0); the cursor (param 1) is an i32.
        let hw = crate::witness::helper::HelperWitness::new(format!("<display-ty:{ety}>"), "display", &[0]);
        let ir = work.display_ir(*irk);
        let ty = types.el(crate::ETy::from_index(*ety as usize));
        let built = build_helper_body(table, types, work, pool, (Shell::PAIR, hw), |em| {
            em.f.instructions().local_get(1).local_set(2);
            em.f.instructions().local_get(0);
            em.emit_display_level(ty, true, ir.as_ref(), &mut Vec::new())?;
            em.f.instructions().local_get(2);
            Ok(())
        });
        let entry = match built {
            Ok((f, calls)) => {
                all_calls.extend(calls.iter().copied());
                crate::work::DisplayBuild::Built(f)
            }
            Err(e) => {
                work.display_ty_bodies.borrow_mut().insert((*ety, *irk), crate::work::DisplayBuild::Failed);
                return Err(e);
            }
        };
        work.display_ty_bodies.borrow_mut().insert((*ety, *irk), entry);
    }
    Ok(!todo.is_empty())
}
