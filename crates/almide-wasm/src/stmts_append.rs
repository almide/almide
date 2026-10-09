//! The growing-accumulator ASSIGN windows (`acc = acc + s`, `data =
//! data + [x]`) — split from stmts.rs for the 800-line file budget
//! (#1729 pushed it over); pure text move, same `impl Emitter` surface.

use almide_ir::IrExprKind;
use wasm_encoder::ValType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// The growing-accumulator window (`acc = acc + s`, Str): route
    /// through $str_append — in place under rc == 1 with class slack,
    /// else concat + release of the outgrown block. Without this the
    /// Assign dec-skip (rhs mentions the var) plus F_CONCAT's borrow
    /// semantics leaked every outgrown accumulator
    /// (spec/churn/string_accumulator_churn OOM'd at the commissioning).
    /// UNMETERED only — the metered ConcatStr path carries the T3-5
    /// dynamic charge and region programs keep that exact cost model.
    pub(crate) fn try_str_append_assign(
        &mut self,
        var: &almide_ir::VarId,
        value: &IrExpr,
    ) -> Result<bool, EmitError> {
        if self.metered || self.cells.contains(var) {
            return Ok(false);
        }
        let Some(&(idx, SliceTy::Scalar(Scalar::Str))) = self.locals.get(var) else {
            return Ok(false);
        };
        let is_var = |e: &IrExpr| matches!(&e.kind, IrExprKind::Var { id } if id == var);
        // #3501: `s = "${s}…"` appends its later pieces, built as one
        // fresh string, exactly as `s = s + "…"` appends its right side.
        let tail;
        let right = match concat_operands(value, almide_ir::BinOp::ConcatStr) {
            Some((left, right)) if is_var(left) => right,
            _ => match interp_tail(value, *var, is_var) {
                Some((_, t)) => {
                    tail = t;
                    &tail
                }
                None => return Ok(false),
            },
        };
        self.f.instructions().local_get(idx);
        self.lower(right, Some(STR))?;
        // `$str_append` BORROWS its operand: an owned one (the call result
        // `concat_operands` unwrapped from arg_temps' `{ let t = …; s + t }`)
        // is released right after, or `s = s + int.to_string(i)` leaked one
        // block per append (#2757: the witness found the unrecorded object).
        let release = self.hold_owned_operand(right)?;
        self.f.instructions().call(F_STR_APPEND).local_set(idx);
        self.release_owned_operand(release);
        self.rc_own(idx, STR);
        Ok(true)
    }

    /// An OWNED string operand on the stack that a borrowing helper is about
    /// to read: parked (teed) in a hold so the site releases it after the
    /// helper; `None` for a borrowed or pool-static operand (nothing to
    /// release). The witness records the pair (`id`).
    pub(crate) fn hold_owned_operand(&mut self, e: &IrExpr) -> Result<Option<u32>, EmitError> {
        if matches!(e.kind, IrExprKind::LitStr { .. }) || !self.rc_owned_result(e) {
            return Ok(None);
        }
        let h = self.hold_i32()?;
        self.f.instructions().local_tee(h);
        self.witness_discard();
        Ok(Some(h))
    }

    /// Release what `hold_owned_operand` parked (stack-neutral).
    pub(crate) fn release_owned_operand(&mut self, held: Option<u32>) {
        if let Some(h) = held {
            let dec = self.dec_fn_of(STR);
            self.f.instructions().local_get(h).call(dec);
            self.release_i32();
        }
    }

    /// The list twin of [`Self::try_str_append_assign`] (#1729):
    /// `data = data + [e]` — the canonical accumulator loop — routes
    /// through `$cow` + `$list_push_{8,4}` (amortized in-place growth,
    /// the outgrown block freed at rc==1) instead of `$concat`'s full
    /// copy per append. The COW judge keeps value semantics for a
    /// shared accumulator; the element is lowered AFTER the judge but
    /// read against the pre-assign block either way, so a
    /// self-referencing element (`data + [list.len(data)]`) observes
    /// the value before the mutation, exactly like the concat form.
    pub(crate) fn try_list_append_assign(
        &mut self,
        var: &almide_ir::VarId,
        value: &IrExpr,
    ) -> Result<bool, EmitError> {
        if self.metered || self.cells.contains(var) {
            return Ok(false);
        }
        let Some(&(idx, SliceTy::List(h))) = self.locals.get(var) else {
            return Ok(false);
        };
        let Some((left, right)) = concat_operands(value, almide_ir::BinOp::ConcatList) else {
            return Ok(false);
        };
        if !matches!(&left.kind, IrExprKind::Var { id } if id == var) {
            return Ok(false);
        }
        let IrExprKind::List { elements } = &right.kind else {
            return Ok(false);
        };
        let [elem] = &elements[..] else {
            return Ok(false);
        };
        let el = self.types.el(h);
        let cow = self.cow_fn_of(SliceTy::List(h));
        self.f.instructions().local_get(idx).call(cow);
        self.lower(elem, Some(el))?;
        if el.val_type() == ValType::F64 {
            self.f.instructions().i64_reinterpret_f64();
        }
        // A 4-byte HANDLE element (#2310) takes the literal builder's Dup
        // discipline, which is what admits it here: the spine becomes a
        // holder, so a BORROWED handle takes its +1 and an OWNED one moves
        // in with its credit (`rc_share_guard`, the same call the list
        // literal and the index store make). Without it the push
        // double-owns the element — the C-186 trap. The copy side already
        // knew how: `cow_fn_of` picks `CowElems` for a handle element, and
        // `$list_push`'s grow path frees the outgrown SPINE untyped, so the
        // elements it memcpy'd keep exactly one credit each.
        self.rc_share_guard(elem, el);
        self.witness_store(elem, el);
        let push = if el.slot_size() == 8 { F_LIST_PUSH_8 } else { F_LIST_PUSH_4 };
        self.f.instructions().call(push).local_set(idx);
        self.rc_own(idx, SliceTy::List(h));
        Ok(true)
    }

    /// The map twin (#1219 stage 1): the rebind `m = map.set(m, k, v)`
    /// — what `m[k] = v` and `map.insert(m, k, v)` are on the incumbent
    /// leg and what a persistent-style program writes by hand — routes
    /// through the in-place window when `m` owns its block. Any other
    /// receiver (`b = map.set(a, k, v)`) keeps the functional copy, so
    /// `a` is untouched.
    pub(crate) fn try_map_set_assign(
        &mut self,
        var: &almide_ir::VarId,
        value: &IrExpr,
    ) -> Result<bool, EmitError> {
        let IrExprKind::Call {
            target: almide_ir::CallTarget::Module { module, func, .. },
            args,
            ..
        } = &value.kind
        else {
            return Ok(false);
        };
        if module.as_str() != "map" || func.as_str() != "set" {
            return Ok(false);
        }
        let [recv, key, val] = &args[..] else {
            return Ok(false);
        };
        if !matches!(&recv.kind, IrExprKind::Var { id } if id == var) {
            return Ok(false);
        }
        self.try_map_set_in_place(var, key, val)
    }
}

/// The resolved field place of [`Emitter::try_field_append_assign`]: the
/// root local, its record type, the (type, offset) of every inner record on
/// the path, and the leaf field's (type, offset).
struct FieldPlace {
    idx: u32,
    root: SliceTy,
    inner: Vec<(SliceTy, u32)>,
    leaf: (SliceTy, u32),
}

/// What the field window appends: a string operand, or one list element.
enum Appended<'a> {
    Str(std::borrow::Cow<'a, IrExpr>),
    Elem(&'a IrExpr),
}

impl Emitter<'_> {
    /// #3501 — the field twin of the accumulator windows above:
    /// `b.text = b.text + s`, `b.text = "${b.text}…"` and
    /// `b.xs = b.xs + [e]` extend the field's block in place instead of
    /// copying the record and rebuilding the whole value (each step was a
    /// full concat: quadratic in the accumulated length). The path is made
    /// unique first (`make_mut_place_unique`, the `mut`-argument judge of
    /// #3101): a record another binding holds is copied and the var
    /// repointed at the copy, so `let old = b` before the loop keeps its
    /// value; then `$str_append` (in place only under rc == 1 with slack,
    /// above the static pool) or `$cow` + `$list_push` writes the leaf, so a
    /// field alias (`let t = b.text`) keeps its value too. The window is
    /// Koka / Lean's reuse-when-unique ("Counting Immutable Beans", Ullrich
    /// & de Moura 2019; Perceus, Reinking et al. 2021) applied to a field
    /// slot: the rc test is the dynamic proof the update is unobservable.
    ///
    /// Admitted: a plain local root (not a C-319 cell, not a global — a call
    /// in the right side could replace either under the held record), the
    /// right side not mentioning the root (it would read the place twice),
    /// an unmetered frame. Anything else keeps the copy-on-write write-back.
    pub(crate) fn try_field_append_assign(
        &mut self,
        target: &almide_ir::VarId,
        path: &[almide_base::intern::Sym],
        value: &IrExpr,
    ) -> Result<bool, EmitError> {
        let Some(place) = self.field_append_place(target, path) else {
            return Ok(false);
        };
        let is_place = |e: &IrExpr| {
            crate::list_mut::record_field_receiver(e).is_some_and(|(id, p)| id == *target && p == path)
        };
        let free_of_root = |e: &IrExpr| !almide_ir::free_vars::free_vars(e, &Default::default()).contains(target);
        let (head, appended) = match place.leaf.0 {
            SliceTy::Scalar(Scalar::Str) => match concat_operands(value, almide_ir::BinOp::ConcatStr) {
                Some((left, right)) if is_place(left) && free_of_root(right) => {
                    (left, Appended::Str(std::borrow::Cow::Borrowed(right)))
                }
                _ => match interp_tail(value, *target, is_place) {
                    Some((head, tail)) => (head, Appended::Str(std::borrow::Cow::Owned(tail))),
                    None => return Ok(false),
                },
            },
            SliceTy::List(_) => match concat_operands(value, almide_ir::BinOp::ConcatList) {
                Some((left, right)) if is_place(left) => match &right.kind {
                    IrExprKind::List { elements } if elements.len() == 1 && free_of_root(&elements[0]) => {
                        (left, Appended::Elem(&elements[0]))
                    }
                    _ => return Ok(false),
                },
                _ => return Ok(false),
            },
            _ => return Ok(false),
        };
        self.emit_field_append(target, head, &place, appended)?;
        Ok(true)
    }

    /// The place `target.path` when the field window may write it: a plain
    /// droppable record local, every inner step a record (each one judged
    /// unique by `make_mut_place_unique`), the leaf a String or a List.
    fn field_append_place(&self, target: &almide_ir::VarId, path: &[almide_base::intern::Sym]) -> Option<FieldPlace> {
        if self.metered || self.cells.contains(target) {
            return None;
        }
        let &(idx, root) = self.locals.get(target)?;
        let (leaf_field, inner_fields) = path.split_last()?;
        let mut cur = root;
        let mut inner = Vec::with_capacity(inner_fields.len());
        for field in inner_fields {
            if !matches!(cur, SliceTy::Named(_)) || !self.rc_droppable(cur) {
                return None;
            }
            let slot = self.record_field_slot(cur, field).ok()?;
            inner.push(slot);
            cur = slot.0;
        }
        if !matches!(cur, SliceTy::Named(_)) || !self.rc_droppable(cur) {
            return None;
        }
        let leaf = self.record_field_slot(cur, leaf_field).ok()?;
        matches!(leaf.0, SliceTy::Scalar(Scalar::Str) | SliceTy::List(_)).then_some(FieldPlace { idx, root, inner, leaf })
    }

    /// Unshare the path, read the leaf's parent record, append, store the
    /// (possibly reallocated) leaf block back into its slot. The leaf's one
    /// credit goes through the helper exactly as a plain accumulator var's
    /// does: `$str_append` / `$list_push` consume it and hand one back.
    fn emit_field_append(
        &mut self,
        target: &almide_ir::VarId,
        head: &IrExpr,
        place: &FieldPlace,
        appended: Appended<'_>,
    ) -> Result<(), EmitError> {
        self.make_mut_place_unique(head)?;
        // `$cow` took the var's credit on the record it held and left the
        // var holding one on the (possibly copied) record — the mut
        // receiver's rebind (`emit_read_mut_var_cow`).
        self.witness_mut_rebind(*target, false);
        let (lty, loff) = place.leaf;
        let hp = self.hold_i32()?;
        self.emit_read_mut_var(target, place.idx, place.root, false);
        for &(fty, off) in &place.inner {
            self.load_ty_slot(fty, off);
        }
        self.f.instructions().local_tee(hp).local_get(hp);
        self.load_ty_slot(lty, loff);
        match appended {
            Appended::Str(right) => {
                self.lower(&right, Some(STR))?;
                let release = self.hold_owned_operand(&right)?;
                self.f.instructions().call(F_STR_APPEND);
                self.store_ty_slot(lty, loff);
                self.release_owned_operand(release);
            }
            Appended::Elem(elem) => {
                let SliceTy::List(h) = lty else { return unsup("field-append:leaf") };
                let el = self.types.el(h);
                self.lower(elem, Some(el))?;
                if el.val_type() == ValType::F64 {
                    self.f.instructions().i64_reinterpret_f64();
                }
                self.rc_share_guard(elem, el);
                self.witness_store(elem, el);
                let push = if el.slot_size() == 8 { F_LIST_PUSH_8 } else { F_LIST_PUSH_4 };
                self.f.instructions().call(push);
                self.store_ty_slot(lty, loff);
            }
        }
        self.release_i32();
        Ok(())
    }
}

/// The operands of an append-shaped concat, through the temporaries
/// binder's wrap: `x + rhs` arrives either bare or as
/// `{ let t = rhs; x + t }` (arg_temps.rs binds a born-here / call-result
/// operand). The fast paths consume `rhs` directly — the temporary never
/// materialises, so nothing is left to release. Without this the append
/// loop fell back to the concat path, whose >512 KB generations are exact
/// allocations outside the freelist classes (2^17 appends OOM'd, #1701).
pub(crate) fn concat_operands(value: &IrExpr, op: almide_ir::BinOp) -> Option<(&IrExpr, &IrExpr)> {
    match &value.kind {
        IrExprKind::BinOp { op: o, left, right } if *o == op => Some((left, right)),
        IrExprKind::Block { stmts, expr: Some(tail) } if stmts.len() == 1 => {
            let almide_ir::IrStmtKind::Bind { var: t, value: bound, .. } = &stmts[0].kind else { return None };
            let IrExprKind::BinOp { op: o, left, right } = &tail.kind else { return None };
            if *o != op || !matches!(&right.kind, IrExprKind::Var { id } if id == t) {
                return None;
            }
            Some((left, bound))
        }
        _ => None,
    }
}

/// #3501: an interpolation that overwrites a place and extends it — its
/// FIRST piece the place itself, no later piece reading the place's root
/// (`almide_ir::free_vars::interp_extends_place`, the rule native's #3454
/// rewrite asks too) — split into that first piece and the later pieces as
/// one fresh string (a lone literal piece is the literal). Through the
/// temporaries binder's wrap (`{ let t = …; "${s}${t}" }`) the binds stay in
/// front of the tail, and none of them may read the root either.
pub(crate) fn interp_tail(
    value: &IrExpr,
    root: almide_ir::VarId,
    is_place: impl Fn(&IrExpr) -> bool,
) -> Option<(&IrExpr, IrExpr)> {
    let (stmts, interp) = match &value.kind {
        IrExprKind::Block { stmts, expr: Some(t) } if matches!(t.kind, IrExprKind::StringInterp { .. }) => {
            (Some(stmts), t.as_ref())
        }
        _ => (None, value),
    };
    let IrExprKind::StringInterp { parts } = &interp.kind else { return None };
    if !almide_ir::free_vars::interp_extends_place(parts, root, is_place) {
        return None;
    }
    let Some(almide_ir::IrStringPart::Expr { expr: head }) = parts.first() else { return None };
    let rest = parts[1..].to_vec();
    let kind = match rest.as_slice() {
        [almide_ir::IrStringPart::Lit { value }] => IrExprKind::LitStr { value: value.clone() },
        _ => IrExprKind::StringInterp { parts: rest },
    };
    let ty = almide_types::types::Ty::String;
    let piece = IrExpr { kind, ty: ty.clone(), span: interp.span, def_id: None };
    let Some(stmts) = stmts else { return Some((head, piece)) };
    let kind = IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(piece)) };
    let block = IrExpr { kind, ty, span: value.span, def_id: None };
    (!almide_ir::free_vars::free_vars(&block, &Default::default()).contains(&root)).then_some((head, block))
}
