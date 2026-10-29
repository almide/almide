//! The http call handle's host-op leaves (#2633) — the guest half of the
//! handle the embedded host serves with the native call core
//! (crates/almide-rt-core/src/http_call_core.rs). stdlib/http_call.almd
//! declares the leaves bodyless (`= _`); they intercept BEFORE resolution,
//! as the http_framed leaves do. `lower_host_leaf` is the one entry for every
//! such bodyless leaf (http_framed, http.serve, subprocess, http call).
//!
//! `HttpCall` is this emitter's own one-slot block (ty.rs `HTTP_CALL_FIELD`)
//! holding the host call id; its drop glue (`Helper::DropHttpCall`) sends op
//! 59 when the last copy is released, so the native `Drop` — cancel — holds
//! on this leg.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, MemArg, ValType};

use crate::emitter::Emitter;
use crate::fs_meta::{
    OP_HTTP_CALL_CANCEL, OP_HTTP_CALL_OPEN, OP_HTTP_CALL_READ, OP_HTTP_CALL_STATE, OP_HTTP_CALL_STEP,
    OP_HTTP_CALL_WAIT,
};
use crate::*;

impl Emitter<'_> {
    /// A bodyless self-host leaf (`= _`) lowered as a host op, or
    /// `Ok(None)` when `name` is not one. They carry no table body, so they
    /// intercept BEFORE resolution (calls.rs), and the caller marks the call
    /// OWNED: every answer here is built fresh at rc 1 by `fs_result_*`
    /// (#3623 — classed borrowed for want of a table entry, the caller took
    /// a +1 on it and the Result block with its payload outlived every
    /// call). The leaf runs inside an `arm_scope`: its operands are
    /// `ArgMode::Borrow`, so a temporary handed straight in (the
    /// `__http_frame(…)` frame, the `__proc_argv(…)` cells) is released once
    /// the answer is on the stack — without the scope it stayed parked and
    /// was never released.
    pub(crate) fn lower_host_leaf(&mut self, name: &str, args: &[IrExpr]) -> Result<Option<SliceTy>, EmitError> {
        self.arm_scope(|em| em.lower_host_leaf_op(name, args))
    }

    fn lower_host_leaf_op(&mut self, name: &str, args: &[IrExpr]) -> Result<Option<SliceTy>, EmitError> {
        use crate::fs_meta::{
            OP_HTTP_FRAMED_RESPONSE, OP_HTTP_FRAMED_STATUS, OP_HTTP_FRAMED_TEXT, OP_HTTP_SERVE_BIND, OP_HTTP_SERVE_NEXT,
            OP_HTTP_SERVE_REPLY, PROC_LEAVES,
        };
        // The http_framed leaves (#1710 increment 3): url in a, the
        // method/body/headers frame in b — the intra-module twins of the
        // host_env http arms.
        let framed = match name {
            "__http_framed_text" => Some(OP_HTTP_FRAMED_TEXT),
            "__http_framed_status" => Some(OP_HTTP_FRAMED_STATUS),
            "__http_framed_response" => Some(OP_HTTP_FRAMED_RESPONSE),
            _ => None,
        };
        // The subprocess leaves (#2589, ADR-0025): two text operands in
        // a/b, a Result[String, String] answer.
        let proc = PROC_LEAVES
            .iter()
            .find(|(l, _)| *l == name)
            .map(|&(_, op)| op)
            .or_else(|| crate::fs_meta::proc_child_op(name));
        if let Some(op) = framed.or(proc) {
            let [a, b] = args else { return unsup("host-leaf-arity") };
            self.fs_call_str2(a, b, op)?;
            return Ok(Some(self.fs_result_string()?));
        }
        // The http.serve loop's leaves (#2650): bind (port text), next (the
        // request as a List[String] of frames), reply (the response's cells
        // in a).
        let t = match (name, args) {
            ("__http_serve_bind", [p]) => {
                self.fs_call_1(p, OP_HTTP_SERVE_BIND)?;
                self.fs_result_unit()?
            }
            ("__http_serve_next", []) => {
                self.fs_call_0(OP_HTTP_SERVE_NEXT)?;
                self.fs_result_string_list()?
            }
            ("__http_serve_reply", [r]) => {
                self.fs_call_1(r, OP_HTTP_SERVE_REPLY)?;
                self.fs_result_unit()?
            }
            // The contained loop (#4203): the step closure's env pointer in
            // the a_len slot; the host calls the closure itself.
            ("__http_serve_run", [step]) => {
                use crate::fs_meta::OP_HTTP_SERVE_RUN;
                self.note_host_op(OP_HTTP_SERVE_RUN);
                let got = self.lower_arg(step, None, ArgMode::Borrow)?;
                if !matches!(got, SliceTy::Fn(_)) {
                    return unsup("http-serve-run-step");
                }
                let env = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(env);
                i.i32_const(OP_HTTP_SERVE_RUN).i32_const(0);
                i.local_get(env);
                i.i32_const(0).i32_const(0);
                i.call(F_FS_CALL);
                let _ = i;
                self.release_i32();
                self.fs_result_unit()?
            }
            // The http call handle's leaves (#2633), then the tcp family's
            // (#3620).
            _ => {
                return match self.lower_http_call_leaf(name, args)? {
                    Some(t) => Ok(Some(t)),
                    None => self.lower_net_leaf(name, args),
                };
            }
        };
        Ok(Some(t))
    }

    /// The `HttpCall` slice type and its id slot's payload offset.
    fn http_call_ty(&self) -> Result<(SliceTy, u32), EmitError> {
        let Some(t) = crate::ty::slice_ty_of(&almide_types::types::Ty::Named(almide_base::intern::sym("HttpCall"), vec![]), self.types)
        else {
            return unsup("http-call-ty");
        };
        let SliceTy::Named(ti) = t else { return unsup("http-call-ty") };
        let crate::types_table::NamedDef::Record(r) = self.types.def(ti) else {
            return unsup("http-call-ty");
        };
        Ok((t, r.fields[0].offset))
    }

    /// A `__http_call_*` leaf, or `Ok(None)` when `name` is not one.
    fn lower_http_call_leaf(&mut self, name: &str, args: &[IrExpr]) -> Result<Option<SliceTy>, EmitError> {
        let op = match name {
            "__http_call_open" => return self.lower_http_call_open(args).map(Some),
            "__http_call_state" => OP_HTTP_CALL_STATE,
            "__http_call_wait_raw" | "__http_call_result_raw" => OP_HTTP_CALL_WAIT,
            "__http_call_read_raw" => OP_HTTP_CALL_READ,
            "__http_call_cancel_raw" => OP_HTTP_CALL_CANCEL,
            "__http_call_step_raw" => OP_HTTP_CALL_STEP,
            _ => return Ok(None),
        };
        let [c] = args else { return unsup("http-call-arity") };
        self.http_call_id_op(c, op)?;
        Ok(Some(if op == OP_HTTP_CALL_WAIT { self.fs_result_string_list()? } else { self.fs_result_string()? }))
    }

    /// `fs_call(op, 0, id, 0, 0)` for the handle `c` — the id rides the
    /// a_len slot with a null a_ptr (the op-35 scalar discipline); the i64
    /// answer is left on the stack.
    fn http_call_id_op(&mut self, c: &IrExpr, op: i32) -> Result<(), EmitError> {
        let (t, off) = self.http_call_ty()?;
        self.note_host_op(op);
        self.lower_arg(c, Some(t), ArgMode::Borrow)?;
        let hc = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hc);
        i.i32_const(op).i32_const(0);
        i.local_get(hc).i64_load(MemArg { offset: u64::from(almide_layout::PAYLOAD + off), align: 2, memory_index: 0 });
        i.i32_wrap_i64();
        i.i32_const(0).i32_const(0);
        i.call(F_FS_CALL);
        let _ = i;
        self.release_i32();
        Ok(())
    }

    /// `__http_call_open(url, frame) -> Result[HttpCall, String]`: status 1
    /// is err(message); otherwise the len half IS the call id, stored into
    /// a fresh `HttpCall` block wrapped in ok.
    fn lower_http_call_open(&mut self, args: &[IrExpr]) -> Result<SliceTy, EmitError> {
        let [url, frame] = args else { return unsup("http-call-open-arity") };
        let (t, off) = self.http_call_ty()?;
        let SliceTy::Named(ti) = t else { return unsup("http-call-ty") };
        let size = match self.types.def(ti) {
            crate::types_table::NamedDef::Record(r) => r.size,
            _ => return unsup("http-call-ty"),
        };
        // The drop glue is part of the type: register it (and its op) with
        // the first producer, whatever releases the block later.
        let _ = self.dec_fn_of(t);
        self.fs_call_str2(url, frame, OP_HTTP_CALL_OPEN)?;
        let hret = self.hold_i64()?;
        let hb = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hret);
        i.local_get(hret).i64_const(32).i64_shr_s().i32_wrap_i64().i32_const(1).i32_eq();
        i.if_(BlockType::Result(ValType::I32));
        // err(msg)
        i.local_get(hret).i64_const(0xFFFF_FFFF).i64_and().i32_wrap_i64().call(F_ALLOC).local_set(hb);
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add().call(F_HOST_READ);
        let hs = self.tmp_i32_local;
        i.i32_const(16).call(F_ALLOC).local_set(hs);
        i.local_get(hs).i32_const(1).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hs).local_get(hb).i32_store(slot_memarg(almide_layout::SUM_FIELD));
        i.local_get(hs);
        i.else_();
        // ok(HttpCall { id })
        i.i32_const(size as i32).call(F_ALLOC).local_set(hb);
        i.local_get(hb).local_get(hret).i64_const(0xFFFF_FFFF).i64_and();
        i.i64_store(MemArg { offset: u64::from(almide_layout::PAYLOAD + off), align: 2, memory_index: 0 });
        i.i32_const(16).call(F_ALLOC).local_set(hs);
        i.local_get(hs).i32_const(0).i32_store(slot_memarg(almide_layout::SUM_TAG));
        i.local_get(hs).local_get(hb).i32_store(slot_memarg(almide_layout::SUM_FIELD));
        i.local_get(hs);
        i.end();
        let _ = i;
        self.release_i32();
        self.release_i64();
        let th = self.types.intern(t);
        let sh = self.types.intern(STR);
        Ok(SliceTy::Result(th, sh))
    }
}
