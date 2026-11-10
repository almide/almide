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
            Op::ConstInt { .. } | Op::IntBinOp { .. } | Op::Borrow { .. } | Op::ListGetScalar { .. }
            | Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. }
            | Op::Return { .. }
            | Op::CallFn { dst: Some(_), result: Some(Repr::Scalar { .. }), .. }
            | Op::Prim { kind: PrimKind::Handle | PrimKind::Load { .. }
                | PrimKind::LoadHandle | PrimKind::FloatCmp(_), .. })) {
        return BTreeSet::new();
    }
    tail_calls(f)
}

/// Self calls whose result reaches the function return through all joins.
/// This identifies tail position independently of whether the frame permits
/// borrowed-parameter loop conversion. Releases remain a conversion barrier,
/// but do not make an otherwise terminal call into non-tail computation.
pub(crate) fn tail_calls(f: &MirFunction) -> BTreeSet<usize> {
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
                Some(op) if crate::render_native_move::bookkeeping(op) => { at += 1; }
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
        assert_eq!(guarded_reads(&f, &BTreeMap::new()), BTreeMap::from([(ValueId(10), 1)]));
        let mut scalar_tag = f.clone();
        scalar_tag.ops[1] = Op::ConstInt { dst: ValueId(6), value: 0 };
        scalar_tag.ops[3] = Op::ListGetScalar { dst: ValueId(8), list: ValueId(0), idx: ValueId(6) };
        assert_eq!(guarded_reads(&scalar_tag, &BTreeMap::new()), BTreeMap::from([(ValueId(10), 1)]));
        scalar_tag.ops[1] = Op::ConstInt { dst: ValueId(6), value: 1 };
        assert!(guarded_reads(&scalar_tag, &BTreeMap::new()).is_empty(), "a payload read proves no variant tag");
        f.ops[5] = Op::ConstInt { dst: ValueId(1), value: 1 };
        assert!(guarded_reads(&f, &BTreeMap::new()).is_empty(), "a truthy condition proves no payload tag");
    }
    #[test]
    fn only_actual_tail_calls_in_frames_without_owned_values_are_looped() {
        let f = function();
        assert_eq!(sites(&f), BTreeSet::from([2]));
        let mut later = f.clone();
        later.ops.push(Op::IntBinOp { dst: ValueId(5), op: crate::IntOp::Add, a: ValueId(4), b: ValueId(1) });
        later.ret = Some(ValueId(5));
        assert!(sites(&later).is_empty(), "an inner-arm yield is not a function tail");
        assert!(tail_calls(&later).is_empty(), "the refusal gate must follow the same joins");
        let mut owns = f.clone(); owns.owned_params.insert(ValueId(0));
        assert!(sites(&owns).is_empty());
        assert_eq!(tail_calls(&owns), BTreeSet::from([2]), "owned tail calls still need conversion");
        let mut allocates = f;
        allocates.ops.insert(0, Op::Alloc { dst: ValueId(9), repr: Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT }, init: crate::Init::Str("owned".into()) });
        assert!(sites(&allocates).is_empty());
    }

    #[test]
    fn releases_do_not_hide_tail_calls_but_later_computation_does() {
        let mut f = function();
        f.ops.insert(3, Op::Drop { v: ValueId(0) });
        assert_eq!(tail_calls(&f), BTreeSet::from([2]));
        assert!(sites(&f).is_empty());
        f.ops.push(Op::IntBinOp { dst: ValueId(5), op: crate::IntOp::Add,
            a: ValueId(4), b: ValueId(1) });
        f.ret = Some(ValueId(5));
        assert!(tail_calls(&f).is_empty());
    }

    #[test]
    fn default_arm_needs_one_remaining_declared_tag() {
        let v = ValueId;
        let mut f = function();
        f.ops = vec![
            Op::ConstInt { dst: v(1), value: 0 },
            Op::ListGetScalar { dst: v(2), list: v(0), idx: v(1) },
            Op::IntBinOp { dst: v(3), op: crate::IntOp::Eq, a: v(2), b: v(1) },
            Op::IfThen { cond: v(3), dst: Some(v(4)) },
            Op::Else { val: Some(v(1)) },
            Op::ConstInt { dst: v(5), value: 1 },
            Op::IntBinOp { dst: v(6), op: crate::IntOp::Eq, a: v(2), b: v(5) },
            Op::IfThen { cond: v(6), dst: Some(v(7)) },
            Op::Else { val: Some(v(1)) },
            Op::Prim { kind: PrimKind::Handle, dst: Some(v(8)), args: vec![v(0)] },
            Op::ConstInt { dst: v(9), value: i64::from(crate::lower::layout::BLOCK_HEADER + crate::lower::layout::SLOT_SIZE) },
            Op::IntBinOp { dst: v(10), op: crate::IntOp::Add, a: v(8), b: v(9) },
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(v(11)), args: vec![v(10)] },
            Op::CallFn { dst: Some(v(12)), name: f.name.clone(), args: vec![CallArg::Handle(v(11))],
                result: Some(Repr::Scalar { width: crate::ScalarWidth::Double }) },
            Op::EndIf { val: Some(v(12)) },
            Op::EndIf { val: Some(v(7)) },
        ];
        f.ret = Some(v(4));
        let mut decl = crate::render_native_rec::RecDecl { name: "Choices".into(),
            cases: Some(vec![(0, vec![]), (1, vec![]), (2, vec![NTy::Rec(0)])]),
            ..Default::default() };
        let guard = crate::render_native_rec::install(vec![decl.clone()]);
        let records = BTreeMap::from([(v(0), 0)]);
        assert_eq!(guarded_reads(&f, &records), BTreeMap::from([(v(11), 2)]));
        drop(guard);
        decl.cases.as_mut().unwrap().push((3, vec![NTy::Rec(0)]));
        let _guard = crate::render_native_rec::install(vec![decl]);
        assert!(guarded_reads(&f, &records).is_empty(), "two remaining tags are not a checked case");
    }
}

/// Tag facts for readonly frames. A payload borrow in a proven variant arm
/// refers to the original input, never to the fallback Default temporary.
/// Equality between two tag reads carries the selected tag to both inputs.
pub(crate) fn guarded_reads(f: &MirFunction, records: &BTreeMap<ValueId, u16>) -> BTreeMap<ValueId, u32> {
    if sites(f).is_empty() { return BTreeMap::new(); }
    let mut facts = TagFacts {
        addresses: f.params.iter().map(|p| (p.value, (p.value, 0i64))).collect(),
        ..TagFacts::default()
    };
    for op in &f.ops {
        facts.step(op, records);
    }
    facts.reads
}

/// What one forward walk of a frame knows at the current op.
#[derive(Default)]
struct TagFacts {
    /// A value as (base block, byte offset).
    addresses: BTreeMap<ValueId, (ValueId, i64)>,
    constants: BTreeMap<ValueId, i64>,
    /// A block's tag read.
    tags: BTreeMap<ValueId, ValueId>,
    /// An equality test's two operands, by its result.
    comparisons: BTreeMap<ValueId, (ValueId, ValueId)>,
    /// The tests that hold, and those that failed, in the open scopes.
    equalities: Vec<(ValueId, ValueId)>,
    inequalities: Vec<(ValueId, ValueId)>,
    /// Per open `if`: both lists' lengths at entry, and its own test.
    scopes: Vec<(usize, usize, Option<(ValueId, ValueId)>)>,
    /// A payload borrow and the one tag its block holds there.
    reads: BTreeMap<ValueId, u32>,
}

impl TagFacts {
    fn step(&mut self, op: &Op, records: &BTreeMap<ValueId, u16>) {
        match op {
            Op::ConstInt { dst, value } => { self.constants.insert(*dst, *value); }
            Op::IntBinOp { dst, op: crate::IntOp::Eq, a, b } => { self.comparisons.insert(*dst, (*a, *b)); }
            Op::IntBinOp { dst, op: crate::IntOp::Add, a, b } => self.add(*dst, *a, *b),
            Op::Prim { kind, dst: Some(dst), args } if args.len() == 1 => self.prim(kind, *dst, args[0], records),
            Op::ListGetScalar { dst, list, idx } if self.constants.get(idx) == Some(&0) => {
                self.tags.insert(*list, *dst);
            }
            Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } => self.scope(op),
            _ => {}
        }
    }

    fn add(&mut self, dst: ValueId, a: ValueId, b: ValueId) {
        let (Some(&(base, offset)), Some(&k)) = (self.addresses.get(&a), self.constants.get(&b)) else { return };
        if let Some(offset) = offset.checked_add(k) { self.addresses.insert(dst, (base, offset)); }
    }

    fn prim(&mut self, kind: &PrimKind, dst: ValueId, arg: ValueId, records: &BTreeMap<ValueId, u16>) {
        match kind {
            PrimKind::Handle => {
                let address = self.addresses.get(&arg).copied().unwrap_or((arg, 0));
                self.addresses.insert(dst, address);
            }
            PrimKind::Load { width: 8 } => {
                if let Some(&(base, offset)) = self.addresses.get(&arg) {
                    if offset == i64::from(crate::lower::layout::BLOCK_HEADER) { self.tags.insert(base, dst); }
                }
            }
            PrimKind::LoadHandle => {
                if let Some(tag) = self.proven_tag(arg, records) { self.reads.insert(dst, tag); }
                self.addresses.insert(dst, (dst, 0));
            }
            _ => {}
        }
    }

    fn scope(&mut self, op: &Op) {
        match op {
            Op::IfThen { cond, .. } => {
                let eq = self.comparisons.get(cond).copied();
                self.scopes.push((self.equalities.len(), self.inequalities.len(), eq));
                if let Some(eq) = eq { self.equalities.push(eq); }
            }
            Op::Else { .. } => {
                if let Some(&(yes, no, eq)) = self.scopes.last() {
                    self.equalities.truncate(yes);
                    self.inequalities.truncate(no);
                    if let Some(eq) = eq { self.inequalities.push(eq); }
                }
            }
            Op::EndIf { .. } => {
                if let Some((yes, no, _)) = self.scopes.pop() {
                    self.equalities.truncate(yes);
                    self.inequalities.truncate(no);
                }
            }
            _ => {}
        }
    }

    /// The one tag the block read through `address` holds here, when proven.
    fn proven_tag(&self, address: ValueId, records: &BTreeMap<ValueId, u16>) -> Option<u32> {
        let (base, _) = self.addresses.get(&address)?;
        let connected = self.connected(*self.tags.get(base)?);
        let mut values: BTreeSet<i64> = connected.iter().filter_map(|v| self.constants.get(v)).copied().collect();
        // A final default arm has no equality test. Its tag
        // follows only when the declared enum's entire set,
        // minus the preceding failed tests, leaves one case.
        if values.is_empty() {
            values = self.remaining_cases(*base, &connected, records);
        }
        let mut values = values.into_iter();
        match (values.next(), values.next()) {
            (Some(only), None) => u32::try_from(only).ok(),
            _ => None,
        }
    }

    /// Every value the holding tests equate with `tag`.
    fn connected(&self, tag: ValueId) -> BTreeSet<ValueId> {
        let mut connected = BTreeSet::from([tag]);
        loop {
            let before = connected.len();
            for (a, b) in &self.equalities {
                if connected.contains(a) || connected.contains(b) { connected.insert(*a); connected.insert(*b); }
            }
            if connected.len() == before { return connected; }
        }
    }

    /// The declared enum's tags minus the ones a failed test ruled out.
    fn remaining_cases(&self, base: ValueId, connected: &BTreeSet<ValueId>, records: &BTreeMap<ValueId, u16>) -> BTreeSet<i64> {
        let Some(cases) = records.get(&base).and_then(|i| crate::render_native_rec::enum_cases(*i)) else {
            return BTreeSet::new();
        };
        let mut values: BTreeSet<i64> = cases.into_iter().map(|(tag, _)| i64::from(tag)).collect();
        for (a, b) in &self.inequalities {
            for (seen, other) in [(a, b), (b, a)] {
                if connected.contains(seen) {
                    if let Some(k) = self.constants.get(other) { values.remove(k); }
                }
            }
        }
        values
    }
}
