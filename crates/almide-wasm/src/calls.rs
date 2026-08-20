//! Call lowering: user functions, variant constructors, the
//! println/eprintln special forms, and the `list.*` runtime forms.

use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStringPart};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::types_table::NamedDef;
use crate::*;

impl Emitter<'_> {
    pub(crate) fn lower_call(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        self.lower_call_at(target, args, false, None)
    }

    pub(crate) fn lower_call_at(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
        tail: bool,
        ret_hint: Option<SliceTy>,
    ) -> Result<Option<SliceTy>, EmitError> {
        match target {
            // Computed callee: a function VALUE — call_indirect through
            // the funcref table (args first, +1-biased slot last).
            CallTarget::Computed { callee } => {
                let got = self.lower(callee, None)?;
                let SliceTy::Fn(sig) = got else {
                    return unsup(&format!("computed-callee-{got:?}"));
                };
                let def = self.types.fn_sig_def(sig);
                if args.len() != def.params.len() {
                    return unsup("computed-arity");
                }
                // Closure convention: env block is arg 0; the callee's
                // table slot is the block's first payload field.
                let h = self.hold_i32()?;
                self.f.instructions().local_set(h);
                self.f.instructions().local_get(h);
                for (a, p) in args.iter().zip(def.params.iter()) {
                    self.lower(a, Some(*p))?;
                }
                self.f.instructions().local_get(h).i32_load(slot_memarg(0));
                let mut ps: Vec<ValType> = vec![ValType::I32];
                ps.extend(def.params.iter().map(|t| t.val_type()));
                let ti = self.work.itype(ps, def.ret.map(SliceTy::val_type));
                // Encoder argument order is (table, type).
                if tail && def.ret.is_some() && def.ret == self.fn_ret {
                    self.f.instructions().return_call_indirect(0, ti);
                } else {
                    self.f.instructions().call_indirect(0, ti);
                }
                self.release_i32();
                let _ = ret_hint;
                Ok(def.ret)
            }
            CallTarget::Named { name } if name.as_str() == "println" && args.len() == 1 => {
                self.lower_print(&args[0], F_PRINTLN_IMPORT, F_PRINTLN_BLOCK)?;
                Ok(None)
            }
            CallTarget::Named { name } if name.as_str() == "eprintln" && args.len() == 1 => {
                self.lower_print(&args[0], F_EPRINTLN_IMPORT, F_EPRINTLN_BLOCK)?;
                Ok(None)
            }
            CallTarget::Named { name } => {
                let name = name.as_str();
                // Variant constructor? Concrete ctors resolve by the global
                // map; GENERIC instances resolve by name within the call's
                // annotated type (the ret hint) — ctor names are ambiguous
                // across instances, the type context is not.
                let ctor = self.types.ctors.get(name).copied().or_else(|| {
                    let SliceTy::Named(ti) = ret_hint? else { return None };
                    let NamedDef::Variant(v) = self.types.def(ti) else { return None };
                    let ci = v.cases.iter().position(|c| c.name == name)?;
                    Some((ti, ci as u32))
                });
                if let Some((ti, ci)) = ctor {
                    let (size, tag, fields) = {
                        let NamedDef::Variant(v) = &self.types.def(ti) else {
                            return unsup("ctor-of-record");
                        };
                        let c = &v.cases[ci as usize];
                        let fs: Vec<(SliceTy, u32)> =
                            c.fields.iter().map(|f| (f.ty, f.offset)).collect();
                        (c.size, c.tag, fs)
                    };
                    if args.len() != fields.len() {
                        return unsup(&format!("ctor-arity:{name}"));
                    }
                    let hold = self.hold_i32()?;
                    self.f
                        .instructions()
                        .i32_const(size as i32)
                        .call(F_ALLOC)
                        .local_tee(hold)
                        .i32_const(tag as i32)
                        .i32_store(slot_memarg(almide_layout::SUM_TAG));
                    for (a, (fty, off)) in args.iter().zip(fields) {
                        self.f.instructions().local_get(hold);
                        self.lower(a, Some(fty))?;
                        self.store_ty_slot(fty, off);
                    }
                    self.f.instructions().local_get(hold);
                    self.release_i32();
                    return Ok(Some(SliceTy::Named(ti)));
                }
                // Entry fns resolve by name; a miss falls back to the
                // module-fn simple-name index (intra-module calls arrive
                // as Named after lower_module).
                // Intra-module Named calls resolve within the CURRENT
                // module first (simple names collide across modules),
                // then the entry program's globals.
                let resolved = self
                    .cur_module
                    .and_then(|m| self.table.by_name.get(&format!("{m}.{name}")))
                    .or_else(|| self.table.by_name.get(name))
                    .copied();
                let Some(i) = resolved else {
                    return unsup(&format!("call:{name}"));
                };
                let info = &self.table.infos[i];
                if let Some(r) = &info.refuse {
                    return unsup(&format!("call-fn:{name}:{r}"));
                }
                if args.len() != info.params.len() {
                    return unsup(&format!("call-arity:{name}"));
                }
                let (index, ret, params) = (info.wasm_index, info.ret, info.params.clone());
                for (a, want) in args.iter().zip(params) {
                    self.lower(a, Some(want))?;
                }
                self.calls.insert(i);
                // Tail position with a matching return type → return_call:
                // constant stack for arbitrarily deep (incl. mutual)
                // recursion, the C-292 contract.
                if tail && ret.is_some() && ret == self.fn_ret {
                    self.f.instructions().return_call(index);
                } else {
                    self.f.instructions().call(index);
                }
                Ok(ret)
            }
            // Stdlib special forms the runtime helpers cover directly.
            CallTarget::Module { .. } => self.lower_module_call(target, args, tail, ret_hint),
            _ => unsup("call:computed-or-method"),
        }
    }










    /// Resolve a qualified stdlib call ("float.to_string") to a table
    /// index: linked module fns first, then the self-host registry's
    /// implementation index.
    pub(crate) fn resolve_qualified(&self, key: &str) -> Option<usize> {
        self.table.by_name.get(key).copied().or_else(|| {
            let impl_fn = almide_types::self_host_registry::self_host_runtime()
                .iter()
                .flat_map(|(_, maps)| maps.iter())
                .find(|(_, surface)| *surface == key)
                .map(|(impl_fn, _)| *impl_fn)?;
            // WHITELIST: linked impls join the resolvable set ONE AT A
            // TIME, each landing with its own parity evidence — the
            // signature heuristic missed incumbent-layout coupling twice
            // (8-byte list slots, header-writing string builders).
            const VERIFIED: &[&str] = &[
                "float_to_string",
                "float_to_string_compound",
                "float_to_fixed",
                "int_to_string",
                // Dragon4's own dependency closure (prim-only bodies).
                "math_log",
                "math_log2",
                "math_log10",
                // String->String, raw stores build STRING blocks only —
                // the string layout is digest-shared with the incumbent.
                "string_trim",
                // (Int, Int) -> List[Int]: raw stores build 8-byte Int
                // slots — the one list class both layouts share. Carries
                // its own C-169 ceiling die.
                "list_repeat",
                // String->String: byte-level string building; its tuple
                // helpers are module fns lowered by THIS emitter.
                "string_to_upper",
                // (Int) -> String: UTF-8 byte encoding into its own fresh
                // string buffer — layout-shared writes only.
                "string_from_codepoint",
            ];
            // Second tier: signatures that TRIP the coupled-type proxy
            // below but whose bodies are AUDITED raw-write-free — every
            // sum is built via language-level ok()/err() (lowered by THIS
            // emitter with THIS layout) and every prim access is a
            // read-only load on a layout-shared block (string payload).
            // The proxy guards hand-written block internals; it misfires
            // on constructor-built sums.
            const VERIFIED_SUM_BUILDERS: &[&str] = &[
                "string_to_int",
                "int_from_hex",
                "float_parse",
                // The JSON parser: raw ops build its OWN string buffers
                // (layout-shared); every Value comes through the public
                // value.* surface, which THIS emitter lowers natively —
                // the whole body is layout-consistent by construction.
                "json_parse",
            ];
            if !VERIFIED.contains(&impl_fn) && !VERIFIED_SUM_BUILDERS.contains(&impl_fn) {
                return None;
            }
            let i = self.table.impl_index.get(impl_fn).copied()?;
            // LAYOUT BOUNDARY: self-host impls encode the INCUMBENT's
            // block layout. Scalars, strings and List[scalar] match our
            // ratified layout byte-for-byte; sums/maps/sets/tuples/named
            // do NOT (the incumbent keeps the Result tag in the len slot
            // — found by the burn-up: result.unwrap_or(ok(5)) returned
            // the default). An impl whose signature carries a
            // layout-coupled type stays UNRESOLVED (honest wall) until
            // the layouts are deliberately reconciled.
            let info = &self.table.infos[i];
            let coupled = |t: &SliceTy| {
                match t {
                    SliceTy::Option(_)
                    | SliceTy::Result(..)
                    | SliceTy::Map(..)
                    | SliceTy::Set(_)
                    | SliceTy::Tuple(_)
                    | SliceTy::Named(_) => true,
                    // The incumbent packs EVERY list element into an
                    // 8-byte slot; ours are 4 for the i32 word class —
                    // List[Int]/List[Float] agree, List[String]/List[Bool]
                    // do not (string.join through a linked impl trapped).
                    SliceTy::List(h) => self.types.el(*h).slot_size() == 4,
                    _ => false,
                }
            };
            if !VERIFIED_SUM_BUILDERS.contains(&impl_fn)
                && (info.params.iter().any(coupled) || info.ret.as_ref().is_some_and(coupled))
            {
                return None;
            }
            Some(i)
        })
    }





    /// `println`/`eprintln`: interpolations build in the line buffer;
    /// everything else must lower to a String block and goes through the
    /// stream's block-print helper.
    pub(crate) fn lower_print(&mut self, arg: &IrExpr, import: u32, block_print: u32) -> Result<(), EmitError> {
        if let IrExprKind::StringInterp { parts } = &arg.kind {
            let start = self.lower_interp_build(parts)?;
            // print(start, cursor - start), then release the buffer region.
            self.f
                .instructions()
                .local_get(start)
                .local_get(self.cursor_local)
                .local_get(start)
                .i32_sub()
                .call(import)
                .local_get(start)
                .global_set(G_LINE_CURSOR);
            self.release_i32();
            return Ok(());
        }
        self.lower(arg, Some(STR))?;
        self.f.instructions().call(block_print);
        Ok(())
    }

    /// Build interpolation parts into the line buffer from the CURRENT
    /// global cursor (stack-disciplined: nested value-position builds
    /// start after our partial content and restore on their exit).
    /// Returns the hold local carrying the build's start; the caller
    /// consumes the region [start, cursor_local), then must restore
    /// `G_LINE_CURSOR = start` and `release_i32()`.
    pub(crate) fn lower_interp_build(
        &mut self,
        parts: &[IrStringPart],
    ) -> Result<u32, EmitError> {
        let start = self.hold_i32()?;
        self.f
            .instructions()
            .global_get(G_LINE_CURSOR)
            .local_tee(start)
            .local_set(self.cursor_local);
        for part in parts {
            match part {
                IrStringPart::Lit { value } => {
                    if value.is_empty() {
                        continue;
                    }
                    let base = self.pool.intern(value);
                    let len = value.len() as i32;
                    self.f
                        .instructions()
                        .local_get(self.cursor_local)
                        .i32_const((base + almide_layout::PAYLOAD) as i32)
                        .i32_const(len)
                        .call(F_APPEND_COPY)
                        .local_set(self.cursor_local);
                }
                IrStringPart::Expr { expr } => {
                    // Publish our cursor so a nested build starts past it.
                    self.f
                        .instructions()
                        .local_get(self.cursor_local)
                        .global_set(G_LINE_CURSOR);
                    let got = self.lower(expr, None)?;
                    self.emit_display_value(got, false)?;
                }
            }
        }
        Ok(start)
    }


    /// Module-surface dispatch — special forms first, then the qualified
    /// table and the verified self-host whitelist. Split from
    /// `lower_call_at` for the complexity budget.
    fn lower_module_call(
        &mut self,
        target: &CallTarget,
        args: &[IrExpr],
        tail: bool,
        ret_hint: Option<SliceTy>,
    ) -> Result<Option<SliceTy>, EmitError> {
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
            _ => self.lower_module_call_b(target, args, tail, ret_hint),
        }
    }

    /// Module dispatch, second third (mechanical split — first-match
    /// order preserved).
    fn lower_module_call_b(
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
                self.lower(&args[0], Some(STR))?;
                let h = self.hold_i32()?;
                self.f.instructions().local_set(h);
                self.lower(&args[1], Some(STR))?;
                let hs = self.hold_i32()?;
                self.f.instructions().local_set(hs);
                let sp = self.work.helper(Helper::StringSplit);
                self.f.instructions().local_get(h).local_get(hs).call(sp);
                self.release_i32();
                self.release_i32();
                Ok(Some(SliceTy::List(self.types.intern(STR))))
            }
            CallTarget::Module { module, func, .. }
                if module.as_str() == "string" && func.as_str() == "to_bytes" && args.len() == 1 =>
            {
                self.lower(&args[0], Some(STR))?;
                self.f.instructions().call(F_BLOCK_COPY);
                Ok(Some(SliceTy::Scalar(Scalar::Bytes)))
            }
            _ => self.lower_module_call_c(target, args, tail, ret_hint),
        }
    }

    /// Module dispatch, final third + the qualified fallback.
    fn lower_module_call_c(
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
                self.lower_list_call(func.as_str(), args)
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
                // Linked module functions live in the table under their
                // qualified name. A stdlib SURFACE call additionally
                // resolves through the self-host registry to its loaded
                // implementation (same registry the interp's bridge uses —
                // one IR, two sound resolutions). Anything else is an
                // honest wall.
                let key = format!("{}.{}", module.as_str(), func.as_str());
                let Some(i) = self.resolve_qualified(&key) else {
                    return unsup(&format!("call:{key}"));
                };
                let info = &self.table.infos[i];
                if let Some(r) = &info.refuse {
                    return unsup(&format!("call-fn:{key}:{r}"));
                }
                if args.len() != info.params.len() {
                    return unsup(&format!("call-arity:{key}"));
                }
                let (index, ret, params) = (info.wasm_index, info.ret, info.params.clone());
                for (a, want) in args.iter().zip(params) {
                    self.lower(a, Some(want))?;
                }
                self.calls.insert(i);
                if tail && ret.is_some() && ret == self.fn_ret {
                    self.f.instructions().return_call(index);
                } else {
                    self.f.instructions().call(index);
                }
                Ok(ret)
            }
            _ => unreachable!("module dispatch"),
        }
    }

}
