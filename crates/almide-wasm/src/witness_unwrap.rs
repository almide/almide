//! The `!` site in the structural witness (#2758, #1696 step 4).
//!
//! `f(x)!` lowers as a one-arm branch (data.rs `lower_try_unwrap`): on err
//! (or none) the frame PROPAGATES — the exit plan releases every credit the
//! frame holds (`ReturnError`, the same set a success exit releases) and the
//! err block leaves as the frame's result — and on ok the payload is read out
//! of the carrier. The recorder sees the site as a branch whose first arm
//! ends in an exit:
//!
//! - the carrier a bound local holds (arg_temps.rs parks the usual `f(x)!`
//!   operand) takes the `$inc` the propagation shares it with (`a`), the
//!   exit releases the local with the rest of the frame (`d`), and the
//!   shared block leaves (`m`);
//! - an OWNED carrier (a tail `f(x)!`, never parked) is a temporary born at
//!   the site: it leaves on the err arm (`im`) and is released on the ok
//!   path (`id`, #2509's `release_ok_carrier`);
//! - `!` on none builds a fresh `err("none")` block that leaves (`im`); a
//!   pure Option fn's `!` returns NULL (nothing leaves).
//!
//! The payload read out of a BORROWED carrier is a view: the frame holds no
//! credit of it, and a consumer that keeps it takes one (`bind_view`,
//! `view_share_move`). One read out of an owned carrier carries the
//! carrier's credit (the node is marked owned), so its consumer records a
//! fresh value.
//!
//! In `main` the site ABORTS instead (`Error: {msg}`, exit 1): the process
//! ends, and the path ends in the checker's abort terminal (`t`, format v6),
//! which discharges every credit still outstanding on it.
//!
//! The routes that convert the error on the way out (a typed error into a
//! String channel, #2725) and a pure frame's `!` (a trap) have RC sites or
//! trap edges this does not record: they decline.

use crate::emitter::Emitter;

/// What the `!` site's carrier is to the recorder.
#[derive(Clone, Copy)]
pub(crate) enum WCarrier {
    /// Unrecorded (no recorder, or a none carrier with no block on err).
    None,
    /// The block a bound local holds (resolved per path).
    Local(u32),
    /// An owned temporary born at the site.
    Temp(u32),
}

/// What leaves the frame on the propagating arm.
pub(crate) enum Leaves {
    /// The carrier itself (a Result's err block).
    Carrier,
    /// A fresh block built on the arm (`err("none")`).
    Fresh,
    /// NULL (a pure Option fn's none).
    Nothing,
}

/// Is `e` a slot read (`xs[i]` / `r.f` / `t.0`, chained) whose base is a
/// bound Var?
pub(crate) fn slot_read_of_var(e: &almide_ir::IrExpr) -> bool {
    use almide_ir::IrExprKind as K;
    match &crate::rc_ownership::rc_tail(e).kind {
        K::Var { .. } => true,
        K::IndexAccess { object, .. } | K::Member { object, .. } | K::TupleIndex { object, .. } => slot_read_of_var(object),
        _ => false,
    }
}

/// Is `e` a VIEW read out of a bound block: a payload out of a BORROWED
/// carrier (a `!` over a local), or an element of a bound list (`xs[i]`)?
pub(crate) fn is_extraction_view(e: &almide_ir::IrExpr) -> bool {
    use almide_ir::IrExprKind as K;
    match &crate::rc_ownership::rc_tail(e).kind {
        K::Try { expr } | K::Unwrap { expr } => matches!(expr.kind, K::Var { .. }),
        // #2755: `xs[i]`, `r.f`, `t.0` read a slot of a bound block (through
        // any chain of such reads): the block holds the slot's credit, the
        // read holds none.
        K::IndexAccess { .. } | K::Member { .. } | K::TupleIndex { .. } => slot_read_of_var(e),
        // #2755: `o ?? fallback` over a bound carrier whose join is BORROWED
        // (every caller asks only once the value is known not to be owned):
        // the payload arm reads the carrier's slot, the fallback arm is a
        // view of its own (a static, a borrowed var — an owned fresh fallback
        // under a borrowed join has already declined at the arm,
        // `witness_unwrap_or_arm`).
        K::UnwrapOr { expr, .. } => slot_read_of_var(expr),
        _ => false,
    }
}

impl Emitter<'_> {
    /// The site opens, right before its `if_`: the carrier is named and
    /// the propagating arm begins.
    pub(crate) fn witness_unwrap_open(
        &mut self,
        operand: &almide_ir::IrExpr,
        owned_carrier: bool,
        is_result: bool,
    ) -> WCarrier {
        let src = match &operand.kind {
            almide_ir::IrExprKind::Var { id } => self.locals.get(id).map(|&(l, _)| l),
            _ => None,
        };
        let Some(w) = self.witness.as_mut() else { return WCarrier::None };
        let c = match (owned_carrier, src) {
            // A none carrier has no block on the arm that propagates; its
            // owned block exists on the ok path only (`witness_unwrap_ok`).
            (true, _) if !is_result => WCarrier::None,
            (true, _) => WCarrier::Temp(w.temp_born()),
            (false, Some(l)) => WCarrier::Local(l),
            (false, None) => {
                w.decline("unwrap:borrowed-temp");
                WCarrier::None
            }
        };
        w.branch_open();
        w.branch_arm();
        c
    }

    /// A borrowed Result carrier is shared before it propagates (the
    /// route's `$inc`), then the exit is armed as a recorded propagation.
    pub(crate) fn witness_unwrap_propagate(&mut self, c: WCarrier, shared: bool) {
        let Some(w) = self.witness.as_mut() else { return };
        if shared {
            match c {
                WCarrier::Local(l) if w.share_local(l) => {}
                _ => w.poison(),
            }
        }
        w.arm_err_exit();
    }

    /// After the exit's releases: the value that leaves, and the exit.
    pub(crate) fn witness_unwrap_exit(&mut self, c: WCarrier, leaves: Leaves) {
        let Some(w) = self.witness.as_mut() else { return };
        match (leaves, c) {
            (Leaves::Carrier, WCarrier::Local(l)) => {
                if !w.move_local(l) {
                    w.poison();
                }
            }
            (Leaves::Carrier, WCarrier::Temp(o)) => w.temp_ops(o, "m"),
            (Leaves::Carrier, WCarrier::None) => w.poison(),
            (Leaves::Fresh, _) => w.temp_move(),
            (Leaves::Nothing, _) => {}
        }
        w.frame_replaced();
    }

    /// A route that leaves the frame some other way (a converted error, an
    /// abort, a trap): withdraw. The exit such a route may still emit is armed, so it is attributed
    /// rather than mistaken for a hook disagreement.
    pub(crate) fn witness_unwrap_decline(&mut self, route: &str) {
        self.witness_decline(&format!("unwrap:{route}"));
        if let Some(w) = self.witness.as_mut() {
            w.arm_err_exit();
        }
    }

    /// The site joins (after the `if_`'s `end`): the ok path is the empty
    /// second arm.
    pub(crate) fn witness_unwrap_close(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.branch_arm();
            w.branch_close();
        }
    }

    /// The ok path released an OWNED carrier (`release_ok_carrier`).
    pub(crate) fn witness_unwrap_ok(&mut self, c: WCarrier, owned_carrier: bool) {
        if !owned_carrier {
            return;
        }
        let Some(w) = self.witness.as_mut() else { return };
        match c {
            WCarrier::Temp(o) => w.temp_ops(o, "d"),
            // An owned Option carrier: its block exists on this path only.
            _ => w.temp_discarded(),
        }
    }

    /// #2755: main's raise-leaf abort (`err(m)!`): a FRESH message is a
    /// block born here that the abort takes with it — born, then discharged
    /// by the abort terminal.
    pub(crate) fn witness_abort_message(&mut self, m: &almide_ir::IrExpr) {
        if self.rc_owned_result(m)
            && let Some(w) = self.witness.as_mut()
        {
            w.temp_born();
        }
    }

    /// #2758: a bare `err(e)` RAISED from an effect body (data.rs
    /// `lower_err_raise`), right before its exit plan: the exit's releases
    /// are recorded like a `!` propagation's.
    pub(crate) fn witness_err_raise_arm(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.arm_err_exit();
        }
    }

    /// After the raise's releases: the fresh err block leaves the frame
    /// (`im`) and the path ends.
    pub(crate) fn witness_err_raise_leave(&mut self) {
        if let Some(w) = self.witness.as_mut() {
            w.temp_move();
            w.frame_replaced();
        }
    }
}
