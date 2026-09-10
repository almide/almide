//! Call lowering: user functions, variant constructors, the
//! println/eprintln special forms, and the `list.*` runtime forms.

use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStringPart};

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
                    // RC-3 callee-owned args hold for a lifted lambda
                    // exactly as for a named fn: its epilogue decs every
                    // droppable param, so a borrowed argument — a
                    // match-bound payload, a field or element read, a
                    // param of the enclosing fn — takes +1 here. Without
                    // it `pred(v)` inside `ok(v) => …` freed the payload
                    // the caller still held (the nightly fuzz's
                    // zeroed-string findings).
                    self.rc_arg_guard(a, *p);
                    self.witness_arg(a, *p);
                }
                self.f.instructions().local_get(h).i32_load(slot_memarg(0));
                let mut ps: Vec<ValType> = vec![ValType::I32];
                ps.extend(def.params.iter().map(|t| t.val_type()));
                let ti = self.work.itype(ps, def.ret.map(SliceTy::val_type));
                // Encoder argument order is (table, type).
                if tail && def.ret.is_some() && def.ret == self.fn_ret && self.tail_transfer_ok(true) {
                    // A tail call REPLACES the frame — the epilogue's param
                    // release never runs, so it runs HERE (args are already
                    // +1'd by rc_arg_guard, so a pass-through param
                    // survives its own dec).
                    let plan = self.exit_plan(crate::exit_plan::Continuation::TailTransfer { replaces_frame: true });
                    self.emit_exit(&plan);
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
            // codec_decode's ONE layout-reading helper gets a NATIVE
            // twin: the incumbent's __is_null reads its tag at h+4 (the
            // len-as-tag convention); OUR tag lives at PAYLOAD+SUM_TAG —
            // linking the body verbatim would read our LEN field and
            // silently never see a null.
            CallTarget::Named { name }
                if name.as_str() == "__is_null"
                    && args.len() == 1
                    && matches!(
                        slice_ty_of(&args[0].ty, self.types),
                        Some(SliceTy::Value)
                    ) =>
            {
                self.lower(&args[0], Some(SliceTy::Value))?;
                self.f
                    .instructions()
                    .i32_load(slot_memarg(almide_layout::SUM_TAG))
                    .i32_eqz();
                Ok(Some(BOOL))
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
                    return self.lower_variant_ctor(name, ti, ci, args);
                }
                // The http_framed op leaves (#1710 increment 3): the
                // splice's bodyless `= _` leaves, lowered as host ops —
                // the intra-module twins of the host_env http arms (url
                // in a, the method/body/headers frame in b). They carry
                // no table body, so they intercept BEFORE resolution.
                match name {
                    "__http_framed_text" => {
                        self.fs_call_str2(&args[0], &args[1], crate::fs_meta::OP_HTTP_FRAMED_TEXT)?;
                        return Ok(Some(self.fs_result_string()?));
                    }
                    "__http_framed_status" => {
                        self.fs_call_str2(&args[0], &args[1], crate::fs_meta::OP_HTTP_FRAMED_STATUS)?;
                        return Ok(Some(self.fs_result_string()?));
                    }
                    "__http_framed_bytes" => {
                        self.fs_call_str2(&args[0], &args[1], crate::fs_meta::OP_HTTP_FRAMED_BYTES)?;
                        return Ok(Some(self.fs_result_bytes()?));
                    }
                    _ => {}
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
                // Codec splices resolve by BARE name through the same
                // registry/whitelist path module calls use.
                let resolved = resolved.or_else(|| self.resolve_qualified(name));
                // Cross-module convention method: `Type.method` defined
                // beside its type in ANOTHER module — resolve by SUFFIX
                // when exactly one module defines it (unique-or-wall:
                // the #1558/#1087 bare-name landmine family demands the
                // ambiguity case refuse, never guess).
                let resolved = resolved.or_else(|| self.resolve_method_suffix(name));
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
                // The `consume(produce(scalars))` region window (#1961):
                // the whole producer/consumer pair runs inside a bump
                // window that RegionRestore rewinds wholesale. Opened
                // before the arguments (the producer call IS one), closed
                // right after the call; a return_call site keeps its C-292
                // constant stack instead.
                let window = !(tail && ret.is_some() && ret == self.fn_ret)
                    && self.region_window_opens(i, ret, args, &params);
                let save = if window { Some(self.emit_region_save()?) } else { None };
                // A self tail call in LOOP form under the raw-address rule:
                // the loop-back rebinds the params and releases nothing
                // (exit_plan.rs), so a param handed straight through
                // (`__arr(b, …)` → `__arr(b, …)`) must MOVE, not take the
                // borrow +1 — that +1 per iteration was the 16 B per call
                // of every `bytes.read_*_array` (#2005).
                let loop_form_raw = tail && Some(index) == self.self_index && !self.tail_release_allowed;
                let mut moved: Vec<u32> = Vec::new();
                let param_owned = self.table.infos[i].param_owned.clone();
                // Owned temporaries handed to BORROWED params are parked
                // here and released right after the call (the arm.rs
                // borrow pool): the callee spends nothing on them. A TRUE
                // tail site cannot release after the jump — and the Try
                // see-through (emitter.rs) relies on the Named arm
                // return_calling whenever ret == fn_ret — so there a fresh
                // temporary moves in under the owned convention instead
                // (param_borrow.rs marks such params owned; this is the
                // fallback, leak-not-dangle).
                let true_tail = tail && ret.is_some() && ret == self.fn_ret;
                // The Try see-through armed this site: it MUST transfer.
                // Any other tail site may stay a plain call and let the
                // epilogue release after it (`no_transfer`).
                let must_transfer = std::mem::take(&mut self.try_see_through) && true_tail;
                let mut no_transfer = false;
                let depth = self.borrowed_temps.len();
                for (k, (a, want)) in args.iter().zip(params).enumerate() {
                    self.lower(a, Some(want))?;
                    if loop_form_raw && let Some(p) = self.frame_param_var(a) && !moved.contains(&p) {
                        moved.push(p);
                        self.witness_arg(a, want);
                        continue;
                    }
                    let owned_pos = param_owned.get(k).copied().unwrap_or(true);
                    if self.lower_conv_arg(a, want, owned_pos, true_tail, must_transfer)? {
                        no_transfer = true;
                    }
                }
                let parked = self.borrowed_temps.len() > depth;
                self.calls.insert(i);
                if let Some(blk) = save {
                    self.f.instructions().call(index);
                    self.release_borrowed_temps(depth);
                    self.emit_region_restore(blk);
                    return Ok(ret);
                }
                // Tail position with a matching return type → return_call:
                // constant stack for arbitrarily deep (incl. mutual)
                // recursion, the C-292 contract. A parked temporary must
                // outlive the callee, so its site stays a plain call.
                if tail
                    && !parked
                    && !no_transfer
                    && ret.is_some()
                    && ret == self.fn_ret
                    && self.tail_transfer_ok(Some(index) != self.self_index)
                {
                    // Same frame-replacement release as the indirect site —
                    // unless the callee is THIS fn: tco.rs turns that
                    // return_call into a loop-back, and the frame lives on.
                    let plan = self.exit_plan(crate::exit_plan::Continuation::TailTransfer {
                        replaces_frame: Some(index) != self.self_index,
                    });
                    self.emit_exit(&plan);
                    self.f.instructions().return_call(index);
                } else {
                    self.f.instructions().call(index);
                    self.release_borrowed_temps(depth);
                }
                Ok(ret)
            }
            // Stdlib special forms the runtime helpers cover directly.
            CallTarget::Module { .. } => self.lower_module_call(target, args, tail, ret_hint),
            _ => unsup("call:computed-or-method"),
        }
    }










    /// Build a variant constructor's tagged block — split from
    /// lower_call_at for the complexity budget.
    fn lower_variant_ctor(
        &mut self,
        name: &str,
        ti: u32,
        ci: u32,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        let (size, tag, fields) = {
            let NamedDef::Variant(v) = &self.types.def(ti) else {
                return unsup("ctor-of-record");
            };
            let c = &v.cases[ci as usize];
            let fs: Vec<(SliceTy, u32)> = c.fields.iter().map(|f| (f.ty, f.offset)).collect();
            (c.size, c.tag, fs)
        };
        if args.len() != fields.len() {
            return unsup(&format!("ctor-arity:{name}"));
        }
        // A nullary case (`Leaf`, `None`-like markers, enum-style
        // variants) is a static block in the pool, one per (type, case)
        // (#1961): it carries only its tag, nothing ever writes it, and
        // the rc ops no-op below the heap floor — so binarytrees' 2^19
        // leaves cost zero allocations instead of two thirds of them.
        if fields.is_empty() {
            let mut payload = vec![0u8; size as usize];
            let at = almide_layout::SUM_TAG as usize;
            payload[at..at + 4].copy_from_slice(&tag.to_le_bytes());
            let block = self.pool.intern_block(&payload);
            self.f.instructions().i32_const(block as i32);
            return Ok(Some(SliceTy::Named(ti)));
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
            // RC-3: a variant payload retaining a borrowed droppable
            // (koka_reuse1's Pair2(acc1, acc2) — params stored, then
            // epilogue-released) co-owns.
            self.rc_share_guard(a, fty);
            self.store_ty_slot(fty, off);
        }
        self.f.instructions().local_get(hold);
        self.release_i32();
        Ok(Some(SliceTy::Named(ti)))
    }

    /// A `Type.method` spelling that missed both the current module and
    /// the bare table: accept the module-qualified key ENDING in
    /// `.Type.method` iff it is UNIQUE across modules — ambiguity walls
    /// (order-independent: uniqueness needs no iteration order).
    pub(crate) fn resolve_method_suffix(&self, name: &str) -> Option<usize> {
        if !name.contains('.') {
            return None;
        }
        let suffix = format!(".{name}");
        let mut hits = self.table.by_name.iter().filter(|(k, _)| k.ends_with(&suffix));
        let first = hits.next()?;
        if hits.next().is_some() {
            return None;
        }
        Some(*first.1)
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
                // Same construction class as list_repeat (prim-mediated
                // alloc_list + store64 into its OWN buffer, same die).
                "list_range",
                // String->String: byte-level string building; its tuple
                // helpers are module fns lowered by THIS emitter.
                "string_to_upper",
                // to_upper's mirror: same table-driven walker, same
                // own-buffer stores (the store32 writes ITS buffer's len
                // header — LEN offset 4 is digest-shared).
                "string_to_lower",
                // (String, Int) -> String: read-only loads on the source,
                // stores into a fresh alloc_str buffer only.
                "string_take_end",
                "string_drop_end",
                // string_from_bytes is NOT here: its `prim.load32(h+4)`
                // reads the LIST len header as an ELEMENT COUNT — the
                // incumbent's unit — where ours holds BYTES (8× the
                // count). Slot agreement is not enough: a RAW HEADER READ
                // couples an impl to the incumbent layout even when every
                // slot matches (found by result_match_rewrap_modcall:
                // "hi" decoded as 16 slots of garbage). Prim-mediated
                // alloc (list_repeat's alloc_list(count)) stays safe —
                // the prim writes OUR header.
                // (Int) -> String: UTF-8 byte encoding into its own fresh
                // string buffer — layout-shared writes only.
                "string_from_codepoint",
                // (String, String) -> String / (String, Int) -> String:
                // the #1675 decode-path composers — language-level
                // len/slice/starts_with/concat only, no raw header
                // access. Parity evidence: spec/wasm_cross/
                // codec_decode_errors.almd prints byte-identical across
                // legs including the composed `at address.city` /
                // `tags[1]` paths.
                "__err_at",
                "__err_at_index",
            ];
            // Second tier: signatures that TRIP the coupled-type proxy
            // below but whose bodies are AUDITED raw-write-free — every
            // sum is built via language-level ok()/err() (lowered by THIS
            // emitter with THIS layout) and every prim access is a
            // read-only load on a layout-shared block (string payload).
            // The proxy guards hand-written block internals; it misfires
            // on constructor-built sums.
            const VERIFIED_SUM_BUILDERS: &[&str] = &[
                // C-348: reads the shared string header/data, writes only its
                // own string buffer, and builds Option via typed constructors.
                "string_byte_slice",
                "string_to_int",
                "int_from_hex",
                "float_parse",
                // The JSON parser: raw ops build its OWN string buffers
                // (layout-shared); every Value comes through the public
                // value.* surface, which THIS emitter lowers natively —
                // the whole body is layout-consistent by construction.
                "json_parse",
                // (String, String) -> (String, String)?: byte-level find
                // over the source payload, LEN read on STRING blocks only
                // (digest-shared), two fresh alloc_str buffers, the pair
                // and the option built by constructors (#1423 stage 4).
                "string_split_once",
                // (String) -> DateTime!: no prim access at all — language-
                // level slicing and int parsing, Result via ok()/err().
                "datetime_parse_iso",
            ];
            if !VERIFIED.contains(&impl_fn)
                && !VERIFIED_SUM_BUILDERS.contains(&impl_fn)
                && !crate::whitelist::SIZED_CONVERT_VERIFIED.contains(&impl_fn)
                && !crate::whitelist::SIZED_CONVERT_SUM_BUILDERS.contains(&impl_fn)
                && !crate::whitelist::SCALAR_TEXT_VERIFIED.contains(&impl_fn)
                && !crate::whitelist::SCALAR_TEXT_SUM_BUILDERS.contains(&impl_fn)
                && !crate::whitelist::MATH_VERIFIED.contains(&impl_fn)
                && !crate::whitelist::CODEC_ENCODE_VERIFIED.contains(&impl_fn)
                && !crate::whitelist::BYTES_FAMILY_VERIFIED.contains(&impl_fn)
                && !crate::whitelist::BYTES_FAMILY_SUM.contains(&impl_fn)
                && !crate::whitelist::HTTP_CLIENT_SUM.contains(&impl_fn)
            {
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
                && !crate::whitelist::SIZED_CONVERT_SUM_BUILDERS.contains(&impl_fn)
                && !crate::whitelist::SCALAR_TEXT_SUM_BUILDERS.contains(&impl_fn)
                && !crate::whitelist::CODEC_ENCODE_VERIFIED.contains(&impl_fn)
                && !crate::whitelist::BYTES_FAMILY_SUM.contains(&impl_fn)
                && !crate::whitelist::HTTP_CLIENT_SUM.contains(&impl_fn)
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
            // Flush [start, cursor) from its PHYSICAL home (the region may
            // have relocated to a heap arena mid-build, #1826), then
            // release the buffer region.
            let flush = if import == F_PRINTLN_IMPORT { F_LINE_PRINTLN } else { F_LINE_EPRINTLN };
            self.f
                .instructions()
                .local_get(start)
                .local_get(self.cursor_local)
                .call(flush)
                .local_get(start)
                .global_set(G_LINE_CURSOR);
            self.release_i32();
            return Ok(());
        }
        // println / eprintln only READ their argument: a temporary handed
        // to them (`println(int.to_string(i))`) is borrowed and released
        // right after the write — this site is its own wrapper (arm.rs
        // `ArgMode`), there being no module-call wrapper around a Named
        // builtin.
        self.arm_scope(|em| {
            em.lower_arg(arg, Some(STR), ArgMode::Borrow)?;
            em.f.instructions().call(block_print);
            Ok(())
        })
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

    /// One already-lowered argument under the callee's declared convention
    /// (param_borrow.rs, #2028) — the Named and the registry route share
    /// it, so the two cannot disagree with the ONE `param_owned` table.
    /// `owned_pos` = the callee owns this param (releases it at its exit
    /// plan). Returns true when a borrowed position forbids a
    /// `return_call` at this site (`no_transfer`).
    pub(crate) fn lower_conv_arg(
        &mut self,
        a: &IrExpr,
        want: SliceTy,
        owned_pos: bool,
        true_tail: bool,
        must_transfer: bool,
    ) -> Result<bool, EmitError> {
        let is_static = matches!(a.kind, IrExprKind::LitStr { .. });
        let fresh = self.rc_droppable(want) && self.rc_owned_result(a) && !is_static;
        // Past a true tail site only a value THIS frame does not own
        // survives the exit plan: a param it borrows itself, or a pool
        // static (param_borrow.rs `tail_safe_arg`).
        let tail_safe = !true_tail
            || is_static
            || matches!(&a.kind, IrExprKind::Var { id }
                if self.locals.get(id).is_some_and(|&(idx, _)| {
                    idx < self.rc_param_ceiling && !self.rc_frame_params.contains(&idx)
                }));
        // A borrowed POSITION: droppable, and not owned by the callee (a
        // scalar param has no convention at all).
        let borrowed_pos = self.rc_droppable(want) && !owned_pos;
        let no_transfer = borrowed_pos && !tail_safe && !must_transfer;
        if !borrowed_pos || (must_transfer && !tail_safe) {
            // RC-3 callee-owned args: a borrowed droppable argument gets
            // +1 here, the callee's epilogue decs its params — the pair
            // keeps a mut-param callee's realloc-free honest (rc reflects
            // both holders).
            self.rc_arg_guard(a, want);
            self.witness_arg(a, want);
        } else {
            // A borrowed param (#2028): a Var passes as is; an owned
            // temporary is parked for release after the call (a string
            // literal is a pool static: nothing to release).
            if fresh {
                if self.borrowed_temps.len() as u32 >= crate::emitter::BORROW_POOL {
                    return Err(EmitError::Unsupported("borrow-depth".into()));
                }
                let h = self.borrow_base + self.borrowed_temps.len() as u32;
                self.f.instructions().local_tee(h);
                self.borrowed_temps.push((h, want));
            }
            self.witness_arg_borrowed(a, want, fresh);
        }
        Ok(no_transfer)
    }

    /// Linked module functions live in the table under their qualified
    /// name. A stdlib SURFACE call additionally resolves through the
    /// self-host registry to its loaded implementation (same registry
    /// the interp's bridge uses — one IR, two sound resolutions).
    /// Anything else is an honest wall.
    pub(crate) fn lower_linked_call(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
        tail: bool,
    ) -> ArmResult {
        let key = format!("{module}.{func}");
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
        // The SAME per-argument convention as the Named route (#2028 /
        // #1696 step 4): the callee's param_owned table decides share-and-
        // move vs borrow; a borrowed position past a true tail site keeps
        // the call plain (`no_transfer`), a parked temporary too.
        let param_owned = self.table.infos[i].param_owned.clone();
        let true_tail = tail && ret.is_some() && ret == self.fn_ret;
        let must_transfer = std::mem::take(&mut self.try_see_through) && true_tail;
        let depth = self.borrowed_temps.len();
        let mut no_transfer = false;
        for (k, (a, want)) in args.iter().zip(params).enumerate() {
            self.lower(a, Some(want))?;
            let owned_pos = param_owned.get(k).copied().unwrap_or(true);
            if self.lower_conv_arg(a, want, owned_pos, true_tail, must_transfer)? {
                no_transfer = true;
            }
        }
        let parked = self.borrowed_temps.len() > depth;
        self.calls.insert(i);
        if tail
            && !parked
            && !no_transfer
            && ret.is_some()
            && ret == self.fn_ret
            && self.tail_transfer_ok(Some(index) != self.self_index)
        {
            // The third tail site, found by scripts/check-exit-sites.sh
            // the day the gate went in (#1995): a registry-table tail call
            // replaced the frame with no release at all — a user fn whose
            // tail is `string.to_upper(s)` leaked `s` on every call.
            let plan = self.exit_plan(crate::exit_plan::Continuation::TailTransfer {
                replaces_frame: Some(index) != self.self_index,
            });
            self.emit_exit(&plan);
            self.f.instructions().return_call(index);
        } else {
            self.f.instructions().call(index);
            self.release_borrowed_temps(depth);
        }
        // The callee-owned convention IS the declaration: a table callee
        // hands its droppable result over with exactly one credit (#1986 /
        // #1990); a scalar result carries nothing.
        Ok(ret.map(|t| if self.rc_droppable(t) { Lowered::owned(t) } else { Lowered::scalar(t) }))
    }
}
