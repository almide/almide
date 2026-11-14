//! Closures that borrow their captures on the native render (#4700).
//!
//! A closure co-owns each heap value it captures: the lowering `Dup`s the
//! source and stores the copy's handle in the closure block (a record, variant
//! or list capture through a two-slot `[tag][payload]` wrapper first). On wasm
//! the `Dup` is a reference count; the native render has none and spells it as
//! a deep copy. When the closure cannot outlive the frame that builds it and
//! the source cannot change while it lives, the share is observationally the
//! source itself, so the native render captures a reference instead. The MIR,
//! the wasm leg and the certificate are unchanged: this is the native spelling
//! of the same co-own.
//!
//! A capture slot borrows when, at every site that builds its lambda's closure:
//!
//! - N1, the closure does not escape: its block is only built (stores through
//!   its own handle), handed to named calls that borrow it (not a by-value
//!   floor or runtime position, not a move param), and released by its one
//!   `DropVariant`. No copy, move, store, return or capture of it.
//! - N2, the source is frozen: it is a parameter the function borrows (not a
//!   move param, not an `inout` one). A borrowed parameter cannot be written,
//!   moved or released by its function, so the window holds by construction;
//!   rustc's borrow checker checks the same again on the rendered crate.
//! - N3, the drop releases nothing borrowed: a borrowed variant owns nothing.
//!
//! Every other slot keeps today's owned copy.

use crate::{CallArg, Init, IntOp, MirFunction, MirProgram, Op, PrimKind, ValueId};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

/// What one function renders differently.
#[derive(Default, Clone, Debug)]
pub(crate) struct FnPlan {
    /// `Dup`s rendered as the source's own reference.
    pub dups: BTreeSet<ValueId>,
    /// Values whose defining op renders nothing: a borrowed slot's wrapper,
    /// the handles taken of it and of the copy, the wrapper's addresses.
    pub swallowed: BTreeSet<ValueId>,
    /// Stores into a swallowed wrapper (op index).
    pub stores: BTreeSet<usize>,
    /// `Consume`s that render nothing (the wrapper, the copy).
    pub consumes: BTreeSet<ValueId>,
    /// A closure slot's stored handle → the copy it borrows.
    pub slot_vals: BTreeMap<ValueId, ValueId>,
    /// Closure blocks holding at least one borrowed capture.
    pub blocks: BTreeSet<ValueId>,
}

#[derive(Default)]
struct Plan {
    fns: BTreeMap<String, FnPlan>,
    /// lambda → borrowed capture slots (slot index → rich wrapper).
    lambdas: BTreeMap<String, BTreeMap<usize, bool>>,
}

thread_local! {
    static PLAN: RefCell<Plan> = RefCell::new(Plan::default());
    /// A lambda's collapsed wrapper reads (the borrowed payload stands for the wrapper).
    static COLLAPSED: RefCell<BTreeSet<(String, ValueId)>> = const { RefCell::new(BTreeSet::new()) };
}

/// One capture slot of one closure-building site.
#[derive(Clone, Debug)]
struct Slot {
    k: usize,
    /// The handle stored into the closure slot.
    stored: ValueId,
    /// The copy (`Dup`) the slot co-owns.
    dup: ValueId,
    /// The wrapper block, for a rich capture.
    wrapper: Option<ValueId>,
    /// Every value and op index the borrowed spelling leaves unrendered.
    swallowed: Vec<ValueId>,
    stores: Vec<usize>,
}

struct Site {
    lambda: String,
    block: ValueId,
    /// `None` = the closure escapes (or is not understood): no slot borrows.
    slots: Option<Vec<Slot>>,
}

fn values(op: &Op) -> Vec<ValueId> {
    let mut out = Vec::new();
    crate::mir_ops::op_values(op, &mut out);
    out
}

struct Index<'a> {
    func: &'a MirFunction,
    def: BTreeMap<ValueId, usize>,
    /// handle value → the value it addresses.
    handle_of: BTreeMap<ValueId, ValueId>,
    /// address value → (handle, constant offset).
    addr_of: BTreeMap<ValueId, (ValueId, i64)>,
    /// value → op indices naming it.
    uses: BTreeMap<ValueId, Vec<usize>>,
}

impl<'a> Index<'a> {
    fn new(func: &'a MirFunction) -> Self {
        let mut def = BTreeMap::new();
        let mut consts = BTreeMap::new();
        let mut handle_of = BTreeMap::new();
        let mut addr_of = BTreeMap::new();
        let mut uses: BTreeMap<ValueId, Vec<usize>> = BTreeMap::new();
        for (i, op) in func.ops.iter().enumerate() {
            if let Some(d) = crate::mir_ops::defined_value(op) {
                def.insert(d, i);
            }
            match op {
                Op::ConstInt { dst, value } => { consts.insert(*dst, *value); }
                Op::Prim { kind: PrimKind::Handle, dst: Some(h), args } if args.len() == 1 => { handle_of.insert(*h, args[0]); }
                Op::IntBinOp { dst, op: IntOp::Add, a, b } if handle_of.contains_key(a) => {
                    if let Some(c) = consts.get(b) { addr_of.insert(*dst, (*a, *c)); }
                }
                _ => {}
            }
            for v in values(op) {
                let at = uses.entry(v).or_default();
                if at.last() != Some(&i) { at.push(i); }
            }
        }
        Index { func, def, handle_of, addr_of, uses }
    }

    fn op_of(&self, v: ValueId) -> Option<&'a Op> {
        self.def.get(&v).map(|i| &self.func.ops[*i])
    }

    fn uses(&self, v: ValueId) -> &[usize] {
        self.uses.get(&v).map(Vec::as_slice).unwrap_or(&[])
    }

    fn is_dyn_block(&self, v: ValueId) -> bool {
        matches!(self.op_of(v), Some(Op::Alloc { init: Init::DynList { .. }, .. }))
    }

    /// The stores through `block`'s own handles: (op index, offset, stored
    /// value). `None` when a handle of it serves anything but those stores
    /// (and, for `extra`, being stored itself).
    fn stores_into(&self, block: ValueId, extra: &BTreeSet<ValueId>) -> Option<Vec<(usize, i64, ValueId)>> {
        let mut out = Vec::new();
        for &i in self.uses(block) {
            if let Op::Prim { kind: PrimKind::Handle, dst: Some(h), .. } = &self.func.ops[i] {
                if !extra.contains(h) {
                    self.handle_stores(*h, &mut out)?;
                }
            }
        }
        Some(out)
    }

    /// The stores through handle `h`: each of its uses is an address it
    /// offsets, and each address only a store's target.
    fn handle_stores(&self, h: ValueId, out: &mut Vec<(usize, i64, ValueId)>) -> Option<()> {
        for &j in self.uses(h) {
            match &self.func.ops[j] {
                Op::Prim { kind: PrimKind::Handle, dst: Some(d), .. } if *d == h => {}
                Op::IntBinOp { dst: a, a: base, .. } if *base == h => self.addr_stores(*a, out)?,
                _ => return None,
            }
        }
        Some(())
    }

    fn addr_stores(&self, a: ValueId, out: &mut Vec<(usize, i64, ValueId)>) -> Option<()> {
        let (_, off) = *self.addr_of.get(&a)?;
        for &s in self.uses(a) {
            match &self.func.ops[s] {
                Op::IntBinOp { dst, .. } if *dst == a => {}
                Op::Prim { kind: PrimKind::Store { width: 8 }, dst: None, args } if args.len() == 2 && args[0] == a && args[1] != a => {
                    out.push((s, off, args[1]));
                }
                _ => return None,
            }
        }
        Some(())
    }
}

/// The parameters `func` borrows for its whole body.
fn borrowed_params(func: &MirFunction) -> BTreeSet<ValueId> {
    func.params
        .iter()
        .filter(|p| p.repr.is_heap() && !func.owned_params.contains(&p.value) && !func.inout_params.contains(&p.value))
        .map(|p| p.value)
        .collect()
}

/// N1: every op naming `block` builds it, lends it to a call, or releases it once.
fn does_not_escape(ix: &Index<'_>, block: ValueId, lends: &dyn Fn(&str, usize) -> bool) -> bool {
    let mut drops = 0;
    for &i in ix.uses(block) {
        match &ix.func.ops[i] {
            Op::Alloc { dst, .. } if *dst == block => {}
            Op::Prim { kind: PrimKind::Handle, dst: Some(_), args } if args.as_slice() == [block] => {}
            Op::CallFn { name, args, .. } => {
                for (k, a) in args.iter().enumerate() {
                    match a {
                        CallArg::Handle(v) if *v == block && !lends(name, k) => return false,
                        _ => {}
                    }
                }
            }
            Op::DropVariant { v, .. } if *v == block => drops += 1,
            _ => return false,
        }
    }
    drops == 1
}

/// The copy `stored` hands the slot: a `Dup` of a borrowed parameter whose
/// only other uses are its handle and its `Consume` into the block.
fn lent_copy(ix: &Index<'_>, handle: ValueId, borrowed: &BTreeSet<ValueId>) -> Option<ValueId> {
    let dup = *ix.handle_of.get(&handle)?;
    let Some(Op::Dup { src, .. }) = ix.op_of(dup) else { return None };
    if !borrowed.contains(src) { return None; }
    if ix.uses(handle).len() != 2 { return None; }
    for &i in ix.uses(dup) {
        match &ix.func.ops[i] {
            Op::Dup { dst, .. } if *dst == dup => {}
            Op::Prim { kind: PrimKind::Handle, dst: Some(h), .. } if *h == handle => {}
            Op::Consume { v } if *v == dup => {}
            _ => return None,
        }
    }
    Some(dup)
}

fn site_slot(ix: &Index<'_>, k: usize, stored: ValueId, borrowed: &BTreeSet<ValueId>) -> Option<Slot> {
    let target = *ix.handle_of.get(&stored)?;
    // A plain heap capture: the slot holds the copy's handle.
    if let Some(dup) = lent_copy(ix, stored, borrowed) {
        return Some(Slot { k, stored, dup, wrapper: None, swallowed: vec![stored], stores: Vec::new() });
    }
    // A rich capture: the slot holds a fresh wrapper whose payload is the copy.
    if !ix.is_dyn_block(target) { return None; }
    let w = target;
    let extra: BTreeSet<ValueId> = [stored].into();
    let stores = ix.stores_into(w, &extra)?;
    let payload = stores.iter().find(|(_, off, _)| *off == 20)?.2;
    if stores.len() != 2 || !stores.iter().any(|(_, off, _)| *off == 12) { return None; }
    let dup = lent_copy(ix, payload, borrowed)?;
    // The wrapper: built, stored into the closure, consumed there — nothing else.
    let mut swallowed = vec![w, stored, payload];
    for &i in ix.uses(w) {
        match &ix.func.ops[i] {
            Op::Alloc { dst, .. } if *dst == w => {}
            Op::Prim { kind: PrimKind::Handle, dst: Some(h), .. } => {
                swallowed.push(*h);
                swallowed.extend(ix.uses(*h).iter().filter_map(|&j| match &ix.func.ops[j] {
                    Op::IntBinOp { dst, a, .. } if a == h => Some(*dst),
                    _ => None,
                }));
            }
            Op::Consume { v } if *v == w => {}
            _ => return None,
        }
    }
    if ix.uses(stored).len() != 2 { return None; }
    Some(Slot { k, stored, dup, wrapper: Some(w), swallowed, stores: stores.iter().map(|(i, _, _)| *i).collect() })
}

/// Every closure `func` builds through the block path, with its slots when
/// it does not escape.
fn sites(func: &MirFunction, lends: &dyn Fn(&str, usize) -> bool) -> Vec<Site> {
    let ix = Index::new(func);
    let borrowed = borrowed_params(func);
    let mut out = Vec::new();
    for op in &func.ops {
        let Op::FuncRef { dst: fr, name } = op else { continue };
        // The block the function reference is stored into at slot 0.
        let block = ix.uses(*fr).iter().find_map(|&i| match &func.ops[i] {
            Op::Prim { kind: PrimKind::Store { width: 8 }, args, .. } if args.len() == 2 && args[1] == *fr => {
                let (h, off) = *ix.addr_of.get(&args[0])?;
                (off == 12).then(|| ix.handle_of.get(&h).copied()).flatten()
            }
            _ => None,
        });
        let Some(block) = block.filter(|b| ix.is_dyn_block(*b)) else {
            out.push(Site { lambda: name.clone(), block: *fr, slots: None });
            continue;
        };
        let slots = (|| {
            if !does_not_escape(&ix, block, lends) { return None; }
            let stores = ix.stores_into(block, &BTreeSet::new())?;
            let mut slots = Vec::new();
            for (_, off, val) in stores {
                if off < 28 || (off - 12) % 8 != 0 { continue; }
                let k = ((off - 12) / 8) as usize;
                if let Some(s) = site_slot(&ix, k, val, &borrowed) { slots.push(s); }
            }
            Some(slots)
        })();
        out.push(Site { lambda: name.clone(), block, slots });
    }
    out
}

/// The payload kinds a borrowed slot can hold: the borrowed spellings the
/// native render already reads a capture through.
fn supported(t: crate::render_native::NTy) -> bool {
    use crate::render_native::NTy;
    matches!(t, NTy::Rec(_) | NTy::Str | NTy::Vec | NTy::VecStr)
}

/// The payload kind of capture slot `k` of `lambda`, when it can borrow.
pub(crate) fn slot_kind(lambda: &str, k: usize) -> Option<crate::render_native::NTy> {
    let caps = crate::lambda_sigs::captures(lambda)?;
    let cap = caps.get(k.checked_sub(2)?)?;
    if cap.cell { return None; }
    let t = crate::render_native_rec::checked_kind(&cap.ty)?;
    supported(t).then_some(t)
}

/// Plan the program's borrowed captures (render start).
pub(crate) fn plan(prog: &MirProgram) {
    let user: BTreeMap<&str, &MirFunction> = prog.functions.iter().map(|f| (f.name.as_str(), f)).collect();
    let lends = |name: &str, k: usize| -> bool {
        !crate::native_move_params::by_value(name, k)
            && user.get(name).is_some_and(|f| f.params.get(k).is_some_and(|p| !f.owned_params.contains(&p.value) && !f.inout_params.contains(&p.value)))
    };
    let all: Vec<(String, Vec<Site>)> = prog.functions.iter().map(|f| (f.name.clone(), sites(f, &lends))).collect();
    let lambdas = chosen_slots(&all);
    let fns = all.into_iter().filter_map(|(name, ss)| fn_plan_of(ss, &lambdas).map(|fp| (name, fp))).collect();
    PLAN.with(|p| *p.borrow_mut() = Plan { fns, lambdas });
    COLLAPSED.with(|c| c.borrow_mut().clear());
}

/// The slots each lambda borrows: those every site of it lets borrow.
fn chosen_slots(all: &[(String, Vec<Site>)]) -> BTreeMap<String, BTreeMap<usize, bool>> {
    let mut lambdas: BTreeMap<String, Option<BTreeMap<usize, bool>>> = BTreeMap::new();
    for s in all.iter().flat_map(|(_, ss)| ss) {
        let here: Option<BTreeMap<usize, bool>> = s.slots.as_ref().map(|v| {
            v.iter().filter(|x| slot_kind(&s.lambda, x.k).is_some() && rich_matches(&s.lambda, x)).map(|x| (x.k, x.wrapper.is_some())).collect()
        });
        let entry = lambdas.entry(s.lambda.clone()).or_insert_with(|| here.clone());
        *entry = match (entry.take(), here) {
            (Some(a), Some(b)) => Some(a.into_iter().filter(|(k, r)| b.get(k) == Some(r)).collect()),
            _ => None,
        };
    }
    lambdas.into_iter().filter_map(|(n, s)| s.filter(|m| !m.is_empty()).map(|m| (n, m))).collect()
}

/// What one function's sites render differently, if anything.
fn fn_plan_of(sites: Vec<Site>, lambdas: &BTreeMap<String, BTreeMap<usize, bool>>) -> Option<FnPlan> {
    let mut fp = FnPlan::default();
    for s in sites {
        let (Some(slots), Some(chosen)) = (s.slots, lambdas.get(&s.lambda)) else { continue };
        for x in slots.into_iter().filter(|x| chosen.contains_key(&x.k)) {
            fp.blocks.insert(s.block);
            fp.dups.insert(x.dup);
            fp.consumes.insert(x.dup);
            fp.consumes.extend(x.wrapper);
            fp.slot_vals.insert(x.stored, x.dup);
            fp.swallowed.extend(x.swallowed.iter().copied());
            fp.stores.extend(x.stores);
        }
    }
    (!fp.blocks.is_empty()).then_some(fp)
}

fn rich_matches(lambda: &str, s: &Slot) -> bool {
    crate::lambda_sigs::captures(lambda)
        .and_then(|c| c.get(s.k.checked_sub(2)?).map(|c| !c.cell && c.rich == s.wrapper.is_some()))
        .unwrap_or(false)
}

/// Does the render hold any borrowed capture? (The closure environment then
/// carries a lifetime.)
pub(crate) fn active() -> bool {
    PLAN.with(|p| !p.borrow().lambdas.is_empty())
}

/// The borrowed slot `k` of `lambda`: Some(rich).
pub(crate) fn lambda_slot(lambda: &str, k: usize) -> Option<bool> {
    PLAN.with(|p| p.borrow().lambdas.get(lambda).and_then(|m| m.get(&k).copied()))
}

pub(crate) fn collapse(v: ValueId) {
    let f = crate::native_hof::current_fn().unwrap_or_default();
    COLLAPSED.with(|c| c.borrow_mut().insert((f, v)));
}

pub(crate) fn collapsed(v: ValueId) -> bool {
    let f = crate::native_hof::current_fn().unwrap_or_default();
    COLLAPSED.with(|c| c.borrow().contains(&(f, v)))
}

/// Pre-render (the move-param pass): is the `Dup` at `at` a copy a
/// non-escaping closure only lends? Such a parameter stays borrowed, so the
/// closure can borrow it instead of the caller copying it in.
pub(crate) fn lent_to_closure(func: &MirFunction, at: usize) -> bool {
    let Some(Op::Dup { dst, .. }) = func.ops.get(at) else { return false };
    let lends = |name: &str, k: usize| !crate::native_move_params::by_value(name, k);
    sites(func, &lends).iter().any(|s| {
        // Ahead of the render the payload's native kind is not resolved yet:
        // the capture's declared shape decides.
        s.slots.as_ref().is_some_and(|v| v.iter().any(|x| x.dup == *dst && rich_matches(&s.lambda, x)))
    })
}

// ---- the native spelling --------------------------------------------------

use crate::lower::LowerError;
use crate::render_native::{var, wall, NTy};
use std::fmt::Write as _;

/// The borrowed kind a payload of kind `t` reads as, and its spelling with
/// lifetime `lt` (`""` for a local).
pub(crate) fn ref_kind(t: NTy, lt: &str) -> Option<(NTy, String)> {
    let a = if lt.is_empty() { String::new() } else { format!("{lt} ") };
    Some(match t {
        NTy::Rec(i) => (NTy::RecRef(i), format!("&{a}{}", crate::render_native_rec::struct_name(i))),
        NTy::Str => (NTy::StrRef, format!("&{a}str")),
        NTy::Vec => (NTy::VecRef, format!("&{a}[i64]")),
        NTy::VecStr => (NTy::VecStrRef, format!("&{a}[AlmideString]")),
        _ => return None,
    })
}

/// The owned payload kind behind a borrowed kind.
fn owned_of(t: NTy) -> NTy {
    match t {
        NTy::RecRef(i) => NTy::Rec(i),
        NTy::StrRef => NTy::Str,
        NTy::VecRef => NTy::Vec,
        NTy::VecStrRef => NTy::VecStr,
        t => t,
    }
}

/// Render op `i` of `func` when the plan spells it differently: the copy a
/// closure borrows is the source's reference, and the wrapper and handles
/// that only carried it render nothing. `Ok(true)` = rendered.
pub(crate) fn render(func: &MirFunction, i: usize, op: &Op, tys: &mut BTreeMap<ValueId, NTy>, out: &mut String, indent: usize) -> Result<bool, LowerError> {
    PLAN.with(|p| {
        let p = p.borrow();
        let Some(fp) = p.fns.get(&func.name) else { return Ok(false) };
        let pad = "    ".repeat(indent);
        match op {
            Op::Dup { dst, src } if fp.dups.contains(dst) => {
                let t = tys.get(src).copied().filter(|t| matches!(t, NTy::RecRef(_) | NTy::StrRef | NTy::VecRef | NTy::VecStrRef));
                let t = t.ok_or_else(|| wall("native: a closure lends a capture that is not a borrowed parameter"))?;
                tys.insert(*dst, t);
                let _ = writeln!(out, "{pad}let mut {} = {}; // lent to a closure", var(*dst), var(*src));
                Ok(true)
            }
            Op::Consume { v } if fp.consumes.contains(v) => Ok(true),
            Op::Prim { kind: PrimKind::Store { .. }, .. } if fp.stores.contains(&i) => Ok(true),
            _ if crate::mir_ops::defined_value(op).is_some_and(|d| fp.swallowed.contains(&d)) => Ok(true),
            _ => Ok(false),
        }
    })
}

/// A closure slot store of `rhs` in `func` that lends: the copy it borrows.
pub(crate) fn slot_value(func: &str, rhs: ValueId) -> Option<ValueId> {
    PLAN.with(|p| p.borrow().fns.get(func).and_then(|f| f.slot_vals.get(&rhs).copied()))
}

/// Is `block` a closure of `func` that holds a borrowed capture?
pub(crate) fn lends_block(func: &str, block: ValueId) -> bool {
    PLAN.with(|p| p.borrow().fns.get(func).is_some_and(|f| f.blocks.contains(&block)))
}

/// The capture-slot store of a lent copy `d` into closure `base`, slot `k`.
pub(crate) fn put_line(base: ValueId, k: usize, d: ValueId, tys: &BTreeMap<ValueId, NTy>) -> Result<String, LowerError> {
    let t = tys.get(&d).copied().ok_or_else(|| wall("native: an untyped lent capture"))?;
    let j = crate::render_native_rec::borrow_variant(owned_of(t)).ok_or_else(|| wall("native: a lent capture of an unsupported kind"))?;
    Ok(format!("{}.put({k}, __AlmdCapture::R{j}({}));", var(base), var(d)))
}

/// The read of borrowed slot `k` of the lambda `lambda` into `dst`.
pub(crate) fn read_line(lambda: &str, base: ValueId, k: usize, dst: ValueId, tys: &mut BTreeMap<ValueId, NTy>) -> Option<Result<String, LowerError>> {
    let rich = lambda_slot(lambda, k)?;
    Some((|| {
        let t = slot_kind(lambda, k).ok_or_else(|| wall("native: a lent capture of an unsupported kind"))?;
        let (r, spelled) = ref_kind(t, "").ok_or_else(|| wall("native: a lent capture of an unsupported kind"))?;
        let j = crate::render_native_rec::borrow_variant(t).ok_or_else(|| wall("native: a lent capture of an unsupported kind"))?;
        let owned = if matches!(t, NTy::Rec(_)) { "x" } else { "&x[..]" };
        tys.insert(dst, r);
        if rich { collapse(dst); }
        Ok(format!(
            "let mut {}: {spelled} = match {}.heap({k}) {{ Some(__AlmdCapture::R{j}(x)) => *x, Some(__AlmdCapture::O{j}(x)) => {owned}, _ => {{ almide_abort(\"closure capture kind mismatch\") }} }};",
            var(dst), var(base)
        ))
    })())
}

/// Spell every reference to a closure alias in `text` as
/// `&__AlmdClosure<'_>` (whole identifiers only): a borrowed closure accepts
/// a closure of any lifetime, an owned one stays the `'static` alias.
pub(crate) fn spell_closure_refs(text: &str, names: &[String]) -> String {
    let mut out = text.to_string();
    for n in names {
        let pat = format!("&{n}");
        let mut res = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(at) = rest.find(&pat) {
            let end = at + pat.len();
            let next = rest.as_bytes().get(end).copied();
            res.push_str(&rest[..at]);
            if next.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
                res.push_str(&pat);
            } else {
                res.push_str("&__AlmdClosure<'_>");
            }
            rest = &rest[end..];
        }
        res.push_str(rest);
        out = res;
    }
    out
}
