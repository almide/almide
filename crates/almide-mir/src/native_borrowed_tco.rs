//! Constant-stack tail calls when the frame owns no heap values.
//! The original MIR/call witnesses remain unchanged. Native rendering replaces
//! a true tail call with simultaneous rebinding of borrowed parameters. There
//! is no allocation, release, or writable borrow to move across the jump.
use std::collections::{BTreeMap, BTreeSet};
use crate::{CallArg, MirFunction, Op, PrimKind, Repr, ValueId};
use crate::lower::LowerError;
use crate::render_native::{NTy, call_arg, var};

/// A value must flow through every enclosing join to the actual return.
/// Merely being the last expression of an inner arm is not tail position.
pub(crate) fn sites(f: &MirFunction) -> BTreeSet<usize> {
    if !f.owned_params.is_empty() || !f.call_moves.is_empty() || f.ret.is_none()
        || f.ops.iter().any(|op| !matches!(op,
            Op::ConstInt { .. } | Op::IntBinOp { .. } | Op::Borrow { .. }
            | Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. }
            | Op::Return { .. }
            | Op::CallFn { dst: Some(_), result: Some(Repr::Scalar { .. }), .. }
            | Op::Prim { kind: PrimKind::Handle | PrimKind::Load { .. }
                | PrimKind::LoadHandle | PrimKind::FloatCmp(_), .. })) {
        return BTreeSet::new();
    }
    let mut joins = BTreeMap::new();
    let mut stack = Vec::new();
    for (i, op) in f.ops.iter().enumerate() {
        match op {
            Op::IfThen { dst, .. } => stack.push((*dst, None)),
            Op::Else { .. } => {
                let Some((_, marker)) = stack.last_mut() else { return BTreeSet::new() };
                if marker.replace(i).is_some() { return BTreeSet::new(); }
            }
            Op::EndIf { .. } => {
                let Some((dst, marker)) = stack.pop() else { return BTreeSet::new() };
                if let (Some(dst), Some(marker)) = (dst, marker) {
                    joins.insert(marker, (i + 1, dst));
                    joins.insert(i, (i + 1, dst));
                }
            }
            _ => {}
        }
    }
    if !stack.is_empty() { return BTreeSet::new(); }
    f.ops.iter().enumerate().filter_map(|(i, op)| {
        let Op::CallFn { name, dst: Some(value), args, .. } = op else { return None; };
        if name != &f.name || args.len() != f.params.len()
            || !args.iter().any(|a| matches!(a, CallArg::Handle(_))) { return None; }
        let (mut at, mut value) = (i + 1, *value);
        loop {
            match f.ops.get(at) {
                None => return (f.ret == Some(value)).then_some(i),
                Some(Op::Return { val }) => return (*val == Some(value)).then_some(i),
                Some(Op::Else { val } | Op::EndIf { val }) if *val == Some(value) => {
                    (at, value) = *joins.get(&at)?;
                }
                _ => return None,
            }
        }
    }).collect()
}

pub(crate) fn begin(f: &MirFunction, tys: &BTreeMap<ValueId, NTy>, out: &mut String) -> Result<(), LowerError> {
    for p in &f.params {
        if !tys.get(&p.value).is_some_and(|t| borrowed_or_scalar(*t)) {
            return Err(LowerError::at(crate::native_closure_decl::source_span(&f.name),
                "native: borrowed tail loop has an owned or non-lane parameter"));
        }
        let v = var(p.value);
        out.push_str(&format!("    let mut {v} = {v};\n"));
    }
    out.push_str("    '__almd_borrow_tail: loop {\n");
    Ok(())
}

fn borrowed_or_scalar(t: NTy) -> bool {
    matches!(t, NTy::I64 | NTy::StrRef | NTy::VecRef | NTy::VecStrRef | NTy::RecRef(_))
}

pub(crate) fn render(f: &MirFunction, op: &Op, tys: &mut BTreeMap<ValueId, NTy>, out: &mut String, indent: usize) -> Result<(), LowerError> {
    let Op::CallFn { dst: Some(dst), args, .. } = op else { unreachable!() };
    let mut staged = Vec::new();
    for (p, arg) in f.params.iter().zip(args) {
        let (code, got) = call_arg(arg, tys)?;
        if !borrowed_or_scalar(got) || tys.get(&p.value) != Some(&got) {
            return Err(LowerError::at(crate::native_closure_decl::source_span(&f.name),
                "native: borrowed tail argument must preserve its exact reference type"));
        }
        staged.push(code);
    }
    let pad = "    ".repeat(indent);
    // `continue` has the uninhabited type: no fabricated return value enters a
    // join. Keep a typed binding so ordinary join rendering remains unchanged.
    out.push_str(&format!("{pad}let mut {}: i64 = {{\n", var(*dst)));
    for (i, code) in staged.iter().enumerate() {
        out.push_str(&format!("{pad}    let __tail_arg_{i} = {code};\n"));
    }
    for (i, p) in f.params.iter().enumerate() {
        out.push_str(&format!("{pad}    {} = __tail_arg_{i};\n", var(p.value)));
    }
    out.push_str(&format!("{pad}    continue '__almd_borrow_tail;\n{pad}}};\n"));
    tys.insert(*dst, NTy::I64);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn function() -> MirFunction {
        MirFunction { name: "walk".into(),
            params: vec![crate::MirParam { value: ValueId(0), repr: Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT } }],
            ops: vec![Op::ConstInt { dst: ValueId(1), value: 1 },
                Op::IfThen { cond: ValueId(1), dst: Some(ValueId(4)) },
                Op::CallFn { dst: Some(ValueId(2)), name: "walk".into(), args: vec![CallArg::Handle(ValueId(0))], result: Some(Repr::Scalar { width: crate::ScalarWidth::Double }) },
                Op::Else { val: Some(ValueId(2)) },
                Op::ConstInt { dst: ValueId(3), value: 0 },
                Op::EndIf { val: Some(ValueId(3)) }],
            ret: Some(ValueId(4)), ..Default::default() }
    }
    #[test]
    fn payload_tag_facts_stop_at_the_arm_boundary() {
        let mut f = function();
        let mut prefix = vec![
            Op::Prim { kind: PrimKind::Handle, dst: Some(ValueId(5)), args: vec![ValueId(0)] },
            Op::ConstInt { dst: ValueId(6), value: i64::from(crate::lower::layout::BLOCK_HEADER) },
            Op::IntBinOp { dst: ValueId(7), op: crate::IntOp::Add, a: ValueId(5), b: ValueId(6) },
            Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(ValueId(8)), args: vec![ValueId(7)] },
            Op::ConstInt { dst: ValueId(9), value: 1 },
            Op::IntBinOp { dst: ValueId(1), op: crate::IntOp::Eq, a: ValueId(8), b: ValueId(9) },
            Op::ConstInt { dst: ValueId(20), value: i64::from(crate::lower::layout::BLOCK_HEADER + crate::lower::layout::SLOT_SIZE) },
            Op::IntBinOp { dst: ValueId(21), op: crate::IntOp::Add, a: ValueId(5), b: ValueId(20) },
        ];
        prefix.push(f.ops[1].clone());
        prefix.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(ValueId(10)), args: vec![ValueId(21)] });
        prefix.extend(f.ops[2..4].iter().cloned());
        prefix.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(ValueId(11)), args: vec![ValueId(21)] });
        prefix.extend(f.ops[4..].iter().cloned());
        f.ops = prefix;
        assert_eq!(guarded_reads(&f), BTreeMap::from([(ValueId(10), 1)]));
        f.ops[5] = Op::ConstInt { dst: ValueId(1), value: 1 };
        assert!(guarded_reads(&f).is_empty(), "a truthy condition proves no payload tag");
    }
    #[test]
    fn only_actual_tail_calls_in_frames_without_owned_values_are_looped() {
        let f = function();
        assert_eq!(sites(&f), BTreeSet::from([2]));
        let mut later = f.clone();
        later.ops.push(Op::IntBinOp { dst: ValueId(5), op: crate::IntOp::Add, a: ValueId(4), b: ValueId(1) });
        later.ret = Some(ValueId(5));
        assert!(sites(&later).is_empty(), "an inner-arm yield is not a function tail");
        let mut owns = f.clone(); owns.owned_params.insert(ValueId(0));
        assert!(sites(&owns).is_empty());
        let mut allocates = f;
        allocates.ops.insert(0, Op::Alloc { dst: ValueId(9), repr: Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }, init: crate::Init::Str("owned".into()) });
        assert!(sites(&allocates).is_empty());
    }
}

/// Tag facts for readonly frames. A payload borrow in a proven variant arm
/// refers to the original input, never to the fallback Default temporary.
/// Equality between two tag reads carries the selected tag to both inputs.
pub(crate) fn guarded_reads(f: &MirFunction) -> BTreeMap<ValueId, u32> {
    if sites(f).is_empty() { return BTreeMap::new(); }
    let mut addresses: BTreeMap<_, _> = f.params.iter().map(|p| (p.value, (p.value, 0i64))).collect();
    let mut constants = BTreeMap::new();
    let mut tags = BTreeMap::new();
    let mut comparisons = BTreeMap::new();
    let mut equalities = Vec::new();
    let mut scopes = Vec::new();
    let mut reads = BTreeMap::new();
    for op in &f.ops {
        match op {
            Op::ConstInt { dst, value } => { constants.insert(*dst, *value); }
            Op::IntBinOp { dst, op: crate::IntOp::Eq, a, b } => { comparisons.insert(*dst, (*a, *b)); }
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b } => {
                if let (Some((base, offset)), Some(k)) = (addresses.get(a), constants.get(b)) {
                    if let Some(offset) = offset.checked_add(*k) { addresses.insert(*dst, (*base, offset)); }
                }
            }
            Op::Prim { kind: PrimKind::Handle, dst: Some(dst), args } if args.len() == 1 => {
                let address = addresses.get(&args[0]).copied().unwrap_or((args[0], 0));
                addresses.insert(*dst, address);
            }
            Op::Prim { kind: PrimKind::Load { width: 8 }, dst: Some(dst), args } if args.len() == 1 => {
                if let Some((base, offset)) = addresses.get(&args[0]) {
                    if *offset == i64::from(crate::lower::layout::BLOCK_HEADER) { tags.insert(*base, *dst); }
                }
            }
            Op::IfThen { cond, .. } => {
                scopes.push(equalities.len());
                if let Some(eq) = comparisons.get(cond) { equalities.push(*eq); }
            }
            Op::Else { .. } => { if let Some(n) = scopes.last() { equalities.truncate(*n); } }
            Op::EndIf { .. } => { if let Some(n) = scopes.pop() { equalities.truncate(n); } }
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(dst), args } if args.len() == 1 => {
                if let Some((base, _)) = addresses.get(&args[0]) {
                    if let Some(tag) = tags.get(base) {
                        let mut connected = BTreeSet::from([*tag]);
                        loop {
                            let before = connected.len();
                            for (a, b) in &equalities {
                                if connected.contains(a) || connected.contains(b) { connected.insert(*a); connected.insert(*b); }
                            }
                            if connected.len() == before { break; }
                        }
                        let values: BTreeSet<_> = connected.iter().filter_map(|v| constants.get(v)).copied().collect();
                        if values.len() == 1 {
                            if let Ok(tag) = u32::try_from(*values.first().unwrap()) { reads.insert(*dst, tag); }
                        }
                    }
                }
                addresses.insert(*dst, (*dst, 0));
            }
            _ => {}
        }
    }
    reads
}
