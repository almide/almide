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

    /// The TWO-LEVEL record+field COW (#794): before an in-place mutator writes through
    /// `r.f`, make BOTH levels uniquely owned so no alias observes the write.
    ///
    /// Level 1 — the RECORD: an UNCONDITIONAL spread-copy (the proven
    /// `try_lower_spread_record_construct` discipline): a fresh block, each scalar slot
    /// value-copied, each heap slot `Dup`'d (CO-OWNED — cert `a`) then moved in (cert
    /// `m`). The var rebinds to the copy; the OLD block's owned reference is released
    /// NOW by its type-routed drop (masked/recursive — at rc>1 it only decs, the alias
    /// keeps its block; at rc=1 it frees the old block and each Dup'd child drops back
    /// to one owner: the copy). Unconditional-copy is value-semantics-exact — an
    /// unshared receiver pays a copy but observes nothing. NOTE `Op::MakeUnique` CANNOT
    /// serve level 1: its `$list_copy` is a raw slot copy that aliases the children
    /// WITHOUT co-owning them (sound only for flat blocks — the bare-var bytes/list
    /// receivers it ships on).
    ///
    /// Level 2 — the FIELD: load the (possibly shared) field handle, `Op::MakeUnique`
    /// it (flat Bytes/list block — exactly the raw-copy shape $list_copy handles), and
    /// store the unique handle back into the record's slot. The mutator's own receiver
    /// arg then borrows the slot and writes the uniquely-owned block.
    ///
    /// Returns None (nothing emitted — the ops are appended only after every gate
    /// passes) when the receiver is not a LOCAL var bound to a materialized aggregate
    /// with a resolvable layout — the caller walls, unchanged.
    fn two_level_field_cow(
        &mut self,
        object: &IrExpr,
        field: almide_lang::intern::Sym,
    ) -> Option<()> {
        use crate::{Init, PrimKind};
        let IrExprKind::Var { id } = &object.kind else { return None };
        let old = self.value_for(*id).ok()?;
        if self.param_values.contains(&old) || !self.materialized_aggregates.contains(&old) {
            return None;
        }
        let (names, tys) = self.aggregate_field_tys(&object.ty)?;
        let fidx = names.iter().position(|n| n.as_str() == field.as_str())?;
        if !is_heap_ty(&tys[fidx]) {
            return None; // a scalar field is not an in-place heap receiver
        }
        // Level 1: the record spread-copy.
        let n = tys.len();
        let len = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: len, value: n as i64 });
        let new = self.fresh_value();
        self.ops.push(Op::Alloc {
            dst: new,
            repr: crate::Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT },
            init: Init::DynList { len },
        });
        let old_h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(old_h), args: vec![old] });
        let new_h = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(new_h), args: vec![new] });
        for (i, fty) in tys.iter().enumerate() {
            let off = crate::lower::layout::slot_offset(i) as i64;
            let src_addr = self.addr_at(old_h, off);
            let dst_addr = self.addr_at(new_h, off);
            if is_heap_ty(fty) {
                let child = self.fresh_value();
                self.ops.push(Op::Prim {
                    kind: PrimKind::LoadHandle,
                    dst: Some(child),
                    args: vec![src_addr],
                });
                let owned = self.fresh_value();
                self.ops.push(Op::Dup { dst: owned, src: child });
                let handle = self.fresh_value();
                self.ops.push(Op::Prim {
                    kind: PrimKind::Handle,
                    dst: Some(handle),
                    args: vec![owned],
                });
                self.ops.push(Op::Prim {
                    kind: PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![dst_addr, handle],
                });
                self.ops.push(Op::Consume { v: owned });
            } else {
                let val = self.fresh_value();
                self.ops.push(Op::Prim {
                    kind: PrimKind::Load { width: 8 },
                    dst: Some(val),
                    args: vec![src_addr],
                });
                self.ops.push(Op::Prim {
                    kind: PrimKind::Store { width: 8 },
                    dst: None,
                    args: vec![dst_addr, val],
                });
            }
        }
        // Rebind the var to the copy; transfer the read-shape/drop tracking; release the
        // old block's owned reference by its type route (masked/recursive).
        self.value_of.insert(*id, new);
        self.materialized_aggregates.insert(new);
        if let Some(mask) = self.record_masks.get(&old).cloned() {
            self.record_masks.insert(new, mask);
        }
        if let Some(route) = self.variant_drop_handles.get(&old).cloned() {
            self.variant_drop_handles.insert(new, route);
        }
        if self.heap_elem_lists.contains(&old) {
            self.heap_elem_lists.insert(new);
        }
        let old_drop = self.drop_op_for(old);
        self.ops.push(old_drop);
        self.live_heap_handles.retain(|h| *h != old);
        self.live_heap_handles.push(new);
        // Level 2: the FIELD COW — load the (possibly shared) child handle, make it
        // unique (flat raw copy), store it back into the copied record's slot.
        let foff = crate::lower::layout::slot_offset(fidx) as i64;
        let faddr = self.addr_at(new_h, foff);
        let buf = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::LoadHandle, dst: Some(buf), args: vec![faddr] });
        self.ops.push(Op::MakeUnique { v: buf });
        let faddr2 = self.addr_at(new_h, foff);
        let bh = self.fresh_value();
        self.ops.push(Op::Prim { kind: PrimKind::Handle, dst: Some(bh), args: vec![buf] });
        self.ops.push(Op::Prim {
            kind: PrimKind::Store { width: 8 },
            dst: None,
            args: vec![faddr2, bh],
        });
        Some(())
    }

    /// `base + off` as a fresh address value (a ConstInt + IntBinOp Add pair).
    fn addr_at(&mut self, base: ValueId, off: i64) -> ValueId {
        let o = self.fresh_value();
        self.ops.push(Op::ConstInt { dst: o, value: off });
        let addr = self.fresh_value();
        self.ops.push(Op::IntBinOp { dst: addr, op: crate::IntOp::Add, a: base, b: o });
        addr
    }

    /// Make the CALLS hidden inside a value whose CONTENT is deferred to
    /// `Init::Opaque` / `Const` VISIBLE to the transitive capability fold. An
    /// Opaque/Const value lowers NONE of its sub-expressions, so a call buried in a
    /// list element, constructor payload, operand, or scalar value (`[f()]`,
    /// `Some(g(x))`, `a ++ h()`, `var n = list.len(xs)`) vanishes from the MIR —
    /// invisible to the caps fold over `Op::CallFn` edges, forcing the corpus gate
    /// to conservatively TAINT the whole function. This appends a bare EFFECT MARKER
    /// `Op::CallFn { dst: None, args: [], result: None }` per such call: the
    /// existing handlers already treat a result-less, dst-less call as a PURE EFFECT
    /// — `ownership_certificate` emits no event (no `+1`/drop), `name_witness`
    /// references nothing (no dangling ref), the `+1`-backing gate ignores it — yet
    /// `reachable_caps_or_tainted` matches it by NAME and folds the callee
    /// transitively. So the EFFECT becomes analyzable while the value CONTENT stays
    /// deferred: the same Opaque deferral, now extended to the capability axis.
    ///
    /// Only calls whose capabilities the fold models SOUNDLY are recorded: a
    /// first-order `Named` call (the fold opens an in-profile callee or honestly
    /// taints an unknown one) and a first-order PURE `Module` call (a dotted name
    /// the gate treats as Stdout-free — sound because it IS pure). A higher-order
    /// call (unmodelled closure caps), an effectful/impure `Module` call (its dotted
    /// name would be WRONGLY treated as free), and a `Method`/`Computed` target are
    /// SKIPPED — left elided, so the `ir_calls > mir_calls` gate keeps the function
    /// tainted (no FALSE de-taint). This never errors and never walls — it only adds
    /// effect markers, so it can never turn an in-profile function `Unsupported`.
    ///
    /// SOUNDNESS BACKSTOP: a marker is recorded ONLY at a wholesale-elided position
    /// (the caller emits one `Opaque`/`Const` op for the whole `value`, lowering
    /// none of its sub-calls), so the MIR call-op count can only rise TOWARD the
    /// IR's, never past it. The corpus gate asserts `mir_calls <= ir_calls` — a
    /// double-count (the one way a marker could mask a real elision and FALSELY
    /// de-taint a function) then fails the gate, structurally impossible to ship.
    pub(crate) fn record_elided_calls(&mut self, value: &IrExpr) {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct Collector<'a> {
            names: Vec<String>,
            registry: &'a crate::lower::RecordLayouts,
        }
        impl IrVisitor for Collector<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                match &e.kind {
                    IrExprKind::Call { target, args, .. } => {
                        if !is_higher_order(args) {
                            match target {
                                CallTarget::Named { name } => {
                                    self.names.push(name.as_str().to_string())
                                }
                                CallTarget::Module { module, func, .. }
                                    if purity::is_pure(module.as_str(), func.as_str()) =>
                                {
                                    self.names
                                        .push(format!("{}.{}", module.as_str(), func.as_str()))
                                }
                                _ => {}
                            }
                        }
                    }
                    // A string `+` OPERATOR (`BinOp::ConcatStr`) lowers, where reachable,
                    // to a real `__str_concat` CallFn (`try_lower_concat_str`); in a
                    // DEFERRED position — a heap-result match/if arm tail, an Opaque
                    // call/branch — it is elided exactly like a call. Surface it as an
                    // elided `__str_concat` marker so the caps gate's `mir_calls` matches
                    // the `ir_calls` ConcatStr count (else the enclosing function falsely
                    // taints caps-unverified — `ir_calls > mir_calls`). SOUND: `__str_concat`
                    // is pure (empty capability witness — an `Op::CallFn` contributes zero
                    // caps), and the marker carries NO value (`dst: None`, no leak). The
                    // marker maps 1:1 to the counted ConcatStr node, so `mir_calls <=
                    // ir_calls` is preserved.
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatStr, .. } => {
                        self.names.push("__str_concat".to_string());
                    }
                    // A SCALAR-element list `+` OPERATOR (`BinOp::ConcatList` over List[Int/Float/Bool])
                    // lowers, where reachable, to a real `__list_concat` CallFn; in a DEFERRED position
                    // (a statement reassignment `c = c + [10]`, an Opaque branch/arg) it is elided like
                    // a call. Surface a `__list_concat` marker so the caps gate's `mir_calls` matches the
                    // `ir_calls` ConcatList count (the gate counts the SAME scalar-element shape). SOUND:
                    // `__list_concat` is pure (prim memory ops, empty capability witness), the marker
                    // carries no value (`dst: None`). A HEAP-element list concat is NOT counted by the
                    // gate and emits NO marker here (the `is_heap_ty` element guard mirrors the count).
                    IrExprKind::BinOp { op: almide_ir::BinOp::ConcatList, .. } => {
                        use almide_lang::types::constructor::TypeConstructorId;
                        let scalar_elem = matches!(&e.ty,
                            Ty::Applied(TypeConstructorId::List, a)
                                if a.len() == 1 && !crate::lower::is_heap_ty(&a[0]));
                        if scalar_elem {
                            self.names.push("__list_concat".to_string());
                        }
                    }
                    // A STRING INTERPOLATION in a DEFERRED position — a heap-result match/if
                    // arm where the WHOLE branch fell back to Opaque, or any Opaque value/arg.
                    // `count_ir_calls` credits a desugarable interp the call NODES of its
                    // desugared tree REGARDLESS of position (the gate's visitor walks every
                    // subtree); when the interp does NOT get folded by `try_lower_string_interp`
                    // (its enclosing branch is Opaque), surface the SAME synthetic calls as
                    // elided markers so `mir_calls` keeps pace with `ir_calls` (else the function
                    // falsely taints — the −32 caps regression). Every synthetic callee
                    // (`__str_concat`, `<module>.to_string`) is pure (no Stdout), so the markers
                    // add no capability; a NON-desugarable interp is credited 0 and emits 0
                    // markers here. The SYNTHETIC names are the ConcatStr + to_string wrappers
                    // ONLY — the operands' OWN calls (a `${g(x)}` callee) are reached by the
                    // `walk_expr` below over the ORIGINAL parts, so there is no double-count.
                    IrExprKind::StringInterp { parts } => {
                        for name in crate::lower::interp_synthetic_call_names(parts, self.registry) {
                            self.names.push(name);
                        }
                    }
                    _ => {}
                }
                walk_expr(self, e);
            }
        }
        let names = {
            let mut c = Collector { names: Vec::new(), registry: &self.record_layouts };
            c.visit_expr(value);
            c.names
        };
        for name in names {
            self.ops.push(Op::CallFn { dst: None, name, args: Vec::new(), result: None });
        }
    }

    /// Lower an EFFECT call (a Unit-typed `Call`) to a runtime [`Op::Call`].
    /// Today the recognized set is `println(s)` for a heap string → [`RtFn::PrintStr`],
    /// which BORROWS the string handle (no refcount change; the value stays live
    /// and is dropped at scope end) and reaches [`crate::Capability::Stdout`] (so a
    /// real printing program's capability witness is derived from real source).
    /// Anything outside the set is an explicit `Unsupported` (totality).
    pub(crate) fn lower_effect_call(&mut self, call: &IrExpr) -> Result<(), LowerError> {
        // An effect-fn call in STATEMENT position carries the auto-`?` of effect-Result
        // propagation: `g()` where `g` returns `Result[Unit, _]` is lowered by the
        // frontend as `Try { g() }` (or `Unwrap` for an explicit `g()!`). In statement
        // position the Result is DISCARDED (Unit), so there is no value to compute wrong —
        // the call simply runs for effect, and Err-propagation is the same loop-completion
        // model the heap-`Unwrap` tail already relies on (see `lower_tail`). Strip the
        // wrapper and lower the inner call. (A value-position `Unwrap` is still walled —
        // there the unwrapped value is load-bearing; here it is thrown away.)
        if let IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } = &call.kind {
            return self.lower_effect_call(expr);
        }
        // A primitive-floor STATEMENT (`prim.store32(...)` / a discarded `prim.*`):
        // `@intrinsic` lowers it to a `RuntimeCall`; map the `almide_rt_prim_*` symbol
        // to an `Op::Prim` (a store is Unit, so the dst is None — nothing to discard).
        if let IrExprKind::RuntimeCall { symbol, args } = &call.kind {
            if let Some(func) = symbol.as_str().strip_prefix("almide_rt_prim_") {
                self.lower_prim_call(func, args)?;
                return Ok(());
            }
        }
        let (target, args) = match &call.kind {
            IrExprKind::Call { target, args, .. } => (target, args),
            other => {
                return Err(LowerError::Unsupported(format!(
                    "effect statement {} is not a call",
                    kind_name(other)
                )))
            }
        };
        let name = match target {
            CallTarget::Named { name } => name.as_str(),
            // A pure Module COMBINATOR applied for side effects (`list.each(xs, f)`):
            // the effect is the CLOSURE's. Capture the closure's capabilities, borrow
            // the regular args, and emit the Unit-result call — exactly the value-
            // position higher-order handling, minus the result. An effectful/impure
            // Module call reaches a host capability of its OWN that the model cannot
            // yet name, so it stays walled (`purity::is_pure` gates inside).
            CallTarget::Module { module, func, .. } => {
                return self.lower_effect_module_call(module.as_str(), func.as_str(), args, &call.ty)
            }
            CallTarget::Method { method, .. } => {
                return Err(LowerError::Unsupported(format!(
                    "effect Method call .{} (unresolved dispatch) not in this brick",
                    method.as_str()
                )))
            }
            // A Computed effect call `(g)()` — the callee is a closure VALUE we cannot
            // name. DEFER it exactly like a Computed VALUE call: the callee's and args'
            // analyzable sub-calls are captured (`record_elided_calls`), the Computed
            // call itself is ELIDED (no nameable `CallFn`). Since `count_ir_calls` counts
            // the Computed `Call` node but the lowering emits no marker for it,
            // `ir_calls > mir_calls` TAINTS the function caps-unverified — honest (the
            // closure's invocation capabilities are unknown), never falsely caps-verified.
            // A discarded HEAP result is a fresh `Alloc{Opaque}` dropped at scope end;
            // a Unit/scalar result carries no ownership.
            CallTarget::Computed { callee } => {
                // C1 UNIT DIRECT-CALL INLINE — the statement-position twin of
                // `try_inline_direct_lambda_call`: `let inc = () => { count = count + 1 };
                // inc()` (the escape_analysis counter shape). The body's statements lower
                // AT THE CALL SITE — a MUTABLE capture is an ordinary in-scope Assign, so
                // no closure object and no lift is needed. Zero-param calls only in this
                // brick, and the body must not re-enter the same callee (a recursive
                // lambda would inline forever); failure rolls back to the paths below.
                if args.is_empty() {
                    if let IrExprKind::Var { id } = &callee.kind {
                        let id = *id;
                        if let Some((params, body)) = self.lambda_bindings.get(&id).cloned() {
                            let recurses = {
                                struct R {
                                    id: almide_ir::VarId,
                                    found: bool,
                                }
                                impl almide_ir::visit::IrVisitor for R {
                                    fn visit_expr(&mut self, e: &IrExpr) {
                                        if matches!(&e.kind, IrExprKind::Var { id } if *id == self.id)
                                        {
                                            self.found = true;
                                        }
                                        almide_ir::visit::walk_expr(self, e);
                                    }
                                }
                                let mut r = R { id, found: false };
                                almide_ir::visit::IrVisitor::visit_expr(&mut r, &body);
                                r.found
                            };
                            if params.is_empty() && !recurses {
                                let ops_mark = self.ops.len();
                                let lhh_mark = self.live_heap_handles.len();
                                let stmt = almide_ir::IrStmt {
                                    kind: almide_ir::IrStmtKind::Expr { expr: body },
                                    span: None,
                                };
                                if self.lower_stmt(&stmt).is_ok() {
                                    return Ok(());
                                }
                                self.ops.truncate(ops_mark);
                                self.live_heap_handles.truncate(lhh_mark);
                            }
                        }
                    }
                }
                // A Unit-result call THROUGH a lifted lambda value EXECUTES via CallIndirect
                // (e.g. `let f = (x) => print_it(x); f(3)`). Otherwise — a dynamic closure
                // value we cannot name — DEFER as before (calls captured, the Computed call
                // elided ⇒ honest caps taint).
                if let Some(blk) = self.closure_block_of_mut(callee) {
                    let mark = self.ops.len();
                    let lhh = self.live_heap_handles.len();
                    if let Ok(lowered) = self.lower_call_args(args) {
                        // The CallIndirect's declared RESULT selects the wasm func TYPE
                        // (none/i64/i32 — render_wasm's sig classes), and the lifted
                        // lambda's own table type comes from its RETURN repr. A
                        // result-less dispatch to a VALUE-returning closure (`drain()`
                        // where `drain = () => { list.pop(xs) }` returns Option[Int])
                        // therefore declared the WRONG type — "indirect call type
                        // mismatch" at runtime — and leaked the returned block. Derive
                        // the result from the callee's Fn RETURN type: a discarded HEAP
                        // result is a fresh owned value dropped at scope end (its
                        // type-routed recursive drop registered); a discarded scalar
                        // binds an unused dst; Unit keeps the result-less dispatch.
                        let ret_ty = match &callee.ty {
                            Ty::Fn { ret, .. } => (**ret).clone(),
                            _ => Ty::Unit,
                        };
                        if matches!(ret_ty, Ty::Unit) {
                            self.emit_closure_call(blk, None, lowered, None);
                        } else {
                            let repr = repr_of(&ret_ty)?;
                            let dst = self.fresh_value();
                            self.emit_closure_call(blk, Some(dst), lowered, Some(repr));
                            if is_heap_ty(&ret_ty) {
                                self.live_heap_handles.push(dst);
                                self.register_owned_heap_eq_drop(dst, &ret_ty);
                            }
                        }
                        return Ok(());
                    }
                    self.ops.truncate(mark);
                    self.live_heap_handles.truncate(lhh);
                }
                // STRICT value mode (the real render path — pipeline.rs sets it): eliding a
                // dynamic closure INVOCATION drops its side effects entirely (`run3(() => {
                // p = p + 10 })` printed p=0 — a silent wrong value, worse than the honest
                // caps taint the elision was designed around). REFUSE instead: the function
                // walls and `--verified` falls back to v0. The permissive caps-counting
                // classifier path keeps the elision (its only consumer is call accounting).
                if crate::lower::strict_values() {
                    return Err(LowerError::Unsupported(
                        "computed closure call outside the liftable subset cannot be \
                         faithfully executed (eliding it would drop the invocation's \
                         effects — a silently wrong value) not in this brick"
                            .into(),
                    ));
                }
                self.record_elided_calls(call);
                if is_heap_ty(&call.ty) {
                    let dst = self.fresh_value();
                    let repr = repr_of(&call.ty)?;
                    self.ops.push(Op::Alloc { dst, repr, init: Init::Opaque });
                    self.live_heap_handles.push(dst);
                }
                return Ok(());
            }
        };
        match (name, args.as_slice()) {
            // println(s) — the heap-string argument is BORROWED for a Stdout write.
            // A non-var arg (a literal `println("x")`, a concat `println(a ++ b)`,
            // an interpolation `println("${x}")`, or a call result `println(f())`)
            // is materialized into an owned temp by `lower_call_args` (the same
            // arg machinery as a normal call), then borrowed; the temp is dropped
            // at scope end. The Stdout effect makes the function caps-unverified
            // (it reaches Stdout, which `declared_caps` is empty for) — honest, not
            // claimed caps-safe.
            ("println", [arg]) if is_heap_ty(&arg.ty) => {
                let lowered = self.lower_call_args(std::slice::from_ref(arg))?;
                self.ops.push(Op::Call { dst: None, func: RtFn::PrintStr, args: lowered, result: None });
                Ok(())
            }
            // A USER function call (Unit result, e.g. `beep()`) → Op::CallFn. The
            // call BORROWS its heap-handle args (no refcount change here). The
            // callee's capabilities are accounted for at the CALL SITE against
            // its signature (the per-call-site subset rule), so a program is
            // rejected for a capability a CALLEE reaches — transitively — even
            // with no direct effect (closes the direct-only caps gap).
            (callee, call_args) => {
                let lowered = self.lower_call_args(call_args)?;
                // A callee whose (post-never-err-rewrite) call type is HEAP returns a
                // real block — a DECLARED-Result effect fn in statement position
                // (`write_message(..)!`, porta) or a discarded heap value. A bare
                // void `(call $f)` left that block ON THE WASM STACK (invalid wasm:
                // "values remaining on stack") and leaked it. Receive it into an
                // owned temp dropped at scope end; the by-type drop classes match
                // the bind path. A genuinely void callee (Unit / a never-err LIFTED
                // effect fn, whose call type was already rewritten to raw Unit)
                // keeps the void call.
                if is_heap_ty(&call.ty) {
                    let dst = self.fresh_value();
                    let pr = repr_of(&call.ty)?;
                    self.ops.push(Op::CallFn {
                        dst: Some(dst),
                        name: callee.to_string(),
                        args: lowered,
                        result: Some(pr),
                    });
                    if crate::lower::is_result_listval_ty(&call.ty) {
                        self.value_result_lists.insert(dst);
                    } else if crate::lower::is_value_result_ty(&call.ty) {
                        self.value_result_results.insert(dst);
                    } else if crate::lower::is_lenlist_list_ty(&call.ty) {
                        self.variant_drop_handles.insert(dst, "list_lenlist".to_string());
                    } else if crate::lower::is_heap_elem_list_ty(&call.ty) {
                        self.heap_elem_lists.insert(dst);
                    }
                    self.live_heap_handles.push(dst);
                } else {
                    self.ops.push(Op::CallFn {
                        dst: None,
                        name: callee.to_string(),
                        args: lowered,
                        result: None,
                    });
                }
                Ok(())
            }
        }
    }
}

include!("calls_p2.rs");
include!("calls_p3.rs");
include!("calls_p4.rs");
