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

/// A bodyless self-host leaf (`= _`), resolved from its name before anything
/// is emitted — the one table every leaf family's name is read from.
#[derive(Clone, Copy)]
enum HostLeaf {
    /// Two text-layout operands in a/b, a Result[String, String] answer:
    /// the http_framed leaves and the subprocess leaves.
    Two(i32),
    ServeBind,
    ServeNext,
    ServeReply,
    ServeRun,
    CallOpen,
    /// A `__http_call_*` op on the handle's id.
    CallId(i32),
    /// A net / udp / unix / shm leaf (net_call.rs).
    Net(i32),
}

fn host_leaf(name: &str) -> Option<HostLeaf> {
    use crate::fs_meta::{OP_HTTP_FRAMED_RESPONSE, OP_HTTP_FRAMED_STATUS, OP_HTTP_FRAMED_TEXT, PROC_LEAVES};
    use HostLeaf::*;
    Some(match name {
        // The http_framed leaves (#1710 increment 3): url in a, the
        // method/body/headers frame in b — the intra-module twins of the
        // host_env http arms.
        "__http_framed_text" => Two(OP_HTTP_FRAMED_TEXT),
        "__http_framed_status" => Two(OP_HTTP_FRAMED_STATUS),
        "__http_framed_response" => Two(OP_HTTP_FRAMED_RESPONSE),
        // The http.serve loop's leaves (#2650).
        "__http_serve_bind" => ServeBind,
        "__http_serve_next" => ServeNext,
        "__http_serve_reply" => ServeReply,
        "__http_serve_run" => ServeRun,
        // The http call handle's leaves (#2633).
        "__http_call_open" => CallOpen,
        "__http_call_state" => CallId(OP_HTTP_CALL_STATE),
        "__http_call_wait_raw" | "__http_call_result_raw" => CallId(OP_HTTP_CALL_WAIT),
        "__http_call_read_raw" => CallId(OP_HTTP_CALL_READ),
        "__http_call_cancel_raw" => CallId(OP_HTTP_CALL_CANCEL),
        "__http_call_step_raw" => CallId(OP_HTTP_CALL_STEP),
        // The subprocess leaves (#2589, ADR-0025), then the tcp family's
        // (#3620).
        _ => {
            return PROC_LEAVES
                .iter()
                .find(|(l, _)| *l == name)
                .map(|&(_, op)| op)
                .or_else(|| crate::fs_meta::proc_child_op(name))
                .map(Two)
                .or_else(|| crate::net_call::leaf_op(name).map(Net));
        }
    })
}

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
    ///
    /// The witness covers a leaf as it covers a module arm (calls_modules.rs
    /// `lower_module_call`): every operand enters through `lower_arg` (the
    /// argument hook), the audit asks that a hook fired for each of the
    /// call's own argument nodes — or the frame declines — and the leaf's
    /// instructions, with the scope's releases, are the arm window the RC
    /// byte check reads.
    pub(crate) fn lower_host_leaf(&mut self, name: &str, args: &[IrExpr]) -> Result<Option<SliceTy>, EmitError> {
        let Some(leaf) = host_leaf(name) else { return Ok(None) };
        let hooks_before = self.witness.as_ref().map(|w| w.arg_hooks());
        let arm_from = self.f.byte_len();
        let window = crate::witness::rc::window_open();
        let lowered = self.arm_scope(|em| {
            let t = em.lower_host_leaf_op(leaf, args)?;
            if let Some(before) = hooks_before {
                em.witness_module_result(name, args, before);
            }
            Ok(t)
        });
        match lowered {
            Ok(t) => {
                crate::witness::rc::arm(window, arm_from, self.f.byte_len());
                Ok(Some(t))
            }
            Err(e) => {
                crate::witness::rc::window_dropped(window);
                Err(e)
            }
        }
    }

    fn lower_host_leaf_op(&mut self, leaf: HostLeaf, args: &[IrExpr]) -> Result<SliceTy, EmitError> {
        use crate::fs_meta::{
            OP_HTTP_FRAMED_RESPONSE, OP_HTTP_FRAMED_STATUS, OP_HTTP_FRAMED_TEXT, OP_HTTP_SERVE_BIND, OP_HTTP_SERVE_NEXT,
            OP_HTTP_SERVE_REPLY,
        };
        Ok(match (leaf, args) {
            (HostLeaf::Two(op), [a, b]) => {
                // The response leaf's frame is Bytes: its body cell is the
                // request's exact bytes (#4744). Both are the text block layout.
                let frame_ty = if op == OP_HTTP_FRAMED_RESPONSE { crate::bytes::BYTES } else { STR };
                self.fs_call_two((a, STR), (b, frame_ty), op)?;
                // The text and status leaves answer the response body as the
                // server sent it: decode it lossily, as native does (#4387). The
                // response leaf's answer keeps the raw body cell `body_bytes`
                // reads (http_framed.almd); the subprocess leaves are unchanged.
                let text = op == OP_HTTP_FRAMED_TEXT || op == OP_HTTP_FRAMED_STATUS;
                if text { self.fs_result_text_lossy()? } else { self.fs_result_string()? }
            }
            // bind (port text), next (the request as a List[String] of
            // frames), reply (the response's cells in a, its body's bytes in b).
            (HostLeaf::ServeBind, [p]) => {
                self.fs_call_1(p, OP_HTTP_SERVE_BIND)?;
                self.fs_result_unit()?
            }
            (HostLeaf::ServeNext, []) => {
                self.fs_call_0(OP_HTTP_SERVE_NEXT)?;
                self.fs_result_string_list()?
            }
            // The body crosses as Bytes in b, exactly (#4311).
            (HostLeaf::ServeReply, [r, body]) => {
                self.fs_call_two((r, STR), (body, crate::bytes::BYTES), OP_HTTP_SERVE_REPLY)?;
                self.fs_result_unit()?
            }
            // The contained loop (#4203): the step closure's env pointer in
            // the a_len slot; the host calls the closure itself.
            (HostLeaf::ServeRun, [step]) => {
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
                self.release_serve_step_result()?;
                self.fs_result_unit()?
            }
            (HostLeaf::CallOpen, _) => self.lower_http_call_open(args)?,
            (HostLeaf::CallId(op), [c]) => {
                self.http_call_id_op(c, op)?;
                if op == OP_HTTP_CALL_WAIT { self.fs_result_string_list()? } else { self.fs_result_string()? }
            }
            (HostLeaf::Net(op), _) => self.lower_net_leaf(op, args)?,
            _ => return unsup("host-leaf-arity"),
        })
    }

    /// The contained loop's answer (#4769): a served run hands back the step
    /// closure's own `Result[Unit, String]` block in the low word (high word
    /// 0), which is released here, and leaves `0` (ok) for `fs_result_unit`;
    /// a not-served answer (high word 1) is left as it is.
    fn release_serve_step_result(&mut self) -> Result<(), EmitError> {
        let uh = self.types.intern(SliceTy::Unit);
        let sh = self.types.intern(STR);
        let dec = self.dec_fn_of(SliceTy::Result(uh, sh));
        let hret = self.hold_i64()?;
        let hr = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_tee(hret).i64_const(32).i64_shr_u().i64_eqz().if_(BlockType::Empty);
        i.local_get(hret).i32_wrap_i64().local_tee(hr).if_(BlockType::Empty);
        i.local_get(hr).call(dec);
        i.end();
        i.i64_const(0).local_set(hret);
        i.end();
        i.local_get(hret);
        let _ = i;
        self.release_i32();
        self.release_i64();
        Ok(())
    }

    /// The `HttpCall` slice type and its id slot's payload offset.
    fn http_call_ty(&self) -> Result<(SliceTy, u32), EmitError> {
        let Some(t) = crate::ty::slice_ty_of(&almide_types::types::Ty::Named(almide_base::intern::sym("HttpCall"), Default::default()), self.types)
        else {
            return unsup("http-call-ty");
        };
        let SliceTy::Named(ti) = t else { return unsup("http-call-ty") };
        let crate::types_table::NamedDef::Record(r) = self.types.def(ti) else {
            return unsup("http-call-ty");
        };
        Ok((t, r.fields[0].offset))
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
        i.local_get(hc).i64_load(MemArg { offset: u64::from(almide_layout::PAYLOAD + off), align: 2, target: wasm_encoder::MemArgTarget::Memory(0), ordering: None });
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
        // The start frame is Bytes: its body cell is the request's exact
        // bytes (#4744).
        self.fs_call_two((url, STR), (frame, crate::bytes::BYTES), OP_HTTP_CALL_OPEN)?;
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
        i.i64_store(MemArg { offset: u64::from(almide_layout::PAYLOAD + off), align: 2, target: wasm_encoder::MemArgTarget::Memory(0), ordering: None });
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
