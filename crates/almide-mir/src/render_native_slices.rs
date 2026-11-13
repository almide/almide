//! String slices of a borrowed parameter (#4661).
//!
//! `string.split_once(line, ";")` returns `(String, String)?`, and its runtime
//! body builds both halves as owned strings: two blocks allocated, filled and
//! freed per call, where the halves are only ever read. A parse loop that
//! splits every line (`station;12.3`, then `12` / `3`) paid five such blocks
//! a line. The standard codegen binds the halves as slices of the subject
//! (`pass_slice_binders.rs`); here the certified render does the same.
//!
//! The runtime carries a slicing twin for such a body (`<symbol>_ref`,
//! [`crate::native_floor::slice_twin`]): the same params, the result the same
//! shape in `&str` borrowed from the params its signature ties the result to.
//! A call renders through the twin when:
//! * every argument the result borrows from is a string parameter the
//!   function borrows and never rebinds — a `&str` for the whole body, so a
//!   slice of it outlives every use;
//! * the Option it returns is only read: its tag word, its payload, and (a
//!   pair) the pair's two fields, each through an address into its block.
//!   Nothing copies, stores, returns or hands on the Option or the pair.
//!
//! A half whose own uses only borrow ([`borrow_reads`]) is then the slice
//! itself; any other is the one owned copy its use asks for, made from the
//! slice (the copy the runtime body made earlier). The MIR is unchanged: the
//! ownership certificate reads the same ops, and the blocks it accounts for
//! are the ones this render no longer builds.
//!
//! [`borrow_reads`]: crate::render_native_borrow_read::borrow_reads

use crate::lower::layout::{BLOCK_HEADER, BLOCK_LEN_OFFSET, SLOT_SIZE};
use crate::lower::LowerError;
use crate::render_native::{var, wall, NTy};
use crate::{CallArg, IntOp, MirFunction, Op, PrimKind, ValueId};
use std::collections::{BTreeMap, BTreeSet};

/// What this render does with one op of a slice view.
#[derive(Clone, Debug, PartialEq)]
enum Step {
    /// The call, through its slicing twin (its name, its items).
    Call { twin: String, pair: bool, items: Vec<&'static str> },
    /// An address into a view or a release of one: nothing to render.
    Nothing,
    /// The tag word of the Option.
    Tag(ValueId),
    /// The payload of the Option: the pair, or the one slice.
    Payload { of: ValueId, pair: bool },
    /// Field `k` of the pair.
    Field { of: ValueId, k: u32 },
}

thread_local! {
    /// The steps of the function being rendered, by op index, and the slice
    /// reads that stay slices.
    static PLAN: std::cell::RefCell<(BTreeMap<usize, Step>, BTreeSet<ValueId>)> =
        const { std::cell::RefCell::new((BTreeMap::new(), BTreeSet::new())) };
}

/// Plan the slice views of `func` (`param_tys`: its params' kinds).
pub(crate) fn begin(func: &MirFunction, param_tys: &BTreeMap<ValueId, NTy>) {
    let steps = plan(func, param_tys);
    let reads = if steps.is_empty() { BTreeSet::new() } else { crate::render_native_borrow_read::borrow_reads(func) };
    PLAN.with(|p| *p.borrow_mut() = (steps, reads));
}

fn plan(func: &MirFunction, param_tys: &BTreeMap<ValueId, NTy>) -> BTreeMap<usize, Step> {
    let mut steps = BTreeMap::new();
    let mut defs: BTreeMap<ValueId, usize> = BTreeMap::new();
    for op in &func.ops {
        if let Some(d) = crate::mir_ops::defined_value(op) {
            *defs.entry(d).or_default() += 1;
        }
    }
    let rebound: BTreeSet<ValueId> = func.ops.iter().filter_map(|op| if let Op::SetLocal { local, .. } = op { Some(*local) } else { None }).collect();
    // Bound once and never rebound: the value its one op gives.
    let fixed = |v: &ValueId| defs.get(v) == Some(&1) && !rebound.contains(v);
    let lent = |v: ValueId| {
        func.params.iter().any(|p| p.value == v)
            && !func.owned_params.contains(&v)
            && !rebound.contains(&v)
            && !defs.contains_key(&v)
            && param_tys.get(&v) == Some(&NTy::StrRef)
    };
    let consts: BTreeMap<ValueId, i64> = func
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::ConstInt { dst, value } if fixed(dst) => Some((*dst, *value)),
            _ => None,
        })
        .collect();
    for (i, op) in func.ops.iter().enumerate() {
        let Op::CallFn { dst: Some(o), name, args, .. } = op else { continue };
        // The runtime body the call's declaration names, on either route.
        let Some(symbol) = crate::native_floor::declared(name)
            .map(|f| f.symbol.as_str())
            .or_else(|| crate::native_hof::of(name).map(|h| h.symbol.as_str()))
        else {
            continue;
        };
        let Some((twin, pair, from, items)) = crate::native_floor::slice_twin(symbol) else { continue };
        let sources_lent = args.len() == from.len()
            && args.iter().zip(&from).all(|(a, borrowed_from)| matches!(a, CallArg::Handle(v) if !*borrowed_from || lent(*v)));
        if !sources_lent || !fixed(o) || func.ret == Some(*o) {
            continue;
        }
        if let Some(view) = view_steps(func, i, *o, pair, &consts, &fixed) {
            steps.insert(i, Step::Call { twin, pair, items });
            steps.extend(view);
        }
    }
    steps
}

/// The step of every op that names the Option `o` (bound at `ops[at]`), its
/// pair, or an address into either — `None` unless each only reads: an
/// address, the tag word, the payload, a field of the pair, or a release.
fn view_steps(
    func: &MirFunction,
    at: usize,
    o: ValueId,
    pair: bool,
    consts: &BTreeMap<ValueId, i64>,
    fixed: &dyn Fn(&ValueId) -> bool,
) -> Option<BTreeMap<usize, Step>> {
    let (tag, slot0, slot1) = (i64::from(BLOCK_LEN_OFFSET), i64::from(BLOCK_HEADER), i64::from(BLOCK_HEADER + SLOT_SIZE));
    let mut out = BTreeMap::new();
    // Each address value: the block it points into and the offset.
    let mut addr: BTreeMap<ValueId, (ValueId, i64)> = BTreeMap::new();
    let mut pairs: BTreeSet<ValueId> = BTreeSet::new();
    let mut vs = Vec::new();
    for (j, op) in func.ops.iter().enumerate() {
        vs.clear();
        crate::mir_ops::op_values(op, &mut vs);
        let block = |v: &ValueId| *v == o || pairs.contains(v);
        if j == at || !vs.iter().any(|v| block(v) || addr.contains_key(v)) {
            continue;
        }
        let step = match op {
            _ if vs.len() == 1 && block(&vs[0]) && crate::native_move_params::is_release(op, vs[0]) => Step::Nothing,
            Op::Prim { kind: PrimKind::Handle, dst: Some(d), args } if args.len() == 1 && block(&args[0]) && fixed(d) => {
                addr.insert(*d, (args[0], 0));
                Step::Nothing
            }
            Op::IntBinOp { dst, op: IntOp::Add, a, b } if fixed(dst) => {
                let (base, by) = match (addr.get(a), consts.get(b), addr.get(b), consts.get(a)) {
                    (Some(base), Some(by), _, _) | (_, _, Some(base), Some(by)) => (*base, *by),
                    _ => return None,
                };
                addr.insert(*dst, (base.0, base.1 + by));
                Step::Nothing
            }
            Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(d), args } if args.len() == 1 && fixed(d) => match addr.get(&args[0]) {
                Some(&(b, off)) if b == o && off == tag => Step::Tag(o),
                _ => return None,
            },
            Op::Prim { kind: PrimKind::LoadHandle, dst: Some(d), args } if args.len() == 1 && fixed(d) => match addr.get(&args[0]) {
                Some(&(b, off)) if b == o && off == slot0 => {
                    if pair {
                        if func.ret == Some(*d) {
                            return None;
                        }
                        pairs.insert(*d);
                    }
                    Step::Payload { of: o, pair }
                }
                Some(&(b, off)) if pairs.contains(&b) && (off == slot0 || off == slot1) => Step::Field { of: b, k: u32::from(off == slot1) },
                _ => return None,
            },
            _ => return None,
        };
        out.insert(j, step);
    }
    Some(out)
}

fn emit(out: &mut String, indent: usize, line: &str) {
    for _ in 0..indent {
        out.push_str("    ");
    }
    out.push_str(line);
    out.push('\n');
}

/// Render `op` (`ops[i]`) if it is a step of a slice view. `Ok(false)` = not
/// one (the caller renders it).
pub(crate) fn render(
    i: usize,
    op: &Op,
    tys: &mut BTreeMap<ValueId, NTy>,
    out: &mut String,
    indent: usize,
    used_shims: &mut Vec<&'static str>,
) -> Result<bool, LowerError> {
    let Some(step) = PLAN.with(|p| p.borrow().0.get(&i).cloned()) else { return Ok(false) };
    let dst = crate::mir_ops::defined_value(op);
    match (step, dst) {
        (Step::Nothing, _) => {}
        (Step::Call { twin, pair, items }, Some(o)) => {
            let Op::CallFn { args, .. } = op else { return Err(wall("native: a slice view's call is not a call")) };
            let mut codes = Vec::with_capacity(args.len());
            for a in args {
                let CallArg::Handle(v) = a else { return Err(wall("native: a slice view's argument is not a string")) };
                codes.push(match (crate::render_native_move::static_str_operand(*v), tys.get(v)) {
                    (Some(literal), _) => literal,
                    (None, Some(NTy::StrRef)) => var(*v),
                    (None, Some(NTy::Str)) => format!("&{}", var(*v)),
                    (None, other) => return Err(wall(format!("native: a {other:?} argument of a slice view"))),
                });
            }
            used_shims.push(crate::native_floor::floor_header());
            used_shims.extend(items);
            let ty = if pair { "Option<(&str, &str)>" } else { "Option<&str>" };
            emit(out, indent, &format!("let {}: {ty} = {twin}({});", var(o), codes.join(", ")));
        }
        (Step::Tag(o), Some(d)) => {
            tys.insert(d, NTy::I64);
            emit(out, indent, &format!("let mut {}: i64 = {}.is_some() as i64;", var(d), var(o)));
        }
        // The absent side is never observed: its reads sit under the tag.
        (Step::Payload { of, pair: true }, Some(d)) => {
            emit(out, indent, &format!("let {}: (&str, &str) = {}.unwrap_or((\"\", \"\"));", var(d), var(of)));
        }
        (Step::Payload { of, pair: false }, Some(d)) => slice(d, &format!("{}.unwrap_or(\"\")", var(of)), tys, out, indent),
        (Step::Field { of, k }, Some(d)) => slice(d, &format!("{}.{k}", var(of)), tys, out, indent),
        (step, None) => return Err(wall(format!("native: the slice view step {step:?} binds nothing"))),
    }
    Ok(true)
}

/// Bind `d` to the slice `code`: the slice itself where every use of `d`
/// only borrows, else the owned copy its use asks for.
fn slice(d: ValueId, code: &str, tys: &mut BTreeMap<ValueId, NTy>, out: &mut String, indent: usize) {
    if PLAN.with(|p| p.borrow().1.contains(&d)) {
        tys.insert(d, NTy::StrRef);
        emit(out, indent, &format!("let mut {}: &str = {code};", var(d)));
    } else {
        tys.insert(d, NTy::Str);
        emit(out, indent, &format!("let mut {}: AlmideString = AlmideString::from({code});", var(d)));
    }
}
