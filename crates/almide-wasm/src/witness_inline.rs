//! The inlined-callback witness hooks of the native arms that run a literal
//! callback AT MOST ONCE (#2755 / #2758): the option / result combinators
//! (sums.rs). Split from witness_hooks.rs for the file budget.
//!
//! Such an arm is a branch site of its own: one arm runs the callback body
//! (lowered inline, its sites the ordinary hooks; each param a VIEW of the
//! payload it is loaded from), the other passes the subject through or
//! shares its payload. The body's value is settled by the share guard the
//! arm emits after it, which `witness_store` mirrors; what the arm does with
//! a RETAINED subject (hand it back, release it) is the arm's own credit,
//! recorded at `lower_arg` as the move into the arm.

use crate::emitter::Emitter;
use crate::SliceTy;

/// Where a callback value's one credit lives once an arm took it: a
/// temporary or a view the recorder names by object, or a bound local.
#[derive(Clone, Copy)]
pub(crate) enum WCredit {
    Obj(u32),
    Local(u32),
}

impl Emitter<'_> {
    /// The callback's arm of the site opens (after `hof_lambda` mapped the
    /// params): the callback node is the arm's argument, hooked here for
    /// the module-call audit (it never passes `lower_arg`).
    pub(crate) fn witness_once_open(&mut self, cb: &almide_ir::IrExpr) {
        let Some(w) = self.witness.as_mut() else { return };
        w.note_arg(cb as *const almide_ir::IrExpr as usize);
        w.branch_open();
        w.branch_arm();
        let almide_ir::IrExprKind::Lambda { params, .. } = &cb.kind else { return };
        for (var, _) in params {
            if let Some(&(idx, ty)) = self.locals.get(var) {
                self.witness_view_local(idx, ty);
            }
        }
    }

    /// The site's other arm begins.
    pub(crate) fn witness_once_arm(&mut self) {
        self.witness_branch_arm();
    }

    /// The site joins.
    pub(crate) fn witness_once_close(&mut self) {
        self.witness_branch_close();
    }

    /// A payload read out of the subject and handed out with a share
    /// (`share_handle_top`): a view that takes the +1 and moves it to the
    /// arm's owned result (`am`). A droppable payload that is not a handle
    /// takes no +1 the owned result could stand on: decline.
    pub(crate) fn witness_payload_share(&mut self, t: SliceTy) {
        if self.witness.is_none() || !self.rc_droppable(t) {
            return;
        }
        if !self.elem_is_handle(t) {
            self.witness_decline("payload-share:flat");
            return;
        }
        if let Some(w) = self.witness.as_mut() {
            w.view_share_move();
        }
    }

    /// A callback value an arm guards with `rc_inc_top` when it is not owned
    /// (`droppable && !owned`) and hands to a holder it builds: an owned
    /// value moves (`im`), a borrowed Var or view shares and moves (`am`).
    pub(crate) fn witness_guarded(&mut self, e: &almide_ir::IrExpr, t: SliceTy) {
        if self.witness.is_some() && self.rc_droppable(t) {
            self.witness_share_or_move(e, "callback-value:borrowed-temp");
        }
    }

    /// A callback value an arm RELEASES when it arrived owned and leaves to
    /// its holder otherwise (`list.filter_map`'s Option, `flat_map`'s handle
    /// chunk): an owned one is born and released here (`id`).
    pub(crate) fn witness_owned_released(&mut self, e: &almide_ir::IrExpr, t: SliceTy) {
        if self.rc_droppable(t)
            && self.rc_owned_result(crate::rc_ownership::rc_tail(e))
            && let Some(w) = self.witness.as_mut()
        {
            w.temp_borrowed();
        }
    }

    /// A callback value an arm shares when borrowed and releases either way
    /// (`flat_map`'s scalar chunk): owned — born and released (`id`);
    /// borrowed — the share and the release land on its source (`ad`).
    pub(crate) fn witness_shared_released(&mut self, e: &almide_ir::IrExpr, t: SliceTy) {
        if self.witness.is_none() || !self.rc_droppable(t) {
            return;
        }
        let tail = crate::rc_ownership::rc_tail(e);
        let owned = self.rc_owned_result(tail);
        let src = match &tail.kind {
            almide_ir::IrExprKind::Var { id } => self.locals.get(id).map(|&(l, _)| l),
            _ => None,
        };
        let view = crate::witness_unwrap::is_extraction_view(tail);
        let Some(w) = self.witness.as_mut() else { return };
        match (owned, src) {
            (true, _) => w.temp_borrowed(),
            (false, Some(l)) => {
                if !(w.share_local(l) && w.dec_local(l)) {
                    w.poison();
                }
            }
            (false, None) if view => {
                w.view_ops("ad");
            }
            (false, None) => w.decline("callback-value:borrowed-temp"),
        }
    }

    /// A scan's HIT (`map.find`), right after the predicate: a one-arm
    /// branch where each droppable handle param — a view of the entry it
    /// was loaded from — is shared into the arm's fresh result (`am`), and
    /// the scan breaks. The activation closes after the site.
    pub(crate) fn witness_hit(&mut self, views: &[(u32, SliceTy)]) {
        self.witness_branch_open();
        self.witness_branch_arm();
        self.witness_view_shares(views);
        self.witness_branch_arm();
        self.witness_branch_close();
        self.witness_loop_close();
    }

    /// Each droppable handle param — a view of the element it was loaded
    /// from — shared into the arm's fresh result (`share_handle_top`, `am`).
    pub(crate) fn witness_view_shares(&mut self, views: &[(u32, SliceTy)]) {
        let shared: Vec<u32> =
            views.iter().filter(|&&(_, t)| self.rc_droppable(t) && self.elem_is_handle(t)).map(|&(l, _)| l).collect();
        let Some(w) = self.witness.as_mut() else { return };
        if !shared.iter().all(|&l| w.arg_share_move(l)) {
            w.poison();
        }
    }

    /// `set.map`'s member, per element (collections_set.rs): a branch on
    /// whether the accumulator already holds it. An OWNED member is born
    /// here and released when present (`d`) or moves into the accumulator
    /// when absent (`m`); a borrowed one is left alone when present and
    /// shared into the accumulator when absent (`am`). The activation closes
    /// after the site.
    pub(crate) fn witness_member_step(&mut self, body: &almide_ir::IrExpr, t: SliceTy, owned: bool) {
        if self.witness.is_none() {
            return;
        }
        let droppable = self.rc_droppable(t);
        let c = if droppable && owned { self.witness.as_mut().map(|w| w.temp_born()) } else { None };
        self.witness_branch_open();
        self.witness_branch_arm();
        if let (Some(o), Some(w)) = (c, self.witness.as_mut()) {
            w.temp_ops(o, "d");
        }
        self.witness_branch_arm();
        match c {
            Some(o) => {
                if let Some(w) = self.witness.as_mut() {
                    w.temp_ops(o, "m");
                }
            }
            None if droppable && self.elem_is_handle(t) => self.witness_share_or_move(body, "set-member:borrowed-temp"),
            None if droppable => self.witness_decline("set-member:flat"),
            None => {}
        }
        self.witness_branch_close();
        self.witness_loop_close();
    }

    /// A fused `src |> map* |> filter* |> fold` (list_fuse.rs), at the top
    /// of each iteration: the chain node is consumed whole (its stages are
    /// inlined, never built), one activation covers every stage, and a heap
    /// accumulator is the loop-carried owner `list.fold`'s is.
    pub(crate) fn witness_fused_open(&mut self, chain: &almide_ir::IrExpr, cb: &almide_ir::IrExpr, acc: u32, t: SliceTy) {
        let carried = self.rc_droppable(t);
        let Some(w) = self.witness.as_mut() else { return };
        w.note_arg(chain as *const almide_ir::IrExpr as usize);
        w.note_arg(cb as *const almide_ir::IrExpr as usize);
        w.loop_open();
        if carried {
            w.carried_owned(acc);
        }
    }

    /// A fused stage's param takes the element: an OWNED map result (a
    /// fresh handle the stage before produced) makes the param its owner —
    /// born here, released by the chain once the element is done with (the
    /// iteration-end `d`); anything else is a view.
    pub(crate) fn witness_stage_param(&mut self, local: u32, t: SliceTy, owned: bool) {
        if !self.rc_droppable(t) {
            return;
        }
        let Some(w) = self.witness.as_mut() else { return };
        if owned {
            w.bind_fresh(local);
        } else {
            w.param_borrowed(local);
        }
    }

    /// A fused filter's verdict: false releases what the element owns and
    /// skips to the next element — the iteration ends on that arm.
    pub(crate) fn witness_filter_skip(&mut self) {
        let Some(w) = self.witness.as_mut() else { return };
        w.branch_open();
        w.branch_arm();
        w.loop_jump();
        w.branch_arm();
        w.branch_close();
    }

    /// A callback value an arm takes ONE credit on, to settle later on each
    /// arm of a branch ([`Self::witness_credit_ops`]): an owned value is born
    /// with it (`i`); a borrowed Var or view takes the guard's share (`a`).
    pub(crate) fn witness_credit_take(&mut self, e: &almide_ir::IrExpr, t: SliceTy) -> Option<WCredit> {
        if self.witness.is_none() || !self.rc_droppable(t) {
            return None;
        }
        let tail = crate::rc_ownership::rc_tail(e);
        let owned = self.rc_owned_result(tail);
        let src = match &tail.kind {
            almide_ir::IrExprKind::Var { id } => self.locals.get(id).map(|&(l, _)| l),
            _ => None,
        };
        let view = crate::witness_unwrap::is_extraction_view(tail);
        let w = self.witness.as_mut()?;
        match (owned, src) {
            (true, _) => Some(WCredit::Obj(w.temp_born())),
            (false, Some(l)) if w.share_local(l) => Some(WCredit::Local(l)),
            (false, Some(_)) => {
                w.poison();
                None
            }
            (false, None) if view => Some(WCredit::Obj(w.view_ops("a"))),
            (false, None) => {
                w.decline("callback-value:borrowed-temp");
                None
            }
        }
    }

    /// Settle a credit [`Self::witness_credit_take`] took: `m` (moved into a
    /// holder) or `d` (released).
    pub(crate) fn witness_credit_ops(&mut self, c: Option<WCredit>, ops: &str) {
        let (Some(c), Some(w)) = (c, self.witness.as_mut()) else { return };
        let ok = match c {
            WCredit::Obj(o) => {
                w.temp_ops(o, ops);
                true
            }
            WCredit::Local(l) => ops.chars().all(|op| if op == 'm' { w.move_local(l) } else { w.dec_local(l) }),
        };
        if !ok {
            w.poison();
        }
    }

    /// `list.unique_by`'s key, per element: kept by the seen list on a first
    /// sighting (`m`), released on a repeat (`d`). The activation closes.
    pub(crate) fn witness_seen_key(&mut self, c: Option<WCredit>) {
        self.witness_branch_open();
        self.witness_branch_arm();
        self.witness_credit_ops(c, "m");
        self.witness_branch_arm();
        self.witness_credit_ops(c, "d");
        self.witness_branch_close();
        self.witness_loop_close();
    }

    /// A one-arm abort site the route emitted right before (`if over { abort
    /// }`): the arm ends in the checker's abort terminal, releasing nothing.
    pub(crate) fn witness_abort_site(&mut self) {
        let Some(w) = self.witness.as_mut() else { return };
        w.branch_open();
        w.branch_arm();
        w.abort_end();
        w.branch_arm();
        w.branch_close();
    }

    /// `r?` over a Result (data.rs): an OWNED carrier is released on both
    /// arms — with its err payload, or as a spine whose ok payload moved into
    /// the fresh some-cell — so it is born and released here (`id`); over a
    /// BORROWED carrier the ok arm shares the payload into the cell.
    pub(crate) fn witness_to_option(&mut self, owned_carrier: bool, payload: SliceTy) {
        if owned_carrier {
            self.witness_discard();
            return;
        }
        self.witness_branch_open();
        self.witness_branch_arm();
        self.witness_branch_arm();
        self.witness_payload_share(payload);
        self.witness_branch_close();
    }

    /// main's err channel (err_channel.rs): the carrier is read; its err arm
    /// ABORTS (the checker's terminal discharges what is held), its ok arm
    /// releases the carrier when main owns it — born here, released there.
    pub(crate) fn witness_main_carrier(&mut self, owned: bool) {
        let Some(w) = self.witness.as_mut() else { return };
        let c = owned.then(|| w.temp_born());
        w.branch_open();
        w.branch_arm();
        w.abort_end();
        w.branch_arm();
        if let Some(o) = c {
            w.temp_ops(o, "d");
        }
        w.branch_close();
    }
}
