//! Typed closure slots in the existing symbolic address algebra. The MIR's
//! ownership events and physical slot indices stay unchanged on both targets.
use super::*;
use crate::render_native_rec as rec;

pub(super) fn render(op: &Op, st: &mut AddrState, tys: &mut BTreeMap<ValueId, NTy>, out: &mut String, indent: usize) -> Option<Result<bool, LowerError>> {
    let mut code = String::new();
    let result = render_inner(op, st, tys, &mut code)?;
    for line in code.lines() {
        for _ in 0..indent { out.push_str("    "); }
        out.push_str(line);
        out.push('\n');
    }
    Some(result.map(|()| true))
}

fn render_inner(op: &Op, st: &mut AddrState, tys: &mut BTreeMap<ValueId, NTy>, out: &mut String) -> Option<Result<(), LowerError>> {
    let allocation = match op {
        Op::Alloc { dst, init: Init::DynList { len }, .. } => Some((*dst, Some(*len), None)),
        Op::ListLit { dst, elems } => Some((*dst, None, Some(elems))),
        _ => None,
    };
    if let Some((dst, len, elems)) = allocation {
        if let Some(i) = st.alloc_rec.get(&dst).copied().filter(|i| rec::closure_sig(*i).is_some()) {
            return Some((|| {
                tys.insert(dst, NTy::Rec(i));
                if let Some(elems) = elems {
                    let words: Vec<String> = elems.iter().map(|v| word(*v, tys)).collect::<Result<_, _>>()?;
                    writeln!(out, "let mut {}: {} = __AlmdClosure {{ words: vec![{}], heaps: Vec::new() }};", var(dst), rec::struct_name(i), words.join(", ")).unwrap();
                } else {
                    let len = len.unwrap();
                    if ty(tys, &len) != Some(NTy::I64) { return Err(wall("native: untyped closure length")); }
                    writeln!(out, "let mut {}: {} = __AlmdClosure::new({}.max(0) as usize);", var(dst), rec::struct_name(i), var(len)).unwrap();
                }
                Ok(())
            })());
        }
    }
    if let Op::ListGetScalar { dst, list, idx } = op {
        if ty(tys, list).is_some_and(rec::is_closure) {
            let slot = crate::render_native_bounds::Slot { base: *list, idx: Some(*idx), k: 0 };
            crate::render_native_bounds::check(slot);
            crate::render_native_bounds::access(slot);
            tys.insert(*dst, NTy::I64);
            writeln!(out, "if {} as usize >= {}.len() {{ eprintln!(\"Error: index out of bounds\"); std::process::exit(1); }}", var(*idx), var(*list)).unwrap();
            writeln!(out, "let mut {}: i64 = {}.words()[{} as usize];", var(*dst), var(*list), var(*idx)).unwrap();
            return Some(Ok(()));
        }
    }
    let Op::Prim { kind, dst, args } = op else { return None };
    let ad = *st.addrs.get(args.first()?)?;
    if !ty(tys, &ad.base).is_some_and(rec::is_closure) { return None; }
    Some((|| {
        if matches!(kind, PrimKind::Load { width: 4 }) && ad.off == i64::from(BLOCK_LEN_OFFSET) && ad.idx.is_none() {
            let dst = dst.ok_or_else(|| wall("native: closure length read without destination"))?;
            tys.insert(dst, NTy::I64);
            writeln!(out, "let mut {}: i64 = {}.len() as i64;", var(dst), var(ad.base)).unwrap();
            return Ok(());
        }
        let k = rec_slot(&ad).ok_or_else(|| wall("native: closure access outside a fixed capture slot"))?;
        let (guard, slot) = bounds_guard(st, &ad, &format!("{k}usize"));
        if let Some(guard) = guard { writeln!(out, "{guard}").unwrap(); }
        match (kind, dst, args.as_slice()) {
            (PrimKind::Load { width: 8 }, Some(d), [_]) => {
                crate::render_native_bounds::access(slot);
                tys.insert(*d, NTy::I64);
                writeln!(out, "let mut {}: i64 = {}.words()[{k}];", var(*d), var(ad.base)).unwrap();
            }
            (PrimKind::LoadHandle, Some(d), [_]) => {
                let current = crate::native_hof::current_fn().ok_or_else(|| wall("native: missing closure context"))?;
                let fields = rec::closure_slots(&current).ok_or_else(|| wall("native: a closure capture without a checked native layout"))?;
                let t = *fields.get(k).filter(|t| **t != NTy::I64).ok_or_else(|| wall("native: a heap closure read of a scalar or absent capture"))?;
                let variant = rec::capture_variant(t);
                crate::render_native_bounds::access(slot);
                let place = format!("match {}.heap({k}) {{ Some(__AlmdCapture::{variant}(x)) => x, _ => {{ eprintln!(\"Error: closure capture kind mismatch\"); std::process::exit(1) }} }}", var(ad.base));
                if let Some((borrowed, spelled, suffix)) = borrowed_kind(t) {
                    tys.insert(*d, borrowed);
                    writeln!(out, "let mut {}: {spelled} = &(*({place})){suffix};", var(*d)).unwrap();
                } else {
                    tys.insert(*d, t);
                    writeln!(out, "let mut {}: {} = ({place}).clone();", var(*d), rec::owned_ty(t)).unwrap();
                }
            }
            (PrimKind::Store { width: 8 }, None, [_, rhs]) => {
                if matches!(ty(tys, &ad.base), Some(NTy::RecRef(_))) { return Err(wall("native: write into a borrowed closure environment")); }
                crate::render_native_bounds::store(slot);
                if let Some(src) = st.addrs.get(rhs) {
                    if src.off != 0 || src.idx.is_some() { return Err(wall("native: a closure stores a partial heap address")); }
                    let t = ty(tys, &src.base).ok_or_else(|| wall("native: an untyped closure capture"))?;
                    let variant = rec::capture_variant(t);
                    let value = match t {
                        NTy::StrRef => format!("AlmideString::from({})", var(src.base)),
                        NTy::VecRef | NTy::VecStrRef => format!("{}.to_vec()", var(src.base)),
                        NTy::RecRef(_) => format!("(*{}).clone()", var(src.base)),
                        _ => var(src.base),
                    };
                    writeln!(out, "{}.put({k}, __AlmdCapture::{variant}({value}));", var(ad.base)).unwrap();
                } else if k >= 2 && ty(tys, rhs) == Some(NTy::Opt) {
                    // Scalar-Option rewriting replaces Store(Handle(option))
                    // with Store(option): the native carrier owns no block.
                    // It still occupies a typed capture, never an i64 word.
                    let variant = rec::capture_variant(NTy::Opt);
                    writeln!(out, "{}.put({k}, __AlmdCapture::{variant}({}));", var(ad.base), var(*rhs)).unwrap();
                } else {
                    writeln!(out, "{}.words[{k}] = {};", var(ad.base), word(*rhs, tys)?).unwrap();
                }
            }
            _ => return Err(wall("native: unsupported typed closure slot operation")),
        }
        Ok(())
    })())
}

pub(super) fn word(v: ValueId, tys: &BTreeMap<ValueId, NTy>) -> Result<String, LowerError> {
    match ty(tys, &v) {
        Some(NTy::I64) => Ok(var(v)),
        Some(NTy::F64) => Ok(format!("{}.to_bits() as i64", var(v))),
        kind => Err(wall(format!("native: closure scalar slot {v:?} has non-scalar kind {kind:?}"))),
    }
}
