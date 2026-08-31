//! Module-surface call dispatch, split from `calls.rs` for the 800-line
//! file discipline: the three-stage `lower_module_call` chain (special
//! forms → qualified table → verified self-host whitelist). `calls.rs`
//! keeps the user-call/ctor/print lowerings and the linked-call tail.

use almide_ir::{CallTarget, IrExpr};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// Module-surface dispatch — special forms first, then the qualified
    /// table and the verified self-host whitelist. Split from
    /// `lower_call_at` for the complexity budget.
    pub(crate) fn lower_module_call(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
        tail: bool,
        ret_hint: Option<SliceTy>,
    ) -> Result<Option<SliceTy>, EmitError> {
        if let Some(out) = self.lower_string_ext(target, args)? {
            return Ok(out);
        }
        if let Some(out) = self.lower_scalar_ext(target, args)? {
            return Ok(out);
        }
        match target {
            CallTarget::Module { module, func, .. }
                if module.as_str() == "process" && func.as_str() == "exit" =>
            {
                // The abort floor (C-153 family): surface the code to the
                // host import, then trap. The host records the code BEFORE
                // the unwind, so exit-code parity is exact; the trailing
                // `unreachable` keeps the stack polymorphic (nothing after
                // `process.exit` executes on any target).
                match args.first() {
                    Some(a) => {
                        self.lower(a, Some(INT))?;
                        self.f.instructions().i32_wrap_i64();
                    }
                    None => {
                        self.f.instructions().i32_const(1);
                    }
                }
                self.f.instructions().call(F_EXIT_IMPORT).unreachable();
                Ok(None)
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "int" && func.as_str() == "to_string" && args.len() == 1 =>
            {
                self.lower(&args[0], Some(INT))?;
                self.f.instructions().call(F_INT_TO_STRING);
                Ok(Some(STR))
            }
            // Two-value i64 min/max — one select each.
            CallTarget::Module { module, func, .. }
                if module.as_str() == "int"
                    && matches!(func.as_str(), "max" | "min")
                    && args.len() == 2 =>
            {
                let is_max = func.as_str() == "max";
                self.lower(&args[0], Some(INT))?;
                let ha = self.hold_i64()?;
                self.f.instructions().local_set(ha);
                self.lower(&args[1], Some(INT))?;
                let hb = self.hold_i64()?;
                let mut i = self.f.instructions();
                i.local_set(hb);
                // select(v1, v2, cond) = cond ? v1 : v2
                i.local_get(ha).local_get(hb);
                i.local_get(ha).local_get(hb);
                if is_max {
                    i.i64_gt_s();
                } else {
                    i.i64_lt_s();
                }
                i.select();
                let _ = i;
                self.release_i64();
                self.release_i64();
                Ok(Some(INT))
            }
            // i64 → f64 is one wasm op; f64.convert_i64_s IS Rust's
            // `as f64` (IEEE round-to-nearest-even), bit-exact.
            CallTarget::Module { module, func, .. }
                if module.as_str() == "int" && func.as_str() == "to_float" && args.len() == 1 =>
            {
                self.lower(&args[0], Some(INT))?;
                self.f.instructions().f64_convert_i64_s();
                Ok(Some(FLOAT))
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string"
                    && matches!(func.as_str(), "len" | "length")
                    && args.len() == 1 =>
            {
                self.lower(&args[0], Some(STR))?;
                self.f.instructions().call(F_STR_LEN_CHARS);
                Ok(Some(INT))
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "json" && func.as_str() == "stringify" && args.len() == 1 =>
            {
                self.lower(&args[0], Some(SliceTy::Value))?;
                self.emit_value_stringify()?;
                Ok(Some(STR))
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "json"
                    && func.as_str() == "stringify_pretty"
                    && args.len() == 1 =>
            {
                self.lower(&args[0], Some(SliceTy::Value))?;
                self.emit_value_stringify_pretty()?;
                Ok(Some(STR))
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "json"
                    && func.as_str() == "set_path"
                    && args.len() == 3 =>
            {
                let h = self.work.helper(crate::work::Helper::JsonPathSet);
                self.lower(&args[0], Some(SliceTy::Value))?;
                self.lower(&args[1], None)?;
                self.f.instructions().i32_const(0);
                self.lower(&args[2], Some(SliceTy::Value))?;
                self.f.instructions().call(h);
                // ok(v) — the surface is Result[Value, String], always ok.
                let hv = self.tmp_i32_local;
                let mut i = self.f.instructions();
                i.local_set(hv);
                i.i32_const(16)
                    .call(F_ALLOC)
                    .local_tee(self.scr_i32_local)
                    .i32_const(0)
                    .i32_store(slot_memarg(almide_layout::SUM_TAG));
                i.local_get(self.scr_i32_local)
                    .local_get(hv)
                    .i32_store(slot_memarg(almide_layout::SUM_FIELD));
                i.local_get(self.scr_i32_local);
                let _ = i;
                let vh = self.types.intern(SliceTy::Value);
                Ok(Some(SliceTy::Result(vh, self.types.intern(STR))))
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "json" && func.as_str() == "to_map" && args.len() == 1 =>
            {
                self.lower_json_to_map(&args[0])
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "json"
                    && func.as_str() == "remove_path"
                    && args.len() == 2 =>
            {
                let h = self.work.helper(crate::work::Helper::JsonPathRemove);
                self.lower(&args[0], Some(SliceTy::Value))?;
                self.lower(&args[1], None)?;
                self.f.instructions().i32_const(0);
                self.f.instructions().call(h);
                Ok(Some(SliceTy::Value))
            }
            _ => self.lower_module_call_b(target, args, tail, ret_hint),
        }
    }

    /// Module dispatch, second third (mechanical split — first-match
    /// order preserved).
    pub(crate) fn lower_module_call_b(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
        tail: bool,
        ret_hint: Option<SliceTy>,
    ) -> Result<Option<SliceTy>, EmitError> {
        match target {
            CallTarget::Module { module, func, .. }
                if (module.as_str() == "option" || module.as_str() == "result")
                    && func.as_str() == "unwrap_or"
                    && args.len() == 2 =>
            {
                let got = self.lower(&args[0], None)?;
                match got {
                    SliceTy::Option(h) => {
                        let et = self.types.el(h);
                        self.f
                            .instructions()
                            .local_tee(self.scr_i32_local)
                            .i32_eqz()
                            .if_(BlockType::Result(et.val_type()));
                        self.lower(&args[1], Some(et))?;
                        self.f.instructions().else_().local_get(self.scr_i32_local);
                        self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                        self.f.instructions().end();
                        Ok(Some(et))
                    }
                    SliceTy::Result(o, _) => {
                        let et = self.types.el(o);
                        self.f
                            .instructions()
                            .local_tee(self.scr_i32_local)
                            .i32_load(slot_memarg(almide_layout::SUM_TAG))
                            .i32_const(0)
                            .i32_ne()
                            .if_(BlockType::Result(et.val_type()));
                        self.lower(&args[1], Some(et))?;
                        self.f.instructions().else_().local_get(self.scr_i32_local);
                        self.load_ty_slot(et, almide_layout::SUM_FIELD);
                        self.f.instructions().end();
                        Ok(Some(et))
                    }
                    other => unsup(&format!("unwrap-or-of:{other:?}")),
                }
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "matrix" => {
                if let Some(out) = self.lower_matrix_call(func.as_str(), args)? {
                    return Ok(out);
                }
                unsup(&format!("call:matrix.{func}"))
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "fan" => {
                if let Some(out) = self.lower_fan_call(func.as_str(), args)? {
                    return Ok(out);
                }
                unsup(&format!("call:fan.{func}"))
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "fs" => {
                if let Some(out) = self.lower_fs_call(func.as_str(), args)? {
                    return Ok(out);
                }
                unsup(&format!("call:fs.{func}"))
            }
            CallTarget::Module { module, func, .. }
                if matches!(module.as_str(), "env" | "io" | "process") =>
            {
                if let Some(out) = self.lower_host_call(module.as_str(), func.as_str(), args)? {
                    return Ok(out);
                }
                unsup(&format!("call:{module}.{func}"))
            }
            // http (#1710): the ported client fns take the host route; the
            // REST of the family keeps its pre-port path (self-host /
            // honest wall) — walling here regressed the pure constructors
            // (http.response measured lower before the port).
            CallTarget::Module { module, func, .. } if module.as_str() == "http" => {
                if let Some(out) = self.lower_host_call("http", func.as_str(), args)? {
                    return Ok(out);
                }
                // The family's pre-port route verbatim (`_ =>` below): the
                // self-host registry serves the pure fns (url_decode, the
                // response constructors) exactly as before the port.
                self.lower_module_call_c(target, args, tail, ret_hint)
            }
            // #1423 stage 4: the error trio — semantics verbatim from
            // runtime/rs/src/error.rs.
            CallTarget::Module { module, func, .. } if module.as_str() == "error" => {
                if let Some(out) = self.lower_error_call(func.as_str(), args)? {
                    return Ok(out);
                }
                unsup(&format!("call:error.{func}"))
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "value" => {
                if let Some(out) = self.lower_value_call(func.as_str(), args)? {
                    return Ok(out);
                }
                unsup(&format!("call:value.{func}"))
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "bytes" => {
                self.lower_bytes_call(func.as_str(), args)
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string" && func.as_str() == "split" && args.len() == 2 =>
            {
                self.lower_string_split(&args[0], &args[1])
            }
            _ => self.lower_module_call_c(target, args, tail, ret_hint),
        }
    }

    /// Module dispatch, final third + the qualified fallback.
    pub(crate) fn lower_module_call_c(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
        tail: bool,
        ret_hint: Option<SliceTy>,
    ) -> Result<Option<SliceTy>, EmitError> {
        match target {
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string"
                    && func.as_str() == "slice"
                    && (args.len() == 2 || args.len() == 3) =>
            {
                self.lower(&args[0], Some(STR))?;
                self.lower(&args[1], Some(INT))?;
                if let Some(e) = args.get(2) {
                    self.lower(e, Some(INT))?;
                } else {
                    // the surface's `end` default: i64::MAX ("to the end")
                    self.f.instructions().i64_const(i64::MAX);
                }
                self.f.instructions().call(F_STR_SLICE);
                Ok(Some(STR))
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string" && func.as_str() == "repeat" && args.len() == 2 =>
            {
                self.lower(&args[0], Some(STR))?;
                self.lower(&args[1], Some(INT))?;
                self.f.instructions().call(F_STR_REPEAT);
                Ok(Some(STR))
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "list" => {
                self.lower_list_call(func.as_str(), args, ret_hint)
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "map" => {
                self.lower_map_call(func.as_str(), args, ret_hint)
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "set" => {
                self.lower_set_call(func.as_str(), args, ret_hint)
            }
            CallTarget::Module { module, func, .. } if module.as_str() == "prim" => {
                self.lower_prim_call(func.as_str(), args)
            }
            CallTarget::Module { module, func, .. } => {
                // The option/result intrinsic combinator matrix first —
                // its source-level siblings (flatten, to_list, zip on the
                // result side, …) fall through to the linked path below.
                if matches!(module.as_str(), "option" | "result")
                    && let Some(out) =
                        self.lower_sum_combinator(module.as_str(), func.as_str(), args)?
                {
                    return Ok(out);
                }
                self.lower_linked_call(module.as_str(), func.as_str(), args, tail)
            }
            _ => unreachable!("module dispatch"),
        }
    }
}
