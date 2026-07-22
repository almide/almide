impl LowerCtx {

    /// Extracted from `Self::lower_prim_call_fs_env` (twelfth-round split, cog
    /// reduction): the WASI filesystem-floor name group, verbatim (only ever called
    /// for a name in the caller's matching `matches!` set).
    fn lower_prim_call_fs(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<ValueId>, LowerError> {
        use crate::PrimKind;
        // `prim.read_text_file(path)` — the WASI file-read floor (fs.read_text). ONE
        // BORROWED `String` arg (the path; the caller still owns it). Its dst is a FRESH
        // OWNED `Result[String, String]` built by the render in the EXACT
        // `materialize_result_str` cap-as-tag layout (1-slot DynListStr, payload @12, tag
        // @16). Tracked like a heap-Ok Result: `materialized_results_str` so a downstream
        // `match`/`!` reads tag @16, AND `heap_elem_lists` so the heap-payload bind gates open
        // AND the scope-end drop is the flat `DropListStr` (frees the one owned String @12 +
        // the block — a flat `Drop` would leak the String). Carries Capability::FsRead
        // (counted in cap_witness). The render emits the WASI path_open/fd_read sequence.
        if func == "read_text_file" || func == "read_bytes_file" {
            // read_bytes_file is the raw-bytes twin: the SAME WASI floor + Result block
            // (the render's $read_text_file reads raw bytes; only the almd-level Ok TYPE
            // differs), so one PrimKind serves both.
            let path = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.read_text_file path is not a lowerable scalar/handle".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::ReadTextFile, dst: Some(dst), args: vec![path] });
            self.materialized_results_str.insert(dst);
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.read_dir(path)` — the WASI directory-listing floor (fs.list_dir). ONE BORROWED
        // `String` arg (the path). Its dst is a FRESH OWNED `Result[List[String], String]` built
        // by the render ($read_dir) in the cap-as-tag layout (1-slot wrapper, payload @12 = a
        // List[String], tag @16). Tracked like a heap-Ok Result: `materialized_results_str` so a
        // downstream `match`/`!` reads tag @16, AND `heap_elem_lists` so the heap-payload bind
        // gates open, AND `list_str_result_results` so the scope-end drop is the RECURSIVE
        // `DropResultListStr` (frees the payload List's element Strings + block; a flat
        // `DropListStr` would leak them) — checked BEFORE heap_elem_lists in `drop_op_for`.
        // Carries Capability::FsRead (counted in cap_witness). The render emits the WASI
        // path_open(O_DIRECTORY)/fd_readdir sequence (skip `.`/`..`, sort, build the list).
        if func == "read_dir" {
            let path = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.read_dir path is not a lowerable scalar/handle".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::ReadDir, dst: Some(dst), args: vec![path] });
            self.materialized_results_str.insert(dst);
            self.heap_elem_lists.insert(dst);
            self.list_str_result_results.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.write_text_file(path, content)` — the WASI file-WRITE floor (fs.write). TWO
        // BORROWED `String` args (the path + the content; the caller still owns both). Its dst is a
        // FRESH OWNED `Result[Unit, String]` built by the render ($write_text_file): Ok(()) with
        // `len@4 = 0` (no payload String — the `materialize_result_ok` convention) so the scope-end
        // flat `DropListStr` frees nothing at @12, or Err(msg) with `len@4 = 1` + `@12 = msg` (the
        // flat drop frees the one owned message). Tracked like a heap Result: `materialized_results_str`
        // so a downstream `match`/`!` reads the @16 tag, AND `heap_elem_lists` so the heap-payload
        // bind gates open AND the scope-end drop is the flat `DropListStr` (sound for BOTH arms given
        // the `len@4 = 0` Ok convention — NO `list_str_result_results`: there is no nested payload).
        // Carries Capability::FsWrite (counted in cap_witness). The render emits the WASI
        // path_open(O_CREAT|O_TRUNC)/fd_write sequence.
        if func == "write_text_file" {
            let path = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.write_text_file path is not a lowerable scalar/handle".into())
            })?;
            let content = self.lower_scalar_value(&args[1]).ok_or_else(|| {
                LowerError::Unsupported("prim.write_text_file content is not a lowerable scalar/handle".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::WriteTextFile,
                dst: Some(dst),
                args: vec![path, content],
            });
            self.materialized_results_str.insert(dst);
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.make_dir(path)` — the WASI directory-CREATE floor (fs.mkdir_p). ONE BORROWED
        // `String` arg (the path; the caller still owns it). Its dst is a FRESH OWNED
        // `Result[Unit, String]` built by the render ($make_dir): Ok(()) with `len@4 = 0` (no
        // payload String — the `materialize_result_ok` convention, IDENTICAL to write_text_file's
        // Ok arm) so the scope-end flat `DropListStr` frees nothing at @12, or Err(msg) with
        // `len@4 = 1` + `@12 = msg` (the flat drop frees the one owned message). Tracked exactly
        // like write_text_file's heap Result: `materialized_results_str` so a downstream `match`/`!`
        // reads the @16 tag, AND `heap_elem_lists` so the heap-payload bind gates open AND the
        // scope-end drop is the flat `DropListStr`. Carries Capability::FsWrite (a mkdir IS a
        // filesystem write — counted in cap_witness). The render emits the WASI recursive
        // path_create_directory sequence.
        if func == "make_dir" {
            let path = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.make_dir path is not a lowerable scalar/handle".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::MakeDir,
                dst: Some(dst),
                args: vec![path],
            });
            self.materialized_results_str.insert(dst);
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.remove_all(path)` — the WASI recursive-remove floor (fs.remove_all). ONE BORROWED
        // `String` arg (the path; the caller still owns it). Its dst is a FRESH OWNED
        // `Result[Unit, String]` built by the render ($remove_all): Ok(()) with `len@4 = 0` (no
        // payload String — the `materialize_result_ok` convention, IDENTICAL to make_dir's Ok arm)
        // so the scope-end flat `DropListStr` frees nothing at @12, or Err(msg) with `len@4 = 1` +
        // `@12 = msg` (the flat drop frees the one owned message). Tracked exactly like make_dir's
        // heap Result: `materialized_results_str` so a downstream `match`/`!` reads the @16 tag, AND
        // `heap_elem_lists` so the heap-payload bind gates open AND the scope-end drop is the flat
        // `DropListStr`. Carries Capability::FsWrite (a recursive remove IS a filesystem write —
        // counted in cap_witness). The render emits the WASI recursive
        // path_remove_directory/path_unlink_file sequence.
        if func == "remove_all" {
            let path = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.remove_all path is not a lowerable scalar/handle".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::RemoveAll,
                dst: Some(dst),
                args: vec![path],
            });
            self.materialized_results_str.insert(dst);
            self.heap_elem_lists.insert(dst);
            return Ok(Some(dst));
        }
        // `prim.path_exists(path)` — the WASI path-stat floor (fs.exists). ONE BORROWED `String`
        // arg (the path; the caller still owns it). Its dst is a SCALAR `Bool` (i64 0/1) — UNLIKE
        // every other fs prim, a stat allocates NO heap result, so the dst is tracked in NO
        // classification set (no `materialized_results_str` / `heap_elem_lists`): it is a plain
        // scalar with no scope-end drop and no ownership-cert `i`. Carries Capability::FsRead (a
        // stat IS a filesystem read — counted in cap_witness). The render emits the WASI
        // path_filestat_get query (errno 0 = exists).
        // `prim.path_filestat(bufaddr, path)` — the WASI FULL-stat floor (fs.stat). TWO args: a
        // raw scratch ADDRESS (an i64 scalar — the self-host's own Bytes data region, so the
        // caller owns the buffer) and a BORROWED `String` path. dst = the SCALAR errno (0 = the
        // 64-byte WASI filestat is at bufaddr). Like path_exists this allocates NO heap result —
        // the dst joins no classification set. Carries Capability::FsRead (counted in cap_witness).
        if func == "path_filestat" {
            let bufaddr = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported(
                    "prim.path_filestat buffer address is not a lowerable scalar".into(),
                )
            })?;
            let path = self.lower_scalar_value(&args[1]).ok_or_else(|| {
                LowerError::Unsupported(
                    "prim.path_filestat path is not a lowerable scalar/handle".into(),
                )
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::PathFilestat,
                dst: Some(dst),
                args: vec![bufaddr, path],
            });
            return Ok(Some(dst));
        }
        if func == "path_exists" {
            let path = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported("prim.path_exists path is not a lowerable scalar/handle".into())
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::PathExists,
                dst: Some(dst),
                args: vec![path],
            });
            return Ok(Some(dst));
        }
        unreachable!("lower_prim_call_fs called with a name outside its caller-matched set: {func}")
    }

    /// Extracted from `Self::lower_prim_call` (eleventh-round split, cog reduction): the
    /// pointer-cast/stdin-read name group, verbatim (only ever called for a name in the
    /// router's matching `matches!` set, so no "unrecognized name" fallthrough is needed).
    fn lower_prim_call_ptr_io(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<ValueId>, LowerError> {
        use crate::PrimKind;
        // `prim.ptr_to_int` / `prim.int_to_ptr` — REINTERPRET casts (identity at the
        // value level: the RawPtr IS the i64 address). No op emitted — the operand's
        // ValueId passes through, so the cert sees nothing (a pure hat-swap).
        if func == "ptr_to_int" || func == "int_to_ptr" {
            let v = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported(
                    "prim ptr cast operand is not a lowerable scalar".into(),
                )
            })?;
            return Ok(Some(v));
        }
        // `prim.read_line()` — the WASI stdin-line floor (io.read_line). NO args. Its dst is a
        // FRESH OWNED canonical `String` (one line of stdin, newline excluded) built by the render
        // ($read_line). A plain String owns NO nested handles, so it is tracked in NO classification
        // set — its scope-end drop (if not moved out as a return) is the flat `Op::Drop` that frees
        // the block (a `DropListStr` would WRONGLY treat the byte payload as i64 element handles).
        // Carries Capability::Stdin (counted in cap_witness). The render emits the byte-by-byte
        // fd_read-from-fd-0 sequence.
        if func == "read_line" {
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::ReadLine,
                dst: Some(dst),
                args: vec![],
            });
            return Ok(Some(dst));
        }
        // `prim.read_n_bytes(n)` — the WASI stdin-N-bytes floor (io.read_n_bytes). The n arg is a
        // scalar Int (byte count); dst is a FRESH OWNED `Bytes` block (byte-buffer layout, built by the
        // preamble `$read_n_bytes`). Carries Capability::Stdin (counted via certificate.rs). Like
        // read_line, a plain Bytes owns no nested handles, so its scope-end drop is the flat `Op::Drop`.
        if func == "read_n_bytes" {
            let n = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported(
                    "prim.read_n_bytes needs a scalar Int byte count not in this brick".into(),
                )
            })?;
            let dst = self.fresh_value();
            self.ops.push(Op::Prim {
                kind: PrimKind::ReadNBytes,
                dst: Some(dst),
                args: vec![n],
            });
            return Ok(Some(dst));
        }
        unreachable!("lower_prim_call_ptr_io called with a name outside its router-matched set: {func}")
    }

    /// Extracted from `Self::lower_prim_call_generic` (twelfth-round split, cog
    /// reduction): the load/store/rc/misc-floor half of the name → `PrimKind` table,
    /// verbatim (only ever called for a name in the caller's matching `matches!` set).
    fn prim_kind_structural(&self, func: &str) -> Result<crate::PrimKind, LowerError> {
        if matches!(func, "handle" | "die" | "load8" | "load32" | "load64" | "load_str" | "load_handle" | "store32" | "store8" | "store64") {
            return Ok(Self::prim_kind_load_store(func));
        }
        self.prim_kind_rc_io(func)
    }

    /// Extracted from `Self::prim_kind_structural` (thirteenth-round split, cog
    /// reduction): the handle/die/load/store sub-table, verbatim (a pure lookup, only
    /// ever called for a name in the caller's matching `matches!` set).
    fn prim_kind_load_store(func: &str) -> crate::PrimKind {
        use crate::PrimKind;
        match func {
            "handle" => PrimKind::Handle,
            "die" => PrimKind::Die,
            "load8" => PrimKind::Load { width: 1 },
            "load32" => PrimKind::Load { width: 4 },
            "load64" => PrimKind::Load { width: 8 },
            // Load a 4-byte handle KEEPING Ptr repr — reads a String element out of a list slot
            // (a borrow of the slot's String, for passing to a closure / String fn).
            "load_str" => PrimKind::LoadHandle,
            // Generic typed `load_handle[A]` — the same i32-handle-keeping load as `load_str`, for
            // reading a `List[Value]`/`Value` payload out of a Value's slot (the Value model floor).
            "load_handle" => PrimKind::LoadHandle,
            "store32" => PrimKind::Store { width: 4 },
            "store8" => PrimKind::Store { width: 1 },
            "store64" => PrimKind::Store { width: 8 },
            _ => unreachable!("prim_kind_load_store called with a name outside its caller-matched set: {func}"),
        }
    }

    /// Extracted from `Self::prim_kind_structural` (thirteenth-round split, cog
    /// reduction): the rc_dec/rc_inc/fd_write/random/clock sub-table, verbatim (only
    /// ever called for a name outside `prim_kind_load_store`'s set).
    fn prim_kind_rc_io(&self, func: &str) -> Result<crate::PrimKind, LowerError> {
        use crate::PrimKind;
        Ok(match func {
            // Raw refcount free/acquire — the Value drop/copy mechanism. GATED to the value-model
            // self-host fns (the trusted recursive-free / shallow-copy, like the inline DropListStr):
            // an UNTRACKED free exposed to arbitrary code would let any fn double-free outside the
            // ownership cert's sight, so only the value-model drop/copy routines may name it: the
            // recursive drop (`__drop_value`, rc_dec), the array shallow-copy (`__varr_copy`, rc_inc),
            // the as_array element-list fill (`__vfill`, rc_inc), and the heap-element list-concat copy
            // (`__lc_copy_rc`, rc_inc — the new list co-owns each appended element, balanced by the
            // source's recursive DropListStr/DropListValue). See docs/roadmap/active/v1-value-model.md.
            //
            // TRUST GROUNDING (柱C Brick 3): these names are a CO-OWN-PRODUCER / RECURSIVE-DROP whitelist
            // — a producer (`__varr_copy`/`__vobj_fill`/`__copy_value`/`__lc_copy_rc`/…) rc_inc's each
            // loaded element (+1) into a fresh container; its balancing rc_dec lives in the SEPARATE
            // recursive drop (`__drop_value`/`__vdrop_arr`/…) over the SAME elements. That cross-loop,
            // element-count-keyed balance is PROVEN leak/double-free-free on the Coq kernel by
            // proofs/CoownLoop.v (`coown_fill_drop_neutral` ⇒ `coown_copy_no_leak` + `…no_double_free`,
            // in the check.sh proof gate). So this gate is no longer bare trust: a name belongs here iff
            // it is a co-own producer or recursive-drop consumer following that proven pattern, and its
            // adherence is ratcheted by the spec/wasm_cross/*_leak_loop fixtures. Cert-PROVING each
            // producer per-function (retiring the whitelist) needs the typed nested-element model + the
            // cross-function fill↔drop pairing that consumes CoownLoop.v — the remaining Brick-3
            // engineering (docs/roadmap/active/value-rc-cert.md).
            // The co-own producer / recursive-drop whitelist lives in ONE shared anchor
            // (crate::coown_names) grounded in proofs/CoownLoop.v + CoownCompose.v — see that module.
            "rc_dec" | "rc_inc"
                if crate::coown_names::is_coown_rc_routine(self.fn_name.as_str())
                    || self.fn_name.starts_with("__drop_")
                    // `__krec_uniqfill_<R>` — the GENERATED list.unique fill over a
                    // String-field-record element (C-015): rc_inc each KEPT element
                    // into the result (the __uh_acquire pattern, per-type generated
                    // like __drop_*; drop_sources.rs is the single emitter).
                    || self.fn_name.starts_with("__krec_uniqfill_") =>
            {
                // `__drop_*` also covers the GENERATED per-type custom-variant recursive drops
                // (`__drop_Expr`, ADT brick 5b) — the same trusted prim-only free routine.
                if func == "rc_dec" { PrimKind::RcDec } else { PrimKind::RcInc }
            }
            "rc_dec" | "rc_inc" => {
                return Err(LowerError::Unsupported(format!(
                    "prim.{func} is restricted to the value-model drop/copy routines (untracked free)"
                )))
            }
            "fd_write" => PrimKind::FdWrite,
            "random_get" => PrimKind::RandomGet,
            "clock_time_get" => PrimKind::ClockTimeGet,
            _ => unreachable!("prim_kind_structural called with a name outside its caller-matched set: {func}"),
        })
    }

    /// Extracted from `Self::lower_prim_call_generic` (twelfth-round split, cog
    /// reduction): the FLOAT-floor half of the name → `PrimKind` table, verbatim (a
    /// pure lookup, only ever called for a name in the caller's matching `matches!`
    /// set — total, so no `Result`/`Option` needed).
    fn prim_kind_float(func: &str) -> crate::PrimKind {
        if matches!(
            func,
            "fabs" | "fsqrt" | "ffloor" | "fceil" | "fneg" | "fadd" | "fsub" | "fmul" | "fdiv"
                | "fmin" | "fmax" | "fcopysign"
        ) {
            return Self::prim_kind_float_arith(func);
        }
        Self::prim_kind_float_cmp_conv(func)
    }

    /// Extracted from `Self::prim_kind_float` (thirteenth-round split, cog reduction):
    /// the float unary/binary-arithmetic sub-table, verbatim (a pure lookup, only ever
    /// called for a name in the caller's matching `matches!` set).
    fn prim_kind_float_arith(func: &str) -> crate::PrimKind {
        use crate::PrimKind;
        match func {
            // The FLOAT floor (the f64 bits live in the i64-uniform value; render reinterprets).
            "fabs" => PrimKind::FloatUn(crate::FUnOp::Abs),
            "fsqrt" => PrimKind::FloatUn(crate::FUnOp::Sqrt),
            "ffloor" => PrimKind::FloatUn(crate::FUnOp::Floor),
            "fceil" => PrimKind::FloatUn(crate::FUnOp::Ceil),
            "fneg" => PrimKind::FloatUn(crate::FUnOp::Neg),
            "fadd" => PrimKind::FloatBin(crate::FBinOp::Add),
            "fsub" => PrimKind::FloatBin(crate::FBinOp::Sub),
            "fmul" => PrimKind::FloatBin(crate::FBinOp::Mul),
            "fdiv" => PrimKind::FloatBin(crate::FBinOp::Div),
            "fmin" => PrimKind::FloatBin(crate::FBinOp::Min),
            "fmax" => PrimKind::FloatBin(crate::FBinOp::Max),
            "fcopysign" => PrimKind::FloatBin(crate::FBinOp::CopySign),
            _ => unreachable!("prim_kind_float_arith called with a name outside its caller-matched set: {func}"),
        }
    }

    /// Extracted from `Self::prim_kind_float` (thirteenth-round split, cog reduction):
    /// the float comparison/conversion sub-table, verbatim (only ever called for a name
    /// outside `prim_kind_float_arith`'s set).
    fn prim_kind_float_cmp_conv(func: &str) -> crate::PrimKind {
        use crate::PrimKind;
        match func {
            "flt" => PrimKind::FloatCmp(crate::FCmpOp::Lt),
            "fle" => PrimKind::FloatCmp(crate::FCmpOp::Le),
            "fgt" => PrimKind::FloatCmp(crate::FCmpOp::Gt),
            "fge" => PrimKind::FloatCmp(crate::FCmpOp::Ge),
            "feq" => PrimKind::FloatCmp(crate::FCmpOp::Eq),
            "fne" => PrimKind::FloatCmp(crate::FCmpOp::Ne),
            "f2i" => PrimKind::FloatToInt,
            "i2f" => PrimKind::IntToFloat,
            "fbits" | "ffrombits" => PrimKind::FloatBits,
            // f32 narrowing/widening (f32 value = its 32-bit pattern in the low half of the i64).
            "f2f32" => PrimKind::F32Demote,
            // `f32_2f` (Float32→Float) and `bits_to_f32` (raw 32-bit pattern→Float) are the SAME
            // f64.promote_f32 over a low-32 f32 pattern.
            "f32_2f" | "bits_to_f32" => PrimKind::F32Promote,
            "i2f32" => PrimKind::IntToF32,
            "f32bits" => PrimKind::F32Bits,
            _ => unreachable!("prim_kind_float_cmp_conv called with a name outside its caller-matched set: {func}"),
        }
    }

    fn lower_prim_call_generic(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<ValueId>, LowerError> {
        // Bitwise binary ops lower to a scalar `Op::IntBinOp` (i64 and/or/xor/shl/shr_s),
        // not an `Op::Prim` — the int.band/bor/bxor/bshl/bshr floor. No ownership.
        if let Some(op) = Self::bitop_for_name(func) {
            return self.emit_prim_bitop(func, op, args);
        }
        let kind = self.prim_kind_for_name(func)?;
        self.emit_prim_call(func, args, kind)
    }

    /// Extracted from `Self::lower_prim_call_generic` (thirteenth-round split, cog
    /// reduction): the bitwise-op name → `IntOp` lookup, verbatim (a pure lookup, no
    /// `&self` needed).
    fn bitop_for_name(func: &str) -> Option<crate::IntOp> {
        match func {
            "band" => Some(crate::IntOp::And),
            "bor" => Some(crate::IntOp::Or),
            "bxor" => Some(crate::IntOp::Xor),
            "bshl" => Some(crate::IntOp::Shl),
            "bshr" => Some(crate::IntOp::Shr),
            "bshr_u" => Some(crate::IntOp::ShrU),
            _ => None,
        }
    }

    /// Extracted from `Self::lower_prim_call_generic` (thirteenth-round split, cog
    /// reduction): the bitwise-op emission, verbatim.
    fn emit_prim_bitop(&mut self, func: &str, op: crate::IntOp, args: &[IrExpr]) -> Result<Option<ValueId>, LowerError> {
        let a = self.lower_scalar_value(&args[0]).ok_or_else(|| {
            LowerError::Unsupported(format!("prim.{func} arg 0 is not a lowerable scalar"))
        })?;
        let b = self.lower_scalar_value(&args[1]).ok_or_else(|| {
            LowerError::Unsupported(format!("prim.{func} arg 1 is not a lowerable scalar"))
        })?;
        let dst = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst, op, a, b });
        Ok(Some(dst))
    }

    /// Extracted from `Self::lower_prim_call_generic` (thirteenth-round split, cog
    /// reduction): the structural-vs-float name-group dispatch, verbatim.
    fn prim_kind_for_name(&self, func: &str) -> Result<crate::PrimKind, LowerError> {
        if matches!(
            func,
            "handle" | "die" | "load8" | "load32" | "load64" | "load_str" | "load_handle"
                | "store32" | "store8" | "store64" | "rc_dec" | "rc_inc" | "fd_write"
                | "random_get" | "clock_time_get"
        ) {
            return self.prim_kind_structural(func);
        }
        if matches!(
            func,
            "fabs" | "fsqrt" | "ffloor" | "fceil" | "fneg" | "fadd" | "fsub" | "fmul" | "fdiv"
                | "fmin" | "fmax" | "fcopysign" | "flt" | "fle" | "fgt" | "fge" | "feq" | "fne"
                | "f2i" | "i2f" | "fbits" | "ffrombits" | "f2f32" | "f32_2f" | "bits_to_f32"
                | "i2f32" | "f32bits"
        ) {
            return Ok(Self::prim_kind_float(func));
        }
        Err(LowerError::Unsupported(format!("unknown primitive prim.{func}")))
    }

    /// Extracted from `Self::lower_prim_call_generic` (thirteenth-round split, cog
    /// reduction): the generic arg-lowering loop + final `Op::Prim` emission, verbatim.
    /// Extracted from `Self::emit_prim_call` (fourteenth-round split, cog reduction):
    /// the per-arg lowering loop BODY (one iteration), verbatim.
    fn lower_prim_call_one_arg(&mut self, func: &str, a: &IrExpr, kind: crate::PrimKind) -> Result<ValueId, LowerError> {
        use crate::PrimKind;
        // A STRING-LITERAL argument to `prim.handle` — the frontend's single-use
        // let-inliner pushes `let tbl = "…"; prim.handle(tbl)` into
        // `prim.handle("…")` (the generated case-mapping tables). Materialize
        // the literal block exactly as its let-bound form would (owned Alloc,
        // scope-end drop) and hand the prim its handle — the scalar-tail
        // deferred-Const fallback was silently returning 0 as the address.
        if matches!(kind, PrimKind::Handle) {
            if let IrExprKind::LitStr { value } = &a.kind {
                let dst = self.fresh_value();
                self.ops.push(Op::Alloc {
                    dst,
                    repr: repr_of(&a.ty)?,
                    init: crate::Init::Str(value.clone()),
                });
                self.live_heap_handles.push(dst);
                return Ok(dst);
            }
            // A COMPUTED String argument (`prim.die(prim.handle("assertion failed: "
            // + msg))` — the 2-arg assert's computed-message die): materialize the
            // concat/interp chain to an owned block (scope-tracked, dropped at the
            // arm/scope end like the literal above) and hand the prim its handle.
            // Without this the whole assert's unit-if rolled back and the wall
            // (misleadingly) named the CONDITION.
            if matches!(
                &a.kind,
                IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. }
                    | IrExprKind::StringInterp { .. }
            ) {
                let obj = match &a.kind {
                    IrExprKind::BinOp { .. } => self.try_lower_concat_str(a),
                    IrExprKind::StringInterp { parts } => self.try_lower_string_interp(parts),
                    _ => unreachable!(),
                };
                if let Some(obj) = obj {
                    self.live_heap_handles.push(obj);
                    return Ok(obj);
                }
            }
        }
        self.lower_scalar_value(a).ok_or_else(|| {
            LowerError::Unsupported(format!("prim.{func} argument is not a lowerable scalar/handle"))
        })
    }

    fn emit_prim_call(&mut self, func: &str, args: &[IrExpr], kind: crate::PrimKind) -> Result<Option<ValueId>, LowerError> {
        use crate::PrimKind;
        let mut lowered = Vec::with_capacity(args.len());
        for a in args {
            lowered.push(self.lower_prim_call_one_arg(func, a, kind)?);
        }
        let dst = if matches!(kind, PrimKind::Store { .. } | PrimKind::RcDec | PrimKind::RcInc | PrimKind::Die | PrimKind::ProcExit) {
            None
        } else {
            Some(self.fresh_value())
        };
        // `prim.load_str` (LoadHandle) yields a BORROW of a list slot's String — the list still owns
        // it. Mark the result BORROWED so a `let` binding does not add it to the scope-end drop set
        // (that would double-free with the owning list's DropListStr).
        if matches!(kind, PrimKind::LoadHandle) {
            if let Some(d) = dst {
                self.param_values.insert(d);
            }
        }
        self.ops.push(Op::Prim { kind, dst, args: lowered });
        Ok(dst)
    }

    /// Extracted from `Self::materialized_call_arg` (codopsy7 max-depth sweep): the
    /// mutually-exclusive drop-route selection for a fresh heap call-argument temp,
    /// verbatim — the original `if/else if` chain rewritten as independent
    /// `if COND { ...; return; }` guards (same order, same first-match-wins semantics,
    /// pure control-flow rewrite). Was nested one level deeper than the sibling bind-site
    /// routers (`seed_call_named_heap_drop_route` et al.) because it lives inside the
    /// caller's `if repr.is_heap() { .. }` — extracting it to its own `&mut self` method
    /// resets the naive depth counter to 1 for every arm.
    fn seed_call_arg_heap_drop_route(&mut self, dst: ValueId, ty: &Ty) {
        // A `value.as_array(v) ?? []` arg temp (the materialized `??` operand) is a
        // Result[List[Value],String] that OWNS its inner list — its drop must free the list AND
        // its element Values RECURSIVELY (`DropResultListValue`); the flat `heap_elem_lists`
        // fallback would only rc_dec the inner-list handle, LEAKING the element Values (a loop
        // OOMs). Checked BEFORE is_heap_elem_list_ty, which also matches this Result type.
        // (A Result[Value,String]'s Ok Value is CO-OWNED — value.get Dup's the object's slot, which
        // keeps its ref — so the flat rc_dec drop is correct there; a recursive free would
        // double-free the still-referenced slot. So only the list case is reclassified here.)
        if crate::lower::is_result_listval_ty(ty) {
            self.value_result_lists.insert(dst);
            return;
        }
        if crate::lower::is_list_list_str_ty(ty) {
            self.list_list_str_lists.insert(dst);
            return;
        }
        if crate::lower::is_list_str_str_ty(ty) {
            // `List[(String,String)]` (map.entries) arg temp — DropListStrStr frees each tuple's
            // two Strings; the flat heap_elem_lists fallback would leak them.
            self.str_str_elem_lists.insert(dst);
            return;
        }
        if crate::lower::is_lenlist_list_ty(ty) {
            self.variant_drop_handles.insert(dst, "list_lenlist".to_string());
            return;
        }
        if crate::lower::is_map_fn_ty(ty) {
            // `Map[String, <Fn>]` arg temp — `$__drop_map_mclo` frees each value via
            // `__drop_closure` (a flat sweep would leak every captured env slot).
            self.variant_drop_handles.insert(dst, "map_mclo".to_string());
            return;
        }
        if let Some(hname) = self.map_named_value_drop(ty) {
            self.variant_drop_handles.insert(dst, hname);
            return;
        }
        if crate::lower::is_map_msv_ty(ty) {
            // `Map[String, Map[String, String]]` arg temp (the inline nested-map literal
            // fed straight to `map.get_or` — map_fold_heap_acc's r7): `$__drop_map_msv`
            // sweeps each last-ref inner map; the flat fallback leaked the whole nested
            // map per iteration (loop OOM).
            self.variant_drop_handles.insert(dst, "map_msv".to_string());
            return;
        }
        if crate::lower::is_map_mlo_ty(ty) {
            // `Map[String, List[Option[Int]]]` arg temp — `$__drop_map_mlo` (the
            // bind-site route, mirrored; the flat fallback would leak the value lists).
            self.variant_drop_handles.insert(dst, "map_mlo".to_string());
            return;
        }
        if let Some(rname) = (match ty {
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::List, a)
                if a.len() == 1 =>
            {
                self.record_or_anon_drop_type_name(&a[0])
            }
            _ => None,
        }) {
            // A `List[<recursive-drop record>]` arg temp — `$__drop_list_<R>` (the
            // bind-site route, mirrored; the flat fallback leaked each element's
            // String fields — the krec-unique residue).
            self.variant_drop_handles.insert(dst, format!("list_{rname}"));
            return;
        }
        if matches!(ty,
            Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Map, a)
                if a.len() == 2 && matches!(a[0], Ty::String) && !is_heap_ty(&a[1]))
        {
            // `Map[String, <scalar>]` arg temp — the key-slot sweep (split layout, @4 = n),
            // mirroring the bind-site fix; the flat fallback leaked every key copy.
            self.heap_elem_lists.insert(dst);
            return;
        }
        if crate::lower::is_heap_elem_list_ty(ty) {
            self.heap_elem_lists.insert(dst);
        }
    }

    /// Register a freshly-materialized call-result temp used as a call argument: a
    /// HEAP temp is BORROWED into the call (`Handle`) and added to the scope-end
    /// drop set (it is owned by THIS scope, not moved out, so it is released after
    /// the call returns); a scalar temp is passed by value. A NESTED-OWNERSHIP temp
    /// (a `List[String]` from `set.from_list(string.split(…))`, etc.) is ALSO recorded
    /// in `heap_elem_lists` so its scope-end drop is the recursive `DropListStr` that
    /// frees the owned element Strings — a flat `Drop` would free only the block and
    /// LEAK the elements (per-iteration in a loop → OOM). Cert is unchanged: one `i`
    /// (alloc) + one `d` (drop) for the temp; DropListStr vs Drop is the runtime
    /// realization of that same single `d`.
    pub(crate) fn materialized_call_arg(&mut self, dst: ValueId, repr: Repr, ty: &Ty) -> CallArg {
        if repr.is_heap() {
            self.live_heap_handles.push(dst);
            self.seed_call_arg_heap_drop_route(dst, ty);
            // A `Value` call-argument temp (`f(value.array([…]))`, `f(value.str(s))`) drops via the
            // runtime-tag-dispatched `Op::DropValue` (recursive — an Array frees its element Values, a
            // Str its String), NOT a flat `Op::Drop` (which would leak the nested payload). Without
            // this a tag-5 Array / tag-4 Str passed as an argument leaks at the call-site scope end.
            if crate::lower::is_value_ty(ty) {
                self.value_handles.insert(dst);
            }
            // A RECORD/TUPLE call-argument temp (`f(mk(x))` — a fresh record passed by handle) drops at
            // the call-site scope end. Without a mask it falls to a flat `Op::Drop` (rc_dec the record
            // block only), LEAKING every heap field (the `f(mk(x))`-in-a-loop OOM). Seed its heap-slot
            // `record_masks` (the masked drop frees the leaf fields) and, when a field is a
            // Map/List[heap]/record/Value, route to the recursive `$__drop_<R>` via variant_drop_handles.
            if let Some((_, tys)) = self.aggregate_field_tys(ty) {
                let heap_slots: Vec<usize> =
                    (0..tys.len()).filter(|&i| is_heap_ty(&tys[i])).collect();
                self.record_masks.insert(dst, heap_slots);
                if let Some(name) = self.record_drop_type_name(ty) {
                    self.variant_drop_handles.insert(dst, name);
                }
            }
            CallArg::Handle(dst)
        } else {
            CallArg::Scalar(dst)
        }
    }
}
