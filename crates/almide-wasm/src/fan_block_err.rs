//! The Err exit of a `fan { … }` block in an effect frame other than `main`
//! (#3463, C-199 / ADR-0024 D1): every arm has run, and the LOWEST-INDEX
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

use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

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
    /// rule (data_unwrap.rs): an effect frame with a String err channel
    /// returns it; `main` aborts.
    pub(crate) fn fan_err_propagates(&self) -> bool {
        !self.in_main && matches!(self.fn_ret, Some(SliceTy::Result(_, fe)) if self.types.el(fe) == STR)
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
