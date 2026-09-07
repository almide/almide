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
        let params: BTreeSet<u32> =
            self.rc_frame_params.iter().copied().filter(|p| !owned.contains(p)).collect();
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
        for &idx in &plan.released {
            self.f.instructions().local_get(idx).call(F_DEC_FLAT);
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
