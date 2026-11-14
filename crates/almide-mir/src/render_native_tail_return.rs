//! A branch value that is the function's result returns where it is made
//! (#4661).
//!
//! A chain of propagations (`let a = f()!; let b = g()!; …; ok(R { a, b })`)
//! lowers to nested branches, the rest of the block in the arm that carries
//! on, and each branch yields into its join: the innermost value is assigned
//! outward once per nesting level before the function returns it. For a
//! derived decode of eight fields that is sixteen levels, each a copy of the
//! whole `Result` — 8 ns of a 92 ns decode — where the standard codegen's `?`
//! returns from the spot.
//!
//! A yield (`Else { val }` / `EndIf { val }`) is in TAIL POSITION when
//! nothing but releases stands between the end of its branch and either the
//! end of the body, with the join the function's result, or an enclosing
//! yield of that join which is itself in tail position ([`tail_yields`]).
//! Its rendered `join = value;` then becomes `return value;`: the value takes
//! the same path out it took through the joins, every local still drops at
//! the return, and the assignments it skipped only moved it.
//!
//! Taken only where the rendered function returns its result as the plain
//! local — an owned heap kind, every join on the way of that same kind, no
//! conversion at the return seam ([`finish`]). The decision needs the kinds
//! the whole body settles, so a yield is marked as it renders ([`mark`]) and
//! rewritten or left once the body is complete.

use crate::render_native::{var, NTy, NativeSigKind, NativeSigs};
use crate::{MirFunction, Op, ValueId};
use std::collections::BTreeMap;

thread_local! {
    /// The tail yields of the function being rendered: the yield's op index
    /// to the joins its value passes through, innermost first.
    static TAILS: std::cell::RefCell<BTreeMap<usize, Vec<ValueId>>> = const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// Find the tail yields of `func`.
pub(crate) fn begin(func: &MirFunction) {
    TAILS.with(|t| *t.borrow_mut() = tail_yields(func));
}

/// Each `IfThen` region of `func`: (start, else, end).
fn regions(func: &MirFunction) -> Vec<(usize, Option<usize>, usize)> {
    let mut open: Vec<(usize, Option<usize>)> = Vec::new();
    let mut out = Vec::new();
    for (i, op) in func.ops.iter().enumerate() {
        match op {
            Op::IfThen { .. } => open.push((i, None)),
            Op::Else { .. } => {
                if let Some(top) = open.last_mut() {
                    top.1 = Some(i);
                }
            }
            Op::EndIf { .. } => {
                if let Some((start, els)) = open.pop() {
                    out.push((start, els, i));
                }
            }
            _ => {}
        }
    }
    out
}

fn tail_yields(func: &MirFunction) -> BTreeMap<usize, Vec<ValueId>> {
    let regions = regions(func);
    // The joins a region's value passes through to the function's result.
    fn ladder(func: &MirFunction, regions: &[(usize, Option<usize>, usize)], at: usize) -> Option<Vec<ValueId>> {
        let (start, _, end) = regions[at];
        let Op::IfThen { dst: Some(join), .. } = func.ops[start] else { return None };
        let next = (end + 1..func.ops.len()).find(|&k| !crate::render_native_move::bookkeeping(&func.ops[k]));
        let Some(next) = next else { return (func.ret == Some(join)).then(|| vec![join]) };
        let (Op::Else { val: Some(v) } | Op::EndIf { val: Some(v) }) = func.ops[next] else { return None };
        if v != join {
            return None;
        }
        let outer = regions.iter().position(|(_, els, e)| *els == Some(next) || *e == next)?;
        let mut joins = vec![join];
        joins.extend(ladder(func, regions, outer)?);
        Some(joins)
    }
    let mut out = BTreeMap::new();
    for (at, (_, els, end)) in regions.iter().enumerate() {
        let Some(joins) = ladder(func, &regions, at) else { continue };
        for k in els.iter().chain([end]) {
            if matches!(func.ops[*k], Op::Else { val: Some(_) } | Op::EndIf { val: Some(_) }) {
                out.insert(*k, joins.clone());
            }
        }
    }
    out
}

fn marker(i: usize) -> String {
    format!(" //__TAIL_{i}__")
}

/// Mark the yield `ops[i]` rendered into `out[start..]`, when it is a tail
/// yield and rendered as one `join = value;` line.
pub(crate) fn mark(i: usize, out: &mut String, start: usize) {
    let Some(join) = TAILS.with(|t| t.borrow().get(&i).and_then(|joins| joins.first().copied())) else { return };
    let Some(text) = out.get(start..) else { return };
    let head = format!("{} = ", var(join));
    let mut at = start;
    let mut found = None;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        if body.trim_start().starts_with(&head) && body.ends_with(';') {
            found = Some(at + body.len());
        }
        at += line.len();
    }
    if let Some(end) = found {
        out.insert_str(end, &marker(i));
    }
}

/// Rewrite every marked yield of `func` to a `return`, or leave it, now that
/// the body's kinds are settled; no marker survives.
pub(crate) fn finish(func: &MirFunction, sigs: &NativeSigs, is_main: bool, tys: &BTreeMap<ValueId, NTy>, out: &mut String) {
    let tails = TAILS.with(|t| std::mem::take(&mut *t.borrow_mut()));
    // The result's kind, when the function returns its result local as it is.
    let plain = (!is_main && crate::native_borrowed_tco::sites(func).is_empty())
        .then(|| func.ret.and_then(|r| tys.get(&r).copied()))
        .flatten()
        .filter(|t| matches!(t, NTy::Str | NTy::Vec | NTy::VecStr | NTy::Res | NTy::ResStr | NTy::Rec(_)))
        .filter(|_| !matches!(sigs.get(func.name.as_str()), Some((_, Some(NativeSigKind::Never)))));
    for (i, joins) in tails {
        let mark = marker(i);
        let Some(at) = out.find(&mark) else { continue };
        let line_at = out[..at].rfind('\n').map_or(0, |n| n + 1);
        let same_kind = plain.is_some_and(|t| joins.iter().all(|j| tys.get(j) == Some(&t)));
        let stmt = out[line_at..at].to_string();
        let head = format!("{} = ", var(joins[0]));
        let returned = match (same_kind, stmt.trim_start().strip_prefix(&head)) {
            (true, Some(value)) => format!("{}return {value}", &stmt[..stmt.len() - stmt.trim_start().len()]),
            _ => stmt,
        };
        out.replace_range(line_at..at + mark.len(), &returned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(n: u32) -> ValueId {
        ValueId(n)
    }

    /// `if c { if c { a } else { b } } else { e }` with the outer join the result.
    fn nested(tail: Vec<Op>, ret: u32) -> MirFunction {
        let mut ops = vec![
            Op::IfThen { cond: v(0), dst: Some(v(9)) },
            Op::IfThen { cond: v(0), dst: Some(v(8)) },
            Op::Else { val: Some(v(1)) },
            Op::EndIf { val: Some(v(2)) },
            Op::Else { val: Some(v(8)) },
            Op::EndIf { val: Some(v(3)) },
        ];
        ops.extend(tail);
        MirFunction { name: "f".into(), ops, ret: Some(v(ret)), ..Default::default() }
    }

    #[test]
    fn a_yield_is_tail_only_when_its_join_reaches_the_result_through_releases() {
        let tails = tail_yields(&nested(vec![Op::Drop { v: v(5) }], 9));
        assert_eq!(tails.get(&2), Some(&vec![v(8), v(9)]));
        assert_eq!(tails.get(&3), Some(&vec![v(8), v(9)]));
        assert_eq!(tails.get(&4), Some(&vec![v(9)]));
        assert_eq!(tails.get(&5), Some(&vec![v(9)]));
        // Another value is the result, or an op reads the join first.
        assert!(tail_yields(&nested(Vec::new(), 7)).is_empty());
        let read = Op::Dup { dst: v(7), src: v(9) };
        assert!(tail_yields(&nested(vec![read], 7)).is_empty());
    }

    #[test]
    fn a_marked_yield_returns_only_when_every_join_has_the_result_kind() {
        let f = nested(Vec::new(), 9);
        let sigs = NativeSigs::default();
        let text = "    if c {\n        v8 = v1; //__TAIL_2__\n    }\n".to_string();
        for (kinds, expected) in [
            (vec![(v(8), NTy::Str), (v(9), NTy::Str)], "    if c {\n        return v1;\n    }\n"),
            (vec![(v(8), NTy::Str), (v(9), NTy::ResStr)], "    if c {\n        v8 = v1;\n    }\n"),
            (vec![(v(8), NTy::I64), (v(9), NTy::I64)], "    if c {\n        v8 = v1;\n    }\n"),
        ] {
            TAILS.with(|t| *t.borrow_mut() = tail_yields(&f));
            let mut out = text.clone();
            finish(&f, &sigs, false, &kinds.into_iter().collect(), &mut out);
            assert_eq!(out, expected);
        }
        // `main` reports its result at the seam: it never returns early.
        TAILS.with(|t| *t.borrow_mut() = tail_yields(&f));
        let mut out = text.clone();
        finish(&f, &sigs, true, &[(v(8), NTy::Str), (v(9), NTy::Str)].into_iter().collect(), &mut out);
        assert_eq!(out, "    if c {\n        v8 = v1;\n    }\n");
    }
}
