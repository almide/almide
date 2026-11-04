//! Concatenation chains built in one buffer (#4006, no-perf-loss rule).
//!
//! An interpolation `"a${x}b${n}"` lowers to a chain of string
//! concatenations, `((("a" + x) + "b") + int.to_string(n))`, each step's
//! result read only by the next step. Rendered one op at a time, every step
//! grows (and reallocates) the buffer the step before it built, and every
//! interpolated integer is first rendered to a string of its own. The
//! standard codegen formats the whole interpolation into one buffer
//! (`AlmideStr::from_fmt`). Here a chain of two or more steps renders the
//! same way: at its last step, one `AlmideString::from_fmt(format_args!(..))`
//! over the pieces — a literal piece written into the template, an
//! `int.to_string` piece passed as the integer, any other piece as the
//! string value — and its earlier steps, and the literal and integer pieces'
//! own ops, render nothing.
//!
//! A chain is taken only when moving the read of every piece to the last step
//! leaves it the same value: a piece's other reads all come after the last
//! step, nothing rebinds it before then, and the integer under an
//! `int.to_string` piece is not rebound either. A last step that is a
//! self-append (`s = s + …`, `render_native_append`) is left to that window.

use crate::{CallArg, Init, MirFunction, Op, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// One piece of a chain.
#[derive(Clone, Debug)]
pub(crate) enum Piece {
    /// A literal, written into the template.
    Lit(String),
    /// An integer formatted in place (`int.to_string`'s text).
    Int(ValueId),
    /// A Bool formatted in place (`bool.to_string`'s text).
    Bool(ValueId),
    /// A string value.
    Str(ValueId),
}

/// The chains of one function: the ops that render nothing, and each last
/// step's pieces.
#[derive(Default)]
pub(crate) struct Chains {
    pub skip: BTreeSet<usize>,
    pub finals: BTreeMap<usize, (ValueId, Vec<Piece>)>,
}

fn concat_of(op: &Op) -> Option<(ValueId, ValueId, ValueId)> {
    match op {
        Op::CallFn { dst: Some(d), name, args, .. } if name == "__str_concat" => match args.as_slice() {
            [CallArg::Handle(a), CallArg::Handle(b)] => Some((*d, *a, *b)),
            _ => None,
        },
        _ => None,
    }
}

/// Every non-bookkeeping op each value occurs in (its definition included).
fn occurrences(ops: &[Op]) -> BTreeMap<ValueId, Vec<usize>> {
    let mut occurs: BTreeMap<ValueId, Vec<usize>> = BTreeMap::new();
    let mut vals = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        if crate::render_native_move::bookkeeping(op) {
            continue;
        }
        vals.clear();
        crate::mir_ops::op_values(op, &mut vals);
        for v in &vals {
            let at = occurs.entry(*v).or_default();
            if at.last() != Some(&i) {
                at.push(i);
            }
        }
    }
    occurs
}

/// What [`chains`] reads while it walks one function.
struct ChainScan<'a> {
    func: &'a MirFunction,
    ops: &'a [Op],
    occurs: BTreeMap<ValueId, Vec<usize>>,
    concats: BTreeMap<usize, (ValueId, ValueId, ValueId)>,
}

impl ChainScan<'_> {
    fn def_at(&self, v: ValueId) -> Option<usize> {
        self.occurs.get(&v).and_then(|a| a.first().copied())
    }

    fn rebound(&self, v: ValueId, from: usize, to: usize) -> bool {
        self.ops[from..=to].iter().any(|o| matches!(o, Op::SetLocal { local, .. } if *local == v))
    }

    /// A step whose result only the next step reads, as its left side.
    fn inner(&self, d: ValueId) -> Option<usize> {
        let at = self.occurs.get(&d)?;
        let [def, next] = at.as_slice() else { return None };
        let (_, a, b) = self.concats.get(next)?;
        (self.concats.contains_key(def) && *a == d && *b != d && self.func.ret != Some(d)).then_some(*next)
    }

    /// Walk the left sides back from `last`: the chain's steps (last first),
    /// and (step index, piece value) per piece, left to right.
    fn walk_back(&self, last: usize) -> (Vec<usize>, Vec<(usize, ValueId)>) {
        let mut steps = vec![last];
        let mut pieces: Vec<(usize, ValueId)> = Vec::new();
        let mut at = last;
        loop {
            let (_, a, b) = self.concats[&at];
            pieces.push((at, b));
            match self.def_at(a).filter(|d| self.concats.contains_key(d) && self.inner(a) == Some(at)) {
                Some(prev) => {
                    steps.push(prev);
                    at = prev;
                }
                None => {
                    pieces.push((at, a));
                    break;
                }
            }
        }
        pieces.reverse();
        (steps, pieces)
    }

    /// Each piece as it renders, pushing the ops that render nothing onto
    /// `skip`. `None` = the chain is not taken.
    fn render_pieces(&self, pieces: &[(usize, ValueId)], last: usize, skip: &mut Vec<usize>) -> Option<Vec<Piece>> {
        let mut rendered = Vec::with_capacity(pieces.len());
        for &(step, p) in pieces {
            rendered.push(self.render_piece(step, p, last, skip)?);
        }
        Some(rendered)
    }

    fn render_piece(&self, step: usize, p: ValueId, last: usize, skip: &mut Vec<usize>) -> Option<Piece> {
        let ops = self.ops;
        let uses = self.occurs.get(&p).map(Vec::as_slice).unwrap_or(&[]);
        let def = self.def_at(p);
        // Read only at its own step (besides its definition)?
        let temp = uses.iter().all(|u| Some(*u) == def || *u == step);
        match def.map(|d| &ops[d]) {
            Some(Op::Alloc { init: Init::Str(s), .. }) if temp => {
                skip.push(def.expect("defined"));
                Some(Piece::Lit(s.clone()))
            }
            Some(Op::CallFn { name, args, .. })
                if temp
                    && crate::native_floor::declared_scalar_text(name).is_some()
                    && matches!(args.as_slice(), [CallArg::Scalar(_)]) =>
            {
                let [CallArg::Scalar(n)] = args.as_slice() else { unreachable!() };
                if self.rebound(*n, def.expect("defined"), last) {
                    return None;
                }
                skip.push(def.expect("defined"));
                Some(match crate::native_floor::declared_scalar_text(name) {
                    Some(crate::native_floor::ScalarText::Bool) => Piece::Bool(*n),
                    _ => Piece::Int(*n),
                })
            }
            _ => {
                // Read again at the last step: its other reads must come
                // after it, and nothing may rebind it before then.
                if uses.iter().any(|u| Some(*u) != def && *u != step && *u <= last) || self.rebound(p, step, last) {
                    return None;
                }
                Some(Piece::Str(p))
            }
        }
    }
}

pub(crate) fn chains(func: &MirFunction) -> Chains {
    let ops = &func.ops;
    let concats: BTreeMap<usize, (ValueId, ValueId, ValueId)> =
        ops.iter().enumerate().filter_map(|(i, o)| concat_of(o).map(|c| (i, c))).collect();
    let scan = ChainScan { func, ops, occurs: occurrences(ops), concats };
    let mut out = Chains::default();
    for (&last, &(dst, _, _)) in &scan.concats {
        if scan.inner(dst).is_some() || crate::render_native_append::self_append_window(func, last).is_some() {
            continue;
        }
        let (steps, pieces) = scan.walk_back(last);
        let mut skip: Vec<usize> = steps[1..].to_vec();
        let Some(rendered) = scan.render_pieces(&pieces, last, &mut skip) else { continue };
        // One step is a chain too where it is printed (straight into
        // `println!`, no string built). Built into a string, one step stays a
        // concatenation: `from_fmt` costs more than formatting one integer.
        if steps.len() < 2 && printed_by(func, last, dst).is_none() {
            continue;
        }
        out.skip.extend(skip);
        out.finals.insert(last, (dst, rendered));
    }
    out
}

/// The `let` the last step of a chain renders as.
pub(crate) fn render_final(dst: ValueId, pieces: &[Piece], tys: &BTreeMap<ValueId, crate::render_native::NTy>) -> Result<String, crate::lower::LowerError> {
    render_final_opt(dst, pieces, tys).ok_or_else(|| crate::render_native::wall("native: a concatenation chain over a piece that is not a string or an Int"))
}

/// The chain's last step printed by the very next op and read by nothing
/// else (#4006, no-perf-loss): one `println!` over the pieces, as the
/// standard codegen prints an interpolation, with no string built for it.
pub(crate) fn printed_by(func: &MirFunction, at: usize, dst: ValueId) -> Option<usize> {
    let ops = &func.ops;
    let j = (at + 1..ops.len()).find(|&j| !crate::render_native_move::bookkeeping(&ops[j]))?;
    let prints = matches!(&ops[j], Op::Call { func: crate::RtFn::PrintStr, args, dst: None, .. }
        if matches!(args.as_slice(), [CallArg::Handle(v)] if *v == dst));
    let mut vals = Vec::new();
    let other_use = ops.iter().enumerate().any(|(k, op)| {
        if k == at || k == j || crate::render_native_move::bookkeeping(op) {
            return false;
        }
        vals.clear();
        crate::mir_ops::op_values(op, &mut vals);
        vals.contains(&dst)
    });
    (prints && !other_use && func.ret != Some(dst)).then_some(j)
}

/// The `println!` a printed chain renders as.
pub(crate) fn render_print(pieces: &[Piece], tys: &BTreeMap<ValueId, crate::render_native::NTy>) -> Option<String> {
    let (template, args) = fmt_parts(pieces, tys)?;
    Some(format!("rt_println_fmt(format_args!(\"{template}\"{args}));"))
}

fn render_final_opt(dst: ValueId, pieces: &[Piece], tys: &BTreeMap<ValueId, crate::render_native::NTy>) -> Option<String> {
    use crate::render_native::NTy;
    // Strings only: copied into one exact-size block, no formatter.
    if pieces.iter().all(|p| matches!(p, Piece::Lit(_) | Piece::Str(_))) {
        let parts = pieces
            .iter()
            .map(|p| match p {
                Piece::Lit(s) => Some(format!("\"{}\"", almide_base::rust_lit::escape_rust_str(s))),
                Piece::Str(v) => {
                    let t = *tys.get(v)?;
                    let code = crate::render_native::var(*v);
                    match t {
                        NTy::Str => Some(crate::render_native::as_str_arg(&code, t).strip_prefix('&').map_or_else(
                            || crate::render_native::as_str_arg(&code, t),
                            |c| format!("{c}.as_str()"),
                        )),
                        NTy::StrRef => Some(code),
                        _ => None,
                    }
                }
                _ => None,
            })
            .collect::<Option<Vec<String>>>()?;
        return Some(format!(
            "let mut {}: AlmideString = AlmideString::join::<&str>(&[{}], \"\");",
            crate::render_native::var(dst),
            parts.join(", ")
        ));
    }
    let (template, args) = fmt_parts(pieces, tys)?;
    Some(format!("let mut {}: AlmideString = AlmideString::from_fmt(format_args!(\"{template}\"{args}));", crate::render_native::var(dst)))
}

fn fmt_parts(pieces: &[Piece], tys: &BTreeMap<ValueId, crate::render_native::NTy>) -> Option<(String, String)> {
    use crate::render_native::{var, NTy};
    let mut template = String::new();
    let mut args = Vec::new();
    for p in pieces {
        match p {
            Piece::Lit(s) => template.push_str(&almide_base::rust_lit::escape_rust_fmt_str(s)),
            Piece::Int(n) => {
                if tys.get(n) != Some(&NTy::I64) {
                    return None;
                }
                template.push_str("{}");
                // By value (a copy in a block): a reference would pin the
                // integer's local to memory for the rest of the function.
                args.push(format!("{{ {} }}", var(*n)));
            }
            Piece::Bool(n) => {
                if tys.get(n) != Some(&NTy::I64) {
                    return None;
                }
                template.push_str("{}");
                args.push(format!("if {} != 0 {{ \"true\" }} else {{ \"false\" }}", var(*n)));
            }
            Piece::Str(v) => {
                let t = *tys.get(v)?;
                if !matches!(t, NTy::Str | NTy::StrRef) {
                    return None;
                }
                template.push_str("{}");
                args.push(crate::render_native::as_str_arg(&var(*v), t));
            }
        }
    }
    let args = if args.is_empty() { String::new() } else { format!(", {}", args.join(", ")) };
    Some((template, args))
}

/// A number or Bool rendered to a string only to be printed (#4006,
/// no-perf-loss): `println(int.to_string(n))`. The print formats the scalar
/// itself, as the standard codegen does, and the string is never built.
/// Returns the definitions that render nothing and, per print op, the
/// scalar and whether it is a Bool.
pub(crate) fn printed_scalars(func: &MirFunction) -> (BTreeSet<usize>, BTreeMap<usize, (ValueId, bool)>) {
    let ops = &func.ops;
    let mut occurs: BTreeMap<ValueId, Vec<usize>> = BTreeMap::new();
    let mut vals = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        if crate::render_native_move::bookkeeping(op) {
            continue;
        }
        vals.clear();
        crate::mir_ops::op_values(op, &mut vals);
        for v in &vals {
            occurs.entry(*v).or_default().push(i);
        }
    }
    let mut skip = BTreeSet::new();
    let mut prints = BTreeMap::new();
    for (j, op) in ops.iter().enumerate() {
        let Op::Call { func: crate::RtFn::PrintStr, args, dst: None, .. } = op else { continue };
        let [CallArg::Handle(v)] = args.as_slice() else { continue };
        let Some(at) = occurs.get(v) else { continue };
        let [d, p] = at.as_slice() else { continue };
        if *p != j || func.ret == Some(*v) {
            continue;
        }
        let Op::CallFn { dst: Some(dv), name, args: dargs, .. } = &ops[*d] else { continue };
        let [CallArg::Scalar(n)] = dargs.as_slice() else { continue };
        let Some(text) = crate::native_floor::declared_scalar_text(name) else { continue };
        let is_bool = text == crate::native_floor::ScalarText::Bool;
        if dv != v {
            continue;
        }
        // The scalar keeps its value from the definition to the print, which
        // sits in the same block: no rebind, no block boundary between.
        let between = &ops[*d..=j];
        let rebound = between.iter().any(|o| matches!(o, Op::SetLocal { local, .. } if local == n));
        let crosses = between.iter().any(|o| {
            matches!(o, Op::IfThen { .. } | Op::Else { .. } | Op::EndIf { .. } | Op::LoopStart | Op::LoopEnd | Op::LoopBreakUnless { .. } | Op::Return { .. })
        });
        if !rebound && !crosses {
            skip.insert(*d);
            prints.insert(j, (*n, is_bool));
        }
    }
    (skip, prints)
}
