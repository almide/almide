//! Shared captured variables retain one storage identity through closure clones.
//! Existing MIR ownership events still own/release the cell; only the native
//! spelling changes from a linear-memory slot to a typed Rc<RefCell<Option<T>>>.
use super::*;
use crate::render_native_rec as rec;

pub(super) fn render(op: &Op, st: &mut AddrState, tys: &mut BTreeMap<ValueId, NTy>, out: &mut String, indent: usize) -> Option<Result<bool, LowerError>> {
    let mut code = String::new();
    let result = inner(op, st, tys, &mut code)?;
    for line in code.lines() {
        out.push_str(&"    ".repeat(indent)); out.push_str(line); out.push('\n');
    }
    Some(result.map(|()| true))
}

fn inner(op: &Op, st: &mut AddrState, tys: &mut BTreeMap<ValueId, NTy>, out: &mut String) -> Option<Result<(), LowerError>> {
    if let Op::Alloc { dst, init: Init::DynList { len }, .. } = op {
        if let Some(i) = st.alloc_rec.get(dst).copied().filter(|i| rec::cell_inner(*i).is_some()) {
            return Some(if st.consts.get(len) != Some(&1) {
                Err(wall("native: a shared cell must have exactly one slot"))
            } else {
                tys.insert(*dst, NTy::Rec(i));
                writeln!(out, "let mut {}: {} = Default::default();", var(*dst), rec::struct_name(i)).expect("writing to a String cannot fail");
                Ok(())
            });
        }
    }
    let addr = match op {
        Op::Prim { args, .. } => args.first(),
        Op::CallFn { name, args, .. } if name == "__mg_take" => match args.as_slice() {
            [crate::CallArg::Scalar(a)] => Some(a), _ => None,
        },
        _ => None,
    }?;
    let ad = *st.addrs.get(addr)?;
    let (NTy::Rec(i) | NTy::RecRef(i)) = ty(tys, &ad.base)? else { return None };
    let payload = rec::cell_inner(i)?;
    Some((|| {
        if matches!(op, Op::Prim { kind: PrimKind::Load { width: 4 }, .. }) && ad.off == i64::from(BLOCK_LEN_OFFSET) && ad.idx.is_none() {
            let Op::Prim { dst: Some(d), .. } = op else { return Err(wall("native: a cell length read without a destination")) };
            tys.insert(*d, NTy::I64); writeln!(out, "let mut {}: i64 = 1;", var(*d)).expect("writing to a String cannot fail"); return Ok(());
        }
        if rec_slot(&ad) != Some(0) { return Err(wall("native: shared cell access outside its sole fixed slot")); }
        let slot = bounds_slot(st, &ad);
        // Both the declaration and allocation fix the extent to one. This
        // check remains explicit in the witness even for a statically safe slot.
        crate::render_native_bounds::check(slot);
        let base = var(ad.base);
        match op {
            Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(d), .. } if payload == NTy::I64 => {
                crate::render_native_bounds::access(slot); tys.insert(*d, payload);
                writeln!(out, "let mut {}: i64 = {base}.borrow().as_ref().copied().expect(\"initialized shared cell\");", var(*d)).expect("writing to a String cannot fail");
            }
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), .. } if payload != NTy::I64 => {
                crate::render_native_bounds::access(slot);
                // A read the cell's own take follows with nothing between
                // that can look at the cell (native_cell_window): the payload
                // itself, not a copy of it.
                let takes = rec::has_default(payload) && crate::render_native_keyed::cell_read_takes(*d, (ad.base, payload), tys);
                // A read whose every use only borrows, with nothing up to its
                // last use that can look at the cell: a reference into the
                // cell, its guard released after that use.
                if let (false, true, Some((t, spelled, whole))) = (takes, st.borrow_reads.contains(d), borrowed_kind(payload)) {
                    let g = format!("{}_g", var(*d));
                    if crate::render_native_keyed::cell_read_borrows(*d, (ad.base, payload), tys, g.clone()) {
                        tys.insert(*d, t);
                        writeln!(out, "let {g} = {base}.borrow(); let mut {}: {spelled} = &(*{g}.as_ref().expect(\"initialized shared cell\")){whole};", var(*d)).expect("writing to a String cannot fail");
                        return Ok(());
                    }
                }
                let read = if takes { "borrow_mut().take()" } else { "borrow().as_ref()" };
                let copy = if read.ends_with("as_ref()") { ".clone()" } else { "" };
                tys.insert(*d, payload);
                writeln!(out, "let mut {}: {} = {base}.{read}.expect(\"initialized shared cell\"){copy};", var(*d), rec::owned_ty(payload)).expect("writing to a String cannot fail");
            }
            Op::CallFn { dst: Some(d), name, .. } if name == "__mg_take" => {
                crate::render_native_bounds::access(slot); tys.insert(*d, payload);
                // The read before it took the payload: nothing is left, and the
                // take's result is only released (native_cell_window::take_window).
                if crate::render_native_keyed::cell_emptied(*d) {
                    writeln!(out, "// {}: the payload the read before took", var(*d)).expect("writing to a String cannot fail");
                } else {
                    writeln!(out, "let mut {}: {} = {base}.borrow_mut().take().expect(\"initialized shared cell\");", var(*d), rec::owned_ty(payload)).expect("writing to a String cannot fail");
                }
            }
            Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args } if args.len() == 2 => {
                let rhs = args[1];
                let value = if payload == NTy::I64 {
                    env_slots::word(rhs, tys)?
                } else {
                    let src = st.addrs.get(&rhs).filter(|a| a.off == 0 && a.idx.is_none())
                        .ok_or_else(|| wall("native: a shared cell stores a partial or absent heap address"))?;
                    let got = ty(tys, &src.base).ok_or_else(|| wall("native: an untyped shared-cell payload"))?;
                    let empty = st.empty_lists.contains(&src.base) && (matches!(payload, NTy::Vec | NTy::VecStr)
                        || matches!(payload, NTy::Rec(i) if rec::is_empty_collection(i) || rec::is_bytes(payload)));
                    if empty { format!("<{}>::default()", rec::owned_ty(payload)) }
                    else { if !crate::native_value_repr::same_owned(got, payload) {
                        return Err(wall(format!("native: shared-cell payload {got:?} disagrees with its declared type {payload:?}")));
                    }
                    match got {
                        NTy::StrRef => format!("AlmideString::from({})", var(src.base)),
                        NTy::VecRef | NTy::VecStrRef => format!("{}.to_vec()", var(src.base)),
                        NTy::RecRef(_) => format!("(*{}).clone()", var(src.base)),
                        _ => var(src.base),
                    } }
                };
                crate::render_native_bounds::store(slot);
                writeln!(out, "*{base}.borrow_mut() = Some({value});").expect("writing to a String cannot fail");
            }
            _ => return Err(wall("native: unsupported shared-cell operation")),
        }
        Ok(())
    })())
}
