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
