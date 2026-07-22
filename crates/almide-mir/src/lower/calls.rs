//! `LowerCtx` methods: calls (extracted from lower/mod.rs).

use super::*;
use crate::purity;
use crate::{CallArg, Init, Op, Repr, RtFn, ValueId};
use almide_ir::{
    CallTarget, IrExpr, IrExprKind, IrStringPart,
};
use almide_lang::types::Ty;

/// Substitute `Ty::TypeVar(name)` with the supplied concrete type throughout `ty` —
/// the generic-record instantiation used by the VALUE MODEL (`Box[Int]`'s `value: T`
/// becomes `value: Int`). Total over `Ty`; an unmapped `TypeVar` is left as-is (the
/// caller's `scalar_field_width` then rejects it, walling the record).
pub(super) fn subst_type_var(
    ty: &Ty,
    subst: &std::collections::HashMap<almide_lang::intern::Sym, Ty>,
) -> Ty {
    match ty {
        Ty::TypeVar(name) => subst.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Ty::Applied(id, args) => {
            Ty::Applied(id.clone(), args.iter().map(|a| subst_type_var(a, subst)).collect())
        }
        Ty::Record { fields } => Ty::Record {
            fields: fields.iter().map(|(n, t)| (*n, subst_type_var(t, subst))).collect(),
        },
        Ty::OpenRecord { fields } => Ty::OpenRecord {
            fields: fields.iter().map(|(n, t)| (*n, subst_type_var(t, subst))).collect(),
        },
        Ty::Tuple(elems) => Ty::Tuple(elems.iter().map(|e| subst_type_var(e, subst)).collect()),
        // A generic PARAMETER of a record decl is stored as a bare `Named(T, [])` (the
        // frontend lowers an uninstantiated type variable to a nullary named type, NOT a
        // `Ty::TypeVar`). When `T` is one of this type's params (it is in `subst`), resolve
        // it to the instantiated arg — this is the #650 "generic field sized by its
        // INSTANTIATED type" fix, the substitution `aggregate_field_tys` relies on so a
        // `Box[Int]` field `value: T` resolves to `Int` (and its heap-ness is decided
        // correctly for the spread-copy / offset paths). A `Named` WITH args is a real
        // applied type — recurse into the args only.
        Ty::Named(name, args) if args.is_empty() && subst.contains_key(name) => {
            subst.get(name).cloned().unwrap_or_else(|| ty.clone())
        }
        Ty::Named(name, args) => {
            Ty::Named(*name, args.iter().map(|a| subst_type_var(a, subst)).collect())
        }
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|p| subst_type_var(p, subst)).collect(),
            ret: Box::new(subst_type_var(ret, subst)),
        },
        Ty::Union(members) => {
            Ty::Union(members.iter().map(|m| subst_type_var(m, subst)).collect())
        }
        // Scalars, Variant, Const*, Unknown, Never, etc. carry no nested TypeVar this
        // brick substitutes through — returned unchanged.
        other => other.clone(),
    }
}

impl LowerCtx {

    /// Lower a stdlib `Module` call (`<module>.<func>(args)`) in a VALUE position
    /// (bind or tail) to an `Op::CallFn` named `"<module>.<func>"`, IFF admissible.
    ///
    /// THE GATE: PURE — the callee reaches no host capability of its OWN
    /// ([`purity::is_pure`]). An effectful call lowered as a bare `Op::CallFn` would
    /// silently omit its capability from `used` (the checker derives caps only from
    /// `Op::Call`/the transitive fold over named callees), i.e. accept-but-unsafe.
    /// Walling it keeps `used` complete by construction. (A pure combinator's dotted
    /// name is treated as Stdout-free by the fold — sound because it IS pure; the
    /// capabilities come from the CLOSURE it applies, captured below.)
    ///
    /// HIGHER-ORDER closures are admitted (a pure combinator — `list.map`/`filter`/
    /// `fold` … — INVOKES the closure during the call and DISCARDS it: it never
    /// escapes, so the closure's captures cannot outlive the scope). Each closure
    /// ARGUMENT is handled by its capability, its value DEFERRED:
    /// - a `Lambda` — its body's calls are recorded as effect markers
    ///   ([`Self::record_elided_calls`]), so a printing closure taints HONESTLY and a
    ///   nested higher-order call inside the body is left elided (the `mir <= ir`
    ///   gate then taints — never a FALSE caps-verified);
    /// - a `ClosureCreate`/`FnRef` — its named callee is recorded as a marker so the
    ///   fold reaches its capabilities;
    /// - an OPAQUE function value (a `Fn`-typed `Var`/expr whose callee is unknown
    ///   here) is WALLED — its capabilities are unanalyzable, so admitting it would
    ///   be accept-but-unsafe. The closure's captures are BORROWED (the env is not
    ///   materialized → the rendered code owns nothing extra → memory-safe).
    ///
    /// Non-closure args are lowered normally. A heap result is a FRESH OWNED value
    /// (the return-mode signature), a scalar result carries no ownership. The caller
    /// decides bind (push to live handles) vs tail (move out). Returns the result.
    pub(crate) fn lower_pure_module_value_call(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Result<ValueId, LowerError> {
        // The primitive floor: `prim.load32(a)` / `prim.handle(s)` / `prim.fd_write(…)`
        // map to an Op::Prim, not a real CallFn (the v1 self-host floor).
        if module == "prim" {
            return self
                .lower_prim_call(func, args)?
                .ok_or_else(|| LowerError::Unsupported(format!("prim.{func} yields no value here")));
        }
        // INLINE `value.null()` to a tag-0 Value block (Alloc + store32 tag) instead of a CallFn — a
        // trivial pure constructor (value_core: `alloc_value(1); store32(h+4, 0)`). As a CallFn it would
        // OVER-COUNT vs the IR when the TCO synthesizes it for a `(Value,Int)` result-accumulator empty
        // (`mir>ir` caps breach — the synthetic call has no IR node to credit). Inlined it is NO CallFn,
        // so the TCO's synthetic empty adds no mir call; an explicit `value.null()` source node still
        // counts in the IR (mir < ir, allowed). The result is a fresh OWNED Value (cert `i`, same as the
        // call), tracked by the caller via `is_value_ty` exactly as before.
        if module == "value" && func == "null" && args.is_empty() {
            use crate::{IntOp, PrimKind};
            let len = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: len, value: 1 });
            let dst = self.fresh_value();
            self.ops.push(Op::Alloc {
                dst,
                repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
                init: crate::Init::DynList { len },
            });
            let h = self.fresh_value();
            self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(h), args: vec![dst] });
            let off = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: off, value: 4 });
            let addr = self.fresh_value();
            self.ops.push(Op::IntBinOp { dst: addr, op: IntOp::Add, a: h, b: off });
            let zero = self.fresh_value();
            self.ops.push(Op::ConstInt { dst: zero, value: 0 });
            self.ops.push(Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![addr, zero] });
            return Ok(dst);
        }
        // C1 DEFUNCTIONALIZATION — a `list.map`/`filter`/`fold` whose closure arg is an
        // INLINE lambda is specialized as a loop at the call site (no runtime closure, no
        // CallIndirect, no lifted fn). This is tried FIRST so a CAPTURING inline lambda
        // (`(x) => x * k`) WORKS via inline rather than walling at the self-host path below
        // (a capturing lambda has no liftable FuncRef). A non-inlinable form (a first-class
        // Var closure, a heap element/result, a side-effecting body) returns `None` and
        // falls through to the existing `lift_lambda` / self-host-combinator routing.
        // `fan.map` with a PURE lambda is OBSERVABLY list.map (the native runtime maps
        // in list order and collects; fan lambdas cannot capture a `var`, so the only
        // difference — parallelism — is unobservable, and the auto-`?` has already
        // stripped the effect Result by the time the call reaches a value position).
        // Route it through the same C1 defunctionalization; a non-inlinable form falls
        // through and WALLS (an unregistered `fan.*` CallFn), never the elided Const-0
        // that printed all-zero fan results (fan_map_inline_lambda, 2026-07-03).
        // STRUCTURAL dispatch (no name whitelist): any HIGHER-ORDER list/fan call is offered
        // to the defunc engine; WHICH combinators it inlines is the engine's own `match func`
        // — the single source of truth, so adding one there needs no second edit here (the
        // duplicated-list drift already caused a silent miss once).
        if (module == "list" || module == "fan") && crate::lower::is_higher_order(args) {
            if let Some(dst) = self.try_lower_defunc_list_hof(func, args, result_ty) {
                return Ok(dst);
            }
        }
        // The in-place `list.pop` in a VALUE position (`let last = list.pop(xs)`):
        // the same receiver discipline as the statement position — COW a local var,
        // write through a borrowed param, wall anything else (#794).
        if module == "list" && func == "pop" {
            self.cow_inplace_receiver(module, func, args)?;
        }
        let arg_tys: Vec<Ty> = args.iter().map(|a| a.ty.clone()).collect();
        let ops_mark = self.ops.len();
        let lowered = self.lower_pure_module_call_args(module, func, args)?;
        // WALL an UNFAITHFUL self-host HOF combinator BEFORE emitting its `CallFn`. When the closure
        // arg is a CAPTURING/param-invoking lambda (no liftable FuncRef), `lower_pure_module_call_args`
        // DROPS it (`last_call_had_unlifted_closure = true`) — so `lowered` omits the funcref. A SELF-
        // HOST-linked combinator (`list.flat_map`/`map`/`filter`/`fold`/`filter_map`) has a real wasm
        // body expecting `(list i32, funcref i64)`; emitting `(call $list.flat_map list)` with the
        // funcref dropped is INVALID WASM (`expected [i32, i64] but got [i32]` — the value-position
        // C2-lift bug, e.g. c/cpp/ruby `gen_pack_variant`'s inner `get_arr(pl,…) |> flat_map((pf) =>
        // gpe(pf, indent))` whose lambda captures the outer `indent`). The BIND position already walls
        // this via its `faithful` gate (binds_p2), but a VALUE-position call (inside a match arm / the
        // str-acc some-arm) reaches here without that guard — so the function silently emitted invalid
        // wasm. WALL it (→ Err), making the value position CONSISTENT with the bind position: never emit
        // invalid wasm. Truncate the partial closure/list temps we just pushed so the rolled-back
        // function starts clean. A non-HOF call, or a FAITHFULLY-lifted/C1-inlined closure, is
        // unaffected (`last_call_had_unlifted_closure` is false ⇒ this guard never fires for them).
        // GENERAL, not just the defunc five: EVERY registered combinator (`list.find`,
        // `sort_by`, `zip_with`, `set.filter`, `map.fold`, …) has a real wasm body whose
        // signature expects the funcref — emitting the call with the closure dropped is
        // INVALID WASM (the `find_tensor` capturing-`list.find` translation error: expected
        // i64, found i32 — an invalid-wasm-as-Ok escape the render wall cannot catch, since
        // the callee IS linked). An UNREGISTERED callee would have walled at render anyway,
        // so walling here is equally honest for it and strictly sounder for the rest.
        if crate::lower::is_higher_order(args) && self.last_call_had_unlifted_closure {
            self.ops.truncate(ops_mark);
            return Err(LowerError::Unsupported(format!(
                "{module}.{func} with an unliftable/closure-list higher-order argument cannot execute \
                 faithfully in this brick (walled, not mis-valued)"
            )));
        }
        let dst = self.fresh_value();
        let repr = repr_of(result_ty)?;
        // `string.slice(s, start)` is the 2-arg overload of `string.slice(s, start, end)` with the
        // implicit `end = string.len(s)` (v0: `s.chars().skip(start)`). The frontend admits the short
        // form (min_params=2) WITHOUT padding it, so the 3-param `string.slice` impl would underflow.
        // Route the 2-arg form to a DEDICATED `string.slice2(s, start)` variant that computes the end
        // itself — this stays ONE CallFn ↔ ONE IR call node (no extra synthetic call, so the corpus
        // `mir == ir` double-count gate is untouched), unlike synthesizing a `string.len` call arg.
        let name = if module == "string" && func == "slice" && args.len() == 2 {
            "string.slice2".to_string()
        } else if let Some(krec) = self.krec_call_name(module, func, &arg_tys, result_ty) {
            // A String-field-record key/element (C-015) — the generated `__krec_*` twin.
            krec
        } else {
            let key_nullary = self.map_key_is_nullary_variant(&arg_tys, result_ty);
            let key_scalar_rec = self.map_key_is_scalar_record(&arg_tys, result_ty);
            list_heap_call_name(module, func, &arg_tys, result_ty, key_nullary, key_scalar_rec)
        };
        self.ops.push(Op::CallFn {
            dst: Some(dst),
            name,
            args: lowered,
            result: Some(repr),
        });
        Ok(dst)
    }

    /// Admission + closure-capability capture shared by a stdlib `Module` call in any
    /// position (value or effect). Requires PURITY (the combinator's OWN caps must be
    /// ∅ — an effectful call would omit its capability, accept-but-unsafe). Captures
    /// each closure ARGUMENT's capabilities while DEFERRING its value and BORROWING
    /// its captures: a `Lambda` body's calls become effect markers, a `ClosureCreate`/
    /// `FnRef` named callee a marker; an OPAQUE function value (unanalyzable caps) is
    /// walled. Returns the lowered REGULAR (non-closure) args. The pure combinator
    /// invokes-and-discards the closure, so its captures never escape — see
    /// [`Self::lower_pure_module_value_call`].
    pub(crate) fn lower_pure_module_call_args(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Vec<CallArg>, LowerError> {
        // `random.int` / `env.args` / `env.unix_timestamp` / `fs.read_text` / `fs.list_dir` /
        // `fs.write` / `fs.mkdir_p` are the admitted EFFECTFUL stdlib calls: each is self-hosted
        // (random_int.almd / env_args.almd / env_unix_timestamp.almd / fs_read_text.almd /
        // fs_list_dir.almd / fs_write.almd / fs_mkdir_p.almd, linked here), so its prim floor
        // (`prim.random_get` / `prim.args_get_list` / `prim.clock_time_get` / `prim.read_text_file`
        // / `prim.read_dir` / `prim.write_text_file` / `prim.make_dir`) is in the program map and
        // the transitive cap_witness counts its capability (Entropy / CliArgs / Clock / FsRead /
        // FsRead / FsWrite / FsWrite) — UNLIKE a bodyless effectful intrinsic (which would
        // contribute 0 caps = accept-but-unsafe, the reason is_pure walls the rest).
        // `env.unix_timestamp` carries Capability::Clock — a DISTINCT cap (a clock read is neither
        // a filesystem nor an entropy effect). `fs.mkdir_p` / `fs.remove_all` REUSE
        // Capability::FsWrite (a mkdir / recursive remove IS a filesystem write). `io.print` REUSES
        // Capability::Stdout (it self-hosts over the SAME prim.fd_write floor as println, no new
        // prim). `io.read_line` carries Capability::Stdin — a DISTINCT cap (reading the operator's
        // input stream is neither a write, a filesystem, an entropy, nor a clock effect). The
        // caller is an `effect fn` (declares the host caps) so the `used ⊆ declared` checker
        // verifies it; a pure caller is a frontend error.
        // `random.choice` / `random.shuffle` self-host over the SAME prim.random_get floor
        // (random_choice.almd / random_shuffle.almd — typed element variants selected in
        // `list_heap_call_name`, unsupported elements route `_x` and wall at render), so the
        // transitive cap_witness counts Entropy exactly like `random.int`.
        let is_admitted_effectful = (module == "random" && func == "int")
            // `process.args` = argv[0]-inclusive CLI args (std::env::args) — self-hosted
            // over the SAME WASI args bridge as env.args (skip=0), Capability::CliArgs.
            || (module == "process" && func == "args")
            || (module == "random" && matches!(func, "choice" | "shuffle"))
            || (module == "env" && func == "args")
            // `env.get` READS the process environment — Capability::CliArgs (the Env
            // profile's canonical cap, argv and environ are the same initial-state
            // class). Self-hosted to `prim.env_get` (env_get.almd → the WASI environ
            // $env_get floor), so its prim is in the program map and the transitive
            // cap_witness counts CliArgs. Returns Option[String] (heap Option block).
            || (module == "env" && func == "get")
            || (module == "env" && func == "unix_timestamp")
            // `datetime.now` (Unix seconds) / `env.millis` (milliseconds) — the SAME WASI
            // wall-clock floor as env.unix_timestamp (clock_now.almd → prim.clock_time_get,
            // Capability::Clock). `random.float` — the SAME entropy floor as random.int
            // (random_float.almd → prim.random_get, Capability::Entropy). All scalar returns.
            || (module == "datetime" && func == "now")
            || (module == "env" && func == "millis")
            || (module == "random" && func == "float")
            || (module == "fs" && func == "read_text")
            || (module == "fs" && func == "read_bytes_raw")
            || (module == "fs" && func == "list_dir")
            || (module == "fs" && func == "read_bytes")
            || (module == "fs" && func == "write")
            || (module == "fs" && func == "mkdir_p")
            || (module == "fs" && func == "remove_all")
            // `fs.exists` READS the filesystem (a path stat) — it REUSES Capability::FsRead
            // (the SAME accounting as `fs.read_text`, NOT a new cap). Self-hosted to
            // `prim.path_exists` (fs_exists.almd), so its prim floor is in the program map
            // and the transitive cap_witness counts FsRead. UNLIKE the heap-Result fs prims,
            // it returns a SCALAR Bool (no allocation, no scope-end drop).
            || (module == "fs" && func == "exists")
            // `fs.stat` READS the filesystem (the full path_filestat_get) — REUSES
            // Capability::FsRead. Self-hosted to `prim.path_filestat` (fs_stat.almd), so its
            // prim floor is in the program map and the transitive cap_witness counts FsRead.
            // Returns Result[FileStat, String] (a record Ok payload).
            || (module == "fs" && func == "stat")
            || (module == "io" && func == "print")
            || (module == "io" && func == "read_line")
            // `io.read_n_bytes` READS standard input (the SIBLING of read_line) — REUSES
            // Capability::Stdin. Self-hosted to `prim.read_n_bytes` (io_read_n_bytes.almd → the
            // WASI fd-0 $read_n_bytes floor), so its prim is in the program map and the transitive
            // cap_witness counts Stdin. Returns a heap Bytes block (flat Drop, no nested handles).
            || (module == "io" && func == "read_n_bytes");
        // `fan.map` is a compiler-known concurrency primitive whose WASM lowering is a SEQUENTIAL
        // fallible traverse (PURE control flow — it reaches NO host capability itself; the CALLBACK's
        // caps are counted transitively through the lifted funcref, exactly like `list.map`). Admit it
        // (2-arg form); the per-(input, output)-element self-host is selected in `list_heap_call_name`,
        // where an UNSUPPORTED element pairing routes to the UNLINKED `fan.map_x` and walls cleanly at
        // render — never linked to a wrong-typed self-host (no invalid wasm).
        let is_admitted_fan_map = module == "fan" && func == "map" && args.len() == 2;
        if !purity::is_pure(module, func) && !is_admitted_effectful && !is_admitted_fan_map {
            return Err(LowerError::Unsupported(format!(
                "effectful/impure stdlib Module call {module}.{func} needs a declared capability not in this brick"
            )));
        }
        self.last_call_had_unlifted_closure = false;
        let mut out: Vec<CallArg> = Vec::with_capacity(args.len());
        for a in args {
            match &a.kind {
                // A NON-CAPTURING lambda ARGUMENT to a pure combinator (`list.map(xs, (x) =>
                // …)`): LIFT it and PASS its FuncRef table slot BY VALUE, so a SELF-HOSTED
                // combinator (auto-linked `list.map`/`filter`/`fold`) receives a real
                // callable closure and invokes it via CallIndirect. A CAPTURING lambda has no
                // liftable form, so it keeps the builtin-combinator model: its calls are
                // captured for the caps fold and the value is DROPPED (a builtin combinator
                // that is never self-host-linked ignores the extra arg — its name is
                // is_known_free, no body to mismatch). The lifted lambda's caps reach this
                // function through the FuncRef edge (folded at creation), so a printing
                // closure can never be silently caps-verified.
                IrExprKind::Lambda { params, body, .. } => match self.lift_lambda(params, body) {
                    // The closure BLOCK is passed like any heap arg (borrowed; it stays in
                    // the live set here and is dropped at scope end after the combinator).
                    Some(blk) => out.push(CallArg::Handle(blk)),
                    None => {
                        // A lambda outside the liftable subset — no closure form. The
                        // self-host combinator runs with a missing closure slot → an
                        // empty/garbage result.
                        self.last_call_had_unlifted_closure = true;
                        self.record_elided_calls(body);
                    }
                },
                IrExprKind::ClosureCreate { func_name, .. } => self.ops.push(Op::CallFn {
                    dst: None,
                    name: func_name.as_str().to_string(),
                    args: Vec::new(),
                    result: None,
                }),
                IrExprKind::FnRef { name } => self.ops.push(Op::CallFn {
                    dst: None,
                    name: name.as_str().to_string(),
                    args: Vec::new(),
                    result: None,
                }),
                // A FIRST-CLASS fn VALUE argument (`fn transform(xs, f, …) = xs |>
                // list.map(f)` — a Fn-typed PARAM/let flowing into the pure combinator):
                // pass its closure BLOCK by handle — the self-host combinator
                // CallIndirects it exactly like a lifted lambda's block (the 5c
                // possible-callee rows bound the witness). Capability-sound: a PURE
                // combinator can only receive a PURE closure (the frontend's effect
                // typing — an effectful closure is not a plain `(A) -> B` value), so the
                // callback contributes no host capability of its own; a lifted lambda's
                // caps were already folded at its creation site.
                IrExprKind::Var { id } if matches!(a.ty, Ty::Fn { .. }) => {
                    match self.value_for(*id) {
                        Ok(v) => out.push(CallArg::Handle(v)),
                        Err(_) => {
                            return Err(LowerError::Unsupported(format!(
                                "Module call {module}.{func} with an unresolved function-value argument not in this brick"
                            )))
                        }
                    }
                }
                _ if matches!(a.ty, Ty::Fn { .. }) => {
                    return Err(LowerError::Unsupported(format!(
                        "Module call {module}.{func} with an opaque function-value argument (capabilities unanalyzable) not in this brick"
                    )))
                }
                // A regular (non-closure) argument — lower it with the same per-arg machinery
                // as any call, preserving argument ORDER among the closure slots.
                _ => out.extend(self.lower_call_args(std::slice::from_ref(a))?),
            }
        }
        Ok(out)
    }

    /// Lower a pure `Module` COMBINATOR applied for its EFFECT (`list.each(xs, f)` in
    /// statement position) — the side effect is the CLOSURE's, captured by
    /// [`Self::lower_pure_module_call_args`]. A Unit/scalar result carries no
    /// ownership; a (rarely) discarded HEAP result is allocated and dropped at scope
    /// end (value semantics — never leaked).
    pub(crate) fn lower_effect_module_call(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
        result_ty: &Ty,
    ) -> Result<(), LowerError> {
        // A prim-floor STATEMENT (`prim.store32(a, v)`) → Op::Prim (Unit, no result).
        if module == "prim" {
            self.lower_prim_call(func, args)?;
            return Ok(());
        }
        // `process.exit(code)` — the T18 assert desugar's abort tail (and the user
        // form). Lowers to the ProcExit prim over the scalar code: no ownership
        // event, no capability of its own (E006 already forced the caller
        // effect). #782: this statement previously rode the retired v0 emitter.
        if module == "process" && func == "exit" && args.len() == 1 {
            let code = self.lower_scalar_value(&args[0]).ok_or_else(|| {
                LowerError::Unsupported(
                    "process.exit code outside the scalar value subset not in this brick".into(),
                )
            })?;
            self.ops.push(Op::Prim { kind: crate::PrimKind::ProcExit, dst: None, args: vec![code] });
            return Ok(());
        }
        // An IN-PLACE `&mut` BYTES mutator (set_*/write_*/fill/clear/copy_within/
        // copy_from — the self-host bodies store through args[0]'s block), or the
        // in-place `list.pop` (the same &mut protocol over a list receiver). The
        // native oracle's semantics (RcCow + borrow inference) split by receiver:
        //   - a LOCAL var: `make_mut` COWs a SHARED block at the mutation — so
        //     `var b = a; bytes.set_at(b, …)` must not write through `a` (#794;
        //     this lowered as a silently-shared write). Emit `MakeUnique` first.
        //   - a BORROWED PARAM: passed `&mut` through the call, so the write IS
        //     caller-visible (var_in_if_skinning's blend writes its non-mut
        //     `verts` param and the caller reads the results) — the bare
        //     write-through CallFn is exactly right, no COW.
        //   - anything else (a record FIELD needs the two-level record+field COW;
        //     a mutable GLOBAL receiver would mutate a local copy): WALL — a
        //     possibly-shared write is a silent aliasing miscompile, never emitted.
        if (module == "bytes"
            && (func.starts_with("set_")
                || func.starts_with("write_")
                || matches!(func, "fill" | "clear" | "copy_within" | "copy_from")))
            || (module == "list" && func == "pop")
        {
            self.cow_inplace_receiver(module, func, args)?;
        }
        let arg_tys: Vec<Ty> = args.iter().map(|a| a.ty.clone()).collect();
        let lowered = self.lower_pure_module_call_args(module, func, args)?;
        // `list.pop` keys its self-host on the ELEMENT type (scalar → the registered
        // in-place impl; heap → the unregistered `_x`, walling at render) — route it
        // through the SAME `list_heap_call_name` the value position uses, so a
        // statement-position pop can never link the scalar impl over a heap element
        // (which would leak the popped handle). Every other statement call keeps its
        // raw dotted name, byte-identical to before.
        let call_name = if module == "list" && func == "pop" {
            list_heap_call_name(module, func, &arg_tys, result_ty, false, false)
        } else {
            format!("{module}.{func}")
        };
        if is_heap_ty(result_ty) {
            let dst = self.fresh_value();
            let repr = repr_of(result_ty)?;
            self.ops.push(Op::CallFn {
                dst: Some(dst),
                name: call_name,
                args: lowered,
                result: Some(repr),
            });
            self.live_heap_handles.push(dst);
        } else {
            self.ops.push(Op::CallFn {
                dst: None,
                name: call_name,
                args: lowered,
                result: None,
            });
        }
        Ok(())
    }

    /// The #794 in-place-mutator RECEIVER discipline shared by the bytes mutators and
    /// `list.pop`: a LOCAL var receiver gets `Op::MakeUnique` (COW — an alias must keep
    /// the pre-mutation value: `var b = a; list.pop(a)` leaves `b` intact), a BORROWED
    /// PARAM writes through bare (the caller sees the mutation — the &mut protocol),
    /// and any other receiver shape (a record field, a fresh temp) WALLS — the write
    /// would land in a materialized copy and vanish, or alias a shared field.
    pub(crate) fn cow_inplace_receiver(
        &mut self,
        module: &str,
        func: &str,
        args: &[IrExpr],
    ) -> Result<(), LowerError> {
        match args.first().map(|a| &a.kind) {
            Some(IrExprKind::Var { id }) => {
                let v = self.value_for(*id)?;
                if !self.param_values.contains(&v) {
                    self.ops.push(Op::MakeUnique { v });
                }
                Ok(())
            }
            // A RECORD-FIELD receiver (`bytes.set_at(p2.buf, …)`) — the two-level
            // record+field COW (#794).
            Some(IrExprKind::Member { object, field }) => {
                self.two_level_field_cow(object, *field).ok_or_else(|| {
                    LowerError::Unsupported(format!(
                        "in-place mutator {module}.{func} over a non-var receiver \
                         (a shared record field would alias the write — the two-level \
                         record+field COW) not in this brick"
                    ))
                })
            }
            _ => Err(LowerError::Unsupported(format!(
                "in-place mutator {module}.{func} over a non-var receiver \
                 (a shared record field would alias the write — the two-level \
                 record+field COW) not in this brick"
            ))),
        }
    }
}
include!("calls_b.rs");
