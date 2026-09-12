//! The EXIT PLAN (#1995): one producer and one renderer for every way a
//! frame ends.
//!
//! #1988, #1990 and #2001 were one defect: the set of frame credits to
//! release was recomputed by hand on each emitter path — the epilogue,
//! the `return_call` sites, the error and guard exits — and each new
//! path forgot a subset. Here the emitter's exit decisions live in ONE
//! place: `exit_plan` derives, from the frame state at the exit site,
//! which credits this edge releases and which it carries on, and
//! `emit_exit` is the only writer of the release instructions. A path
//! that ends a frame without going through them is refused by
//! `scripts/check-exit-sites.sh` (every `return` / `return_call` the
//! frame-emitting files write must sit right after `emit_exit`).
//!
//! Conservation, per edge: the frame's credits — the rc_owned locals and
//! the droppable params — are partitioned into `released ⊎ carried`. The
//! carried set is non-empty on exactly two edges: the loop-converted self
//! tail call (tco.rs: the frame lives on, its locals meet the next
//! iteration's rebind or the epilogue) and a raw-address-rule frame's
//! tail site (a prim-using body keeps every release on the epilogue: a
//! raw view into a local may still be read after the call). Module
//! membership does not enter the plan.
//!
//! The witness (#1696) mirrors the plan: a success or tail exit records
//! its releases (and the frame replacement); an error or guard exit is
//! one path of several, which the straight-line recorder cannot
//! attribute — it is poisoned rather than fed a partial stream.

use std::collections::BTreeSet;

use crate::emitter::Emitter;
use crate::*;

/// How control leaves the frame at this edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Continuation {
    /// The fall-through epilogue: the body's value is on the stack.
    ReturnSuccess,
    /// `f()!` propagating its err, a raised `err(..)`, `!` on none.
    ReturnError,
    /// `guard c else v`: the else value is the frame's return.
    GuardReturn,
    /// A `return_call`. `replaces_frame == false` is the loop-converted
    /// SELF call (tco.rs): the frame is not replaced.
    TailTransfer { replaces_frame: bool },
}

/// The decision for one exit edge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExitPlan {
    pub(crate) continuation: Continuation,
    /// Frame credits released at this edge (deterministic local order).
    pub(crate) released: BTreeSet<u32>,
    /// Frame credits that survive the edge. `released ⊎ carried` is the
    /// whole frame.
    pub(crate) carried: BTreeSet<u32>,
}

impl Emitter<'_> {
    /// The frame's credits right now: the rc_owned locals, then the
    /// droppable params not among them (a param the Assign routes made
    /// an owner is released once — the #1770 double free).
    fn frame_credits(&self) -> (BTreeSet<u32>, BTreeSet<u32>) {
        let owned: BTreeSet<u32> = self.rc_owned.clone();
        let params: BTreeSet<u32> = self
            .rc_frame_params
            .iter()
            .copied()
            .filter(|p| !owned.contains(p) && !self.tail_consumed.contains(p))
            .collect();
        (owned, params)
    }

    /// Derive the plan for one edge from the frame state at the site.
    /// May this frame END in a `return_call`? A tail transfer that
    /// replaces the frame must release the frame's credits first; under
    /// the raw-address rule (a prim body: `tail_release_allowed` false)
    /// the releases cannot run before the jump — a raw pointer into an
    /// owned block may be among the arguments — so a frame that HOLDS
    /// credits keeps the call in non-tail form and lets the epilogue
    /// release after it (#2005: `float.to_string` handed its 4 KB scratch
    /// list to the dead epilogue on every call). A frame with nothing to
    /// release, or a self tail call (loop form), transfers as before.
    pub(crate) fn tail_transfer_ok(&self, replaces_frame: bool) -> bool {
        if self.tail_release_allowed || !replaces_frame {
            return true;
        }
        let (owned, params) = self.frame_credits();
        owned.is_empty() && params.is_empty()
    }

    pub(crate) fn exit_plan(&self, continuation: Continuation) -> ExitPlan {
        let (owned, params) = self.frame_credits();
        let frame: BTreeSet<u32> = owned.union(&params).copied().collect();
        let released: BTreeSet<u32> = match continuation {
            Continuation::ReturnSuccess
            | Continuation::ReturnError
            | Continuation::GuardReturn => frame.clone(),
            Continuation::TailTransfer { replaces_frame } => {
                if !self.tail_release_allowed {
                    // The raw-address rule: every release stays on the
                    // epilogue (dead after a true return_call — the
                    // witness shows the leak; the frame is a prim body).
                    BTreeSet::new()
                } else if replaces_frame {
                    frame.clone()
                } else {
                    // Loop form: the params are rebound by the loop-back,
                    // the locals live on into the next iteration.
                    params.clone()
                }
            }
        };
        let carried: BTreeSet<u32> = frame.difference(&released).copied().collect();
        debug_assert!(released.is_disjoint(&carried));
        debug_assert_eq!(released.len() + carried.len(), frame.len());
        ExitPlan { continuation, released, carried }
    }

    /// The ONE writer of a frame's exit releases. Safe by the epilogue's
    /// argument on every edge: a tail call's arguments are lowered and
    /// rc_arg_guard-inc'd already; an err block shares its payload
    /// (`rc_share_guard` in lower_sum); a guard's borrowed value took its
    /// ret-inc before this; rc_owned holds only flat blocks.
    pub(crate) fn emit_exit(&mut self, plan: &ExitPlan) {
        self.exit_ledger.push(ExitRecord { plan: plan.clone(), start: self.f.byte_len() });
        // Negative-test hook (tests/exit_validation.rs): omit the first
        // release so the validator has a defect to name. Off by default.
        let omit_first = OMIT_FIRST_RELEASE.load(std::sync::atomic::Ordering::Relaxed);
        for (k, &idx) in plan.released.iter().enumerate() {
            if omit_first && k == 0 {
                continue;
            }
            let dec = self.dec_fn_of_local(idx);
            self.f.instructions().local_get(idx).call(dec);
        }
        match plan.continuation {
            Continuation::ReturnSuccess => {
                for &idx in &plan.released {
                    self.witness_dec(idx);
                }
            }
            Continuation::TailTransfer { replaces_frame } => {
                for &idx in &plan.released {
                    self.witness_dec(idx);
                }
                // These were the frame's last events; the dead epilogue
                // the emitter still writes after the jump records nothing.
                // A self tail call's frame lives on (loop form) and its
                // epilogue decs are real.
                if replaces_frame && let Some(w) = self.witness.as_mut() {
                    w.frame_replaced();
                }
            }
            Continuation::ReturnError | Continuation::GuardReturn => {
                if let Some(w) = self.witness.as_mut() {
                    w.poison();
                }
            }
        }
    }
}

/// The negative-test switch: when set, `emit_exit` skips the first release
/// of every plan. Process-wide; tests/exit_validation.rs is its one user.
static OMIT_FIRST_RELEASE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Test hook (E083 negative half): make the emitter omit the first release
/// of every exit plan, so the validator has a defect to name.
#[doc(hidden)]
pub fn test_omit_first_release(on: bool) {
    OMIT_FIRST_RELEASE.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// One exit as `emit_exit` wrote it: the plan, and the function-body byte
/// offset where its releases begin.
#[derive(Clone, Debug)]
pub(crate) struct ExitRecord {
    pub(crate) plan: ExitPlan,
    pub(crate) start: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    LocalGet(u32),
    Call(u32),
    Return,
    ReturnCall,
    FnEnd,
    Other,
}

/// The defect constructor every check shares: the function is fixed, the
/// four lines vary.
struct Defects<'a> {
    function: &'a str,
}

impl Defects<'_> {
    fn at(&self, headline: &str, value: String, expected: &str, emitted: &str) -> EmitError {
        EmitError::OwnershipLowering(OwnDefect {
            headline: headline.to_string(),
            function: self.function.to_string(),
            value,
            expected: expected.to_string(),
            emitted: emitted.to_string(),
        })
    }
}

/// Re-encode the function and read its operators back with their body
/// offsets (the offsets `byte_len` measured: locals vector, then code).
fn read_ops(f: &wasm_encoder::Function, d: &Defects<'_>) -> Result<Vec<(usize, Op)>, EmitError> {
    use wasm_encoder::Encode;
    let mut encoded = Vec::new();
    f.encode(&mut encoded);
    // Skip the LEB128 size prefix.
    let mut pos = 0;
    while encoded[pos] & 0x80 != 0 {
        pos += 1;
    }
    pos += 1;
    let body = &encoded[pos..];
    let fail = |e: wasmparser::BinaryReaderError| {
        d.at("exit validator could not read the function back", e.to_string(), "a parseable body", "wasmparser refused it")
    };
    let fb = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(body, 0));
    let mut reader = fb.get_operators_reader().map_err(fail)?;
    let mut ops: Vec<(usize, Op)> = Vec::new();
    let mut depth: u32 = 0;
    while !reader.eof() {
        let (op, off) = reader.read_with_offset().map_err(fail)?;
        use wasmparser::Operator as W;
        let kind = match op {
            W::LocalGet { local_index } => Op::LocalGet(local_index),
            W::Call { function_index } => Op::Call(function_index),
            W::Return => Op::Return,
            W::ReturnCall { .. } | W::ReturnCallIndirect { .. } => Op::ReturnCall,
            W::Block { .. } | W::Loop { .. } | W::If { .. } | W::TryTable { .. } => {
                depth += 1;
                Op::Other
            }
            W::End if depth == 0 => Op::FnEnd,
            W::End => {
                depth -= 1;
                Op::Other
            }
            _ => Op::Other,
        };
        ops.push((off as usize, kind));
    }
    Ok(ops)
}

/// One exit window: from `start` to the first transfer not inside an
/// already-claimed (nested) window. Returns the locals decremented in it
/// and the transfer's (op index, kind).
fn scan_window(
    ops: &[(usize, Op)],
    start: usize,
    claimed: &[(usize, usize)],
    cont_name: &str,
    drop_fns: &[u32],
    d: &Defects<'_>,
) -> Result<(Vec<u32>, usize, Op), EmitError> {
    let Some(first) = ops.iter().position(|&(off, _)| off >= start) else {
        return Err(d.at("an exit plan has no instructions after it", cont_name.to_string(), "releases and a transfer", "end of function"));
    };
    let mut decs: Vec<u32> = Vec::new();
    let mut j = first;
    while j < ops.len() {
        let (off, kind) = ops[j];
        if let Some(&(_, e)) = claimed.iter().find(|&&(s, e)| off >= s && off <= e) {
            j = ops.iter().position(|&(o, _)| o > e).unwrap_or(ops.len());
            continue;
        }
        match kind {
            Op::Call(fi) if fi == F_DEC_FLAT || drop_fns.contains(&fi) => match (j > first).then(|| ops[j - 1].1) {
                Some(Op::LocalGet(i)) => decs.push(i),
                _ => {
                    return Err(d.at(
                        "a release in an exit window names no local",
                        cont_name.to_string(),
                        "local.get <credit>; call $dec_flat",
                        "call $dec_flat with another operand",
                    ))
                }
            },
            Op::Return | Op::ReturnCall | Op::FnEnd => return Ok((decs, j, kind)),
            _ => {}
        }
        j += 1;
    }
    Err(d.at("an exit plan reaches no transfer", cont_name.to_string(), "return / return_call / end", "none"))
}

/// The plan against the window: the transfer instruction, every released
/// credit decremented, nothing carried or foreign decremented, nothing
/// outstanding across a returning or frame-replacing edge.
fn check_window(
    rec: &ExitRecord,
    decs: &[u32],
    got: Op,
    cont_name: &str,
    name_of: &dyn Fn(u32) -> String,
    d: &Defects<'_>,
) -> Result<(), EmitError> {
    let cont = rec.plan.continuation;
    let want = match cont {
        Continuation::ReturnSuccess => Op::FnEnd,
        Continuation::ReturnError | Continuation::GuardReturn => Op::Return,
        Continuation::TailTransfer { .. } => Op::ReturnCall,
    };
    if got != want {
        return Err(d.at("an exit transfers by a different instruction than its plan", cont_name.to_string(), &format!("{want:?}"), &format!("{got:?}")));
    }
    if let Some(&idx) = rec.plan.released.iter().find(|i| !decs.contains(i)) {
        return Err(d.at("an exit leaves a released credit undecremented", name_of(idx), &format!("release before {cont_name}"), "none"));
    }
    if let Some(&idx) = decs.iter().find(|i| rec.plan.carried.contains(i)) {
        return Err(d.at("an exit releases a credit its plan carries", name_of(idx), &format!("carried across {cont_name}"), "released"));
    }
    if let Some(&idx) = decs.iter().find(|i| !rec.plan.released.contains(i)) {
        return Err(d.at("an exit releases a value outside its plan", name_of(idx), "no release (not a frame credit at this edge)", "released"));
    }
    let replaces = !matches!(cont, Continuation::TailTransfer { replaces_frame: false });
    if replaces && let Some(&idx) = rec.plan.carried.iter().next() {
        return Err(d.at("tail exit leaves an ownership credit outstanding", name_of(idx), "transfer or release before tail transfer", "neither"));
    }
    Ok(())
}

/// E083 (#1996): read the function's bytes BACK and check that every exit
/// window — from the plan's first release to the transfer instruction —
/// implements the plan. Independent of the writer: it parses wasm, not the
/// emitter's intent. A mismatch is a compiler defect with its own
/// diagnostic, never a wall.
pub(crate) fn validate_exits(
    f: &wasm_encoder::Function,
    ledger: &[ExitRecord],
    function: &str,
    drop_fns: &[u32],
    name_of: impl Fn(u32) -> String,
) -> Result<(), EmitError> {
    if ledger.is_empty() {
        return Ok(());
    }
    let d = Defects { function };
    let ops = read_ops(f, &d)?;
    // Windows may nest (an early exit lowered inside a later-started
    // exit's value): claim from the LATEST start backwards, and skip the
    // ranges already claimed.
    let mut order: Vec<usize> = (0..ledger.len()).collect();
    order.sort_by_key(|&k| std::cmp::Reverse(ledger[k].start));
    let mut claimed: Vec<(usize, usize)> = Vec::new();
    for k in order {
        let rec = &ledger[k];
        let cont_name = match rec.plan.continuation {
            Continuation::ReturnSuccess => "the epilogue return",
            Continuation::ReturnError => "the error return",
            Continuation::GuardReturn => "the guard return",
            Continuation::TailTransfer { .. } => "the tail transfer",
        };
        let (decs, tj, tk) = scan_window(&ops, rec.start, &claimed, cont_name, drop_fns, &d)?;
        claimed.push((rec.start, ops[tj].0));
        check_window(rec, &decs, tk, cont_name, &name_of, &d)?;
    }
    Ok(())
}
