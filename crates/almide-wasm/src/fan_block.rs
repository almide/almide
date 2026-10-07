//! A `fan { … }` block's arm reads, its first-err exit and its value (the
//! loop over the arms is fan.rs `lower_fan_block`).
//!
//! In `main` (or a frame with no String err channel) each Result arm's
//! carrier is read at its arm — the first err's message kept, the payload
//! taken, an owned spine released — and after the last arm the first err
//! aborts with the bare message.
//!
//! In an effect frame other than `main` (#3463, C-199 / ADR-0024 D1): every
//! arm has run, and the LOWEST-INDEX
//! Err is the block's result, which the frame RETURNS exactly as a `!`
//! there returns its operand's err block (data_unwrap.rs). Only `main` (and
//! a frame with no String err channel) aborts with the bare message.
//!
//! The arms are lowered in order and each Result arm's CARRIER is kept in a
//! hold — not read and released at the arm, as the abort mode does — so the
//! exit can hand one of them back whole. After the last arm, a chain of
//! one-arm sites in arm order (`if carrier_k is err { … return }`) picks the
//! first err; on reaching site k every lower arm is known ok. Its arm:
//!
//! - releases every OTHER owned carrier whole (its drop reads the tag, so an
//!   ok or an err one alike) and every owned droppable pure arm value;
//! - shares a borrowed chosen carrier (`$inc`) — an owned one moves out;
//! - leaves through the exit plan (`ReturnError`), the frame's credits
//!   released as on any `!` propagation, with the carrier as the value.
//!
//! Past the chain every carrier is ok: its payload is read out and an owned
//! carrier's spine released (`$dec_flat`), the abort mode's per-arm read.
//!
//! The witness mirrors it: an owned carrier is born at its arm (`i`); on
//! site k's arm the chosen one leaves (`m`) and the others are released
//! (`d`), a borrowed chosen one is a view shared out (`am`), a released pure
//! value is born and released there (`id`); on the ok path each owned
//! carrier's spine is released (`d`) and its payload settles at the slot as
//! before.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

/// One arm's value: (hold, type, does the hold own its value's credit, the
/// arm, was it a carrier) — the last two for the witness's slot record.
pub(crate) type FanVal<'a> = (u32, SliceTy, bool, &'a IrExpr, bool);

/// One Result arm's carrier, kept until the block decides.
pub(crate) struct FanCarrier {
    /// The hold the carrier block lives in.
    hold: u32,
    /// The carrier's Result type (its whole-block drop).
    ty: SliceTy,
    /// Does this frame own the carrier's credit?
    owned: bool,
    /// The recorder's object for an owned carrier.
    obj: Option<u32>,
    /// The payload's hold and type, filled past the chain.
    hv: u32,
    p: SliceTy,
}

impl Emitter<'_> {
    /// Does a fan block's Err return from this frame (else abort)? The `!`
    /// rule (data_unwrap.rs): an effect frame with an err channel returns it
    /// — a String one, or its own typed error (#3467); `main` aborts.
    pub(crate) fn fan_err_propagates(&self) -> bool {
        self.fan_frame_err().is_some()
    }

    /// The err type a propagating frame returns a fan block's Err in.
    fn fan_frame_err(&self) -> Option<SliceTy> {
        match self.fn_ret {
            Some(SliceTy::Result(_, fe)) if !self.in_main => Some(self.types.el(fe)),
            _ => None,
        }
    }

    /// The arm's carrier (on the stack) into a hold of its own.
    pub(crate) fn fan_keep_carrier(&mut self, ty: SliceTy, owned: bool, hv: u32, p: SliceTy) -> Result<FanCarrier, EmitError> {
        let hold = self.hold_i32()?;
        self.f.instructions().local_set(hold);
        let obj = if owned { self.witness.as_mut().map(|w| w.temp_born()) } else { None };
        Ok(FanCarrier { hold, ty, owned, obj, hv, p })
    }

    /// The chain of err sites in arm order; `pures` are the owned droppable
    /// pure arm values (hold, type) an exit releases.
    pub(crate) fn fan_return_first_err(&mut self, carriers: &[FanCarrier], pures: &[(u32, SliceTy)]) {
        for k in 0..carriers.len() {
            self.fan_err_site(carriers, k, pures);
        }
    }

    /// Site k: carrier k is an err (every lower one was ok) — it is the
    /// block's result, and the frame returns it.
    fn fan_err_site(&mut self, carriers: &[FanCarrier], k: usize, pures: &[(u32, SliceTy)]) {
        let c = &carriers[k];
        self.f
            .instructions()
            .local_get(c.hold)
            .i32_load(slot_memarg(almide_layout::SUM_TAG))
            .if_(BlockType::Empty);
        self.witness_branch_open();
        self.witness_branch_arm();
        for o in carriers.iter().enumerate().filter(|&(j, o)| j != k && o.owned).map(|(_, o)| o) {
            let dec = self.dec_fn_of(o.ty);
            self.f.instructions().local_get(o.hold).call(dec);
            self.fan_witness_ops(o.obj, "d");
        }
        for &(h, t) in pures {
            let dec = self.dec_fn_of(t);
            self.f.instructions().local_get(h).call(dec);
            self.witness_discard();
        }
        if !c.owned {
            self.f.instructions().local_get(c.hold).call(F_INC);
            if let Some(w) = self.witness.as_mut() {
                w.view_share_move();
            }
        }
        if let Some(w) = self.witness.as_mut() {
            w.arm_err_exit();
        }
        let plan = self.exit_plan(crate::exit_plan::Continuation::ReturnError);
        self.emit_exit(&plan);
        self.f.instructions().local_get(c.hold).return_();
        self.fan_witness_ops(c.obj, "m");
        if let Some(w) = self.witness.as_mut() {
            w.frame_replaced();
        }
        self.f.instructions().end();
        self.witness_branch_arm();
        self.witness_branch_close();
    }

    /// Past the chain the carrier is ok: its payload into its hold, an
    /// owned carrier's spine released (the payload keeps its credit).
    pub(crate) fn fan_carrier_payload(&mut self, c: &FanCarrier) {
        self.f.instructions().local_get(c.hold);
        self.load_ty_slot(c.p, almide_layout::SUM_FIELD);
        self.f.instructions().local_set(c.hv);
        if c.owned {
            self.f.instructions().local_get(c.hold).call(F_DEC_FLAT);
            self.fan_witness_ops(c.obj, "d");
        }
    }

    fn fan_witness_ops(&mut self, obj: Option<u32>, ops: &str) {
        if let (Some(o), Some(w)) = (obj, self.witness.as_mut()) {
            w.temp_ops(o, ops);
        }
    }
}

impl Emitter<'_> {
    /// One arm's value (on the stack) into its hold: a Result arm's carrier
    /// read at once (abort mode) or kept (propagating mode).
    pub(crate) fn fan_block_arm<'a>(
        &mut self,
        got: SliceTy,
        arm: &'a IrExpr,
        herr: Option<u32>,
        vals: &mut Vec<FanVal<'a>>,
        carriers: &mut Vec<FanCarrier>,
    ) -> Result<(), EmitError> {
        let owned = self.rc_owned_result(arm);
        let SliceTy::Result(o, er) = got else {
            let hv = self.hold_val(got)?;
            self.f.instructions().local_set(hv);
            vals.push((hv, got, owned, arm, false));
            return Ok(());
        };
        // The arm's err must be the one the block exits with: the frame's
        // own (propagating), or the String message `main` aborts with.
        if self.types.el(er) != self.fan_frame_err().unwrap_or(STR) {
            return unsup("fan-block-err-ty");
        }
        let p = self.types.el(o);
        let hv = self.hold_val(p)?;
        match herr {
            Some(herr) => self.fan_arm_first_err(herr, hv, p, owned)?,
            None => carriers.push(self.fan_keep_carrier(got, owned, hv, p)?),
        }
        vals.push((hv, p, owned, arm, true));
        Ok(())
    }

    /// After the last arm: the first err aborts (`herr`, abort mode) or is
    /// returned (propagating mode, whose ok path then reads the payloads).
    pub(crate) fn fan_block_decide(&mut self, herr: Option<u32>, carriers: &[FanCarrier], vals: &[FanVal<'_>]) {
        if let Some(herr) = herr {
            // first err → the bare-message abort frame
            self.f.instructions().local_get(herr).if_(BlockType::Empty);
            self.f.instructions().local_get(herr);
            self.emit_error_frame_abort();
            self.f.instructions().end();
            self.witness_abort_site();
            return;
        }
        let pures: Vec<(u32, SliceTy)> = vals
            .iter()
            .filter(|&&(_, p, owned, _, carrier)| !carrier && owned && self.rc_droppable(p))
            .map(|&(hv, p, ..)| (hv, p))
            .collect();
        self.fan_return_first_err(carriers, &pures);
        for c in carriers {
            self.fan_carrier_payload(c);
        }
    }

    /// The abort mode's per-arm read of a Result arm (the carrier is on the
    /// stack): the first err's message into `herr`, the ok payload into
    /// `hv`, an OWNED carrier's spine released (#2969: its payload credit
    /// moves into the value; an err aborts below).
    fn fan_arm_first_err(&mut self, herr: u32, hv: u32, p: SliceTy, owned: bool) -> Result<(), EmitError> {
        self.witness_fan_block_arm(true, owned);
        let ha = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_set(ha);
            i.local_get(ha).i32_load(slot_memarg(almide_layout::SUM_TAG)).i32_const(0).i32_ne();
            i.local_get(herr).i32_eqz();
            i.i32_and().if_(BlockType::Empty);
            i.local_get(ha);
        }
        self.load_ty_slot(STR, almide_layout::SUM_FIELD);
        self.f.instructions().local_set(herr).end();
        self.f.instructions().local_get(ha);
        self.load_ty_slot(p, almide_layout::SUM_FIELD);
        self.f.instructions().local_set(hv);
        if owned {
            self.f.instructions().local_get(ha).call(F_DEC_FLAT);
        }
        self.release_i32();
        Ok(())
    }

    /// The block's value: the one arm's, or a fresh tuple that owns every
    /// slot. Returns its type and whether the stack value carries a credit.
    pub(crate) fn fan_block_value(&mut self, vals: &[FanVal<'_>]) -> Result<(SliceTy, bool), EmitError> {
        if let [(hv, p, owned, _, _)] = vals {
            self.f.instructions().local_get(*hv);
            return Ok((*p, *owned));
        }
        let tys: Vec<SliceTy> = vals.iter().map(|(_, p, ..)| *p).collect();
        let ti = self.types.tuple(tys);
        let def = self.types.tuple_def(ti);
        let hb = self.hold_i32()?;
        self.f.instructions().i32_const(def.size as i32).call(F_ALLOC).local_set(hb);
        for ((hv, p, owned, arm, carrier), (fty, off)) in vals.iter().zip(def.fields.clone()) {
            debug_assert_eq!(*p, fty);
            self.f.instructions().local_get(hb).local_get(*hv);
            // #2969: the fresh tuple owns every slot — a borrowed
            // value takes its credit here.
            if !owned {
                self.share_handle_top(*p);
            }
            self.witness_fan_block_slot(arm, *carrier, *p, *owned);
            self.store_ty_slot(*p, off);
        }
        self.f.instructions().local_get(hb);
        self.release_i32();
        Ok((SliceTy::Tuple(ti), true))
    }
}
