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
                writeln!(out, "let mut {}: {} = Default::default();", var(*dst), rec::struct_name(i)).unwrap();
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
            tys.insert(*d, NTy::I64); writeln!(out, "let mut {}: i64 = 1;", var(*d)).unwrap(); return Ok(());
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
                writeln!(out, "let mut {}: i64 = {base}.borrow().as_ref().copied().expect(\"initialized shared cell\");", var(*d)).unwrap();
            }
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), .. } if payload != NTy::I64 => {
                crate::render_native_bounds::access(slot); tys.insert(*d, payload);
                writeln!(out, "let mut {}: {} = {base}.borrow().as_ref().expect(\"initialized shared cell\").clone();", var(*d), rec::owned_ty(payload)).unwrap();
            }
            Op::CallFn { dst: Some(d), name, .. } if name == "__mg_take" => {
                crate::render_native_bounds::access(slot); tys.insert(*d, payload);
                writeln!(out, "let mut {}: {} = {base}.borrow_mut().take().expect(\"initialized shared cell\");", var(*d), rec::owned_ty(payload)).unwrap();
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
                    else if let Some(value) = crate::native_value_repr::tuple_copy(&var(src.base), got, payload) { value }
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
                writeln!(out, "*{base}.borrow_mut() = Some({value});").unwrap();
            }
            _ => return Err(wall("native: unsupported shared-cell operation")),
        }
        Ok(())
    })())
}
