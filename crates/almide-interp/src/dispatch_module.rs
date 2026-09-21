// ── dispatch.rs, part 2: the `Module` call tier ──
//
// include!-spliced into `dispatch.rs` at module level (the 800-line file
// discipline, #1856; the `val!`/`val_opt!` macros and the imports are
// dispatch.rs's own). This part holds the `(module, func)` tier:
// `eval_module_call` → HOF / in-place mutation / fan / budget prims →
// `dispatch_module_resolved` (container ops → the prim floors → the lowered
// self-hosted body, with the bridge around it in part 5, `dispatch_body.rs`)
// and the pool-tier boundary sync entry points.

impl<'a> Interpreter<'a> {
    // ── Module calls ────────────────────────────────────────────

    fn eval_module_call(
        &mut self,
        module: Sym,
        func: Sym,
        args: &[IrExpr],
        scope: &Scope,
    ) -> Flow {
        // First: is this an in-interp HOF? Those take closure ARGUMENTS and
        // must be evaluated specially (an interp closure cannot become the
        // `Rc<dyn Fn>` a generic runtime HOF demands).
        if crate::dispatch::is_hof(module.as_str(), func.as_str()) {
            return self.eval_hof(module, func, args, scope);
        }

        // Second: is this an in-place `mut`-receiver mutator? Those must also
        // be intercepted BEFORE the eager arg evaluation below — once the
        // receiver is a value, the binding it came from is unrecoverable and
        // the mutation has nowhere to land. See `inplace.rs`.
        if crate::hofs::is_inplace_mutating_op(module.as_str(), func.as_str()) {
            return self.eval_inplace_mutation(module, func, args, scope);
        }

        // Third: `fan.any`, evaluated at the DETERMINISTIC contract's spec
        // point rather than with threads: first-Ok in LIST ORDER with
        // side-effect order pinned sequential (C-185, C-004's own comment:
        // "any (sequential in list order)"), so it must NOT ride the eager
        // loop below — an arm after the winner is never evaluated. All-fail
        // is the defined Err (C-005). `fan.settle`'s block form never gets
        // here: it lowers to `IrExprKind::Fan`, which `eval_fan` already
        // evaluates (tuple of per-arm Results, the no-1-tuple rule).
        if module.as_str() == "fan" && func.as_str() == "any" {
            return self.eval_fan_any(args, scope);
        }

        // Otherwise evaluate all args eagerly, then dispatch.
        let mut evaled = Vec::with_capacity(args.len());
        for a in args {
            evaled.push(val!(self.eval_expr(a, scope)));
        }
        // `prim.handle`'s STATIC argument type — the only honest way to tell
        // a Bytes value from a List[Int] value at materialization (see
        // `handle_arg_ty`). Stashed, not threaded: the hint is consumed by
        // `heap_prim_handle` before anything can re-enter dispatch.
        if module.as_str() == "prim" && func.as_str() == "handle" {
            self.handle_arg_ty = args.first().map(|a| a.ty.clone());
        }
        self.dispatch_module_resolved(module, func, evaled)
    }

    /// `fan.any`, first-Ok short-circuit — see the dispatch comment above for
    /// the contract pins. The block form arrives as ONE argument: the literal
    /// LIST of 0-ary thunk closures the parser synthesized. Each thunk is
    /// CALLED in list order; a thunk returns its raw Result when the arm is an
    /// effect call (the interp's effect convention) and a bare value when pure
    /// — the pure case takes the Ok adapter here, mirroring what FanLowering
    /// bakes into both backends (#514).
    fn eval_fan_any(&mut self, args: &[IrExpr], scope: &Scope) -> Flow {
        let [thunks_expr] = args else {
            // The mapper form (list + fn) has its own runtime name upstream
            // (`any_map`); anything else reaching here is not the block ABI.
            return Flow::Unsupported(format!(
                "fan.any with {} args (thunk-list ABI takes 1)",
                args.len()
            ));
        };
        let thunks_val = val!(self.eval_expr(thunks_expr, scope));
        let Value::List(thunks) = thunks_val else {
            return Flow::Unsupported("fan.any over a non-list thunk carrier".to_string());
        };
        for t in thunks.iter() {
            let Value::Closure(clo) = t else {
                return Flow::Unsupported("fan.any arm that is not a thunk closure".to_string());
            };
            let v = val!(self.apply_closure(clo, Vec::new()));
            match v {
                // First Ok wins; later arms are never evaluated — the pinned
                // sequential side-effect order (C-004/C-185).
                ok @ Value::Result(Ok(_)) => return Flow::val(ok),
                Value::Result(Err(_)) => {}
                // A pure arm cannot fail: its value IS the winner (#514's
                // Ok adapter), and evaluation stops here too.
                pure => return Flow::val(Value::Result(Ok(Box::new(pure)))),
            }
        }
        Flow::val(Value::Result(Err(Box::new(Value::str(
            "fan.any: all candidates failed".to_string(),
        )))))
    }

    /// An in-place `mut`-receiver mutator, evaluated as a read → transform →
    /// write-back on the receiver's binding. Faithful only when the receiver
    /// names a binding this frame can actually assign to; every other shape
    /// abstains under its own name rather than dropping the effect silently.
    fn eval_inplace_mutation(
        &mut self,
        module: Sym,
        func: Sym,
        args: &[IrExpr],
        scope: &Scope,
    ) -> Flow {
        let (m, f) = (module.as_str(), func.as_str());
        if !crate::inplace::writes_back(m, f) {
            return Flow::Unsupported(format!(
                "in-place byte-level buffer write `{m}.{f}` (writes a scalar into a \
                 buffer at an offset; each carries its own bounds rule and byte \
                 order — issue #1021)"
            ));
        }
        let Some(recv) = args.first().and_then(crate::inplace::receiver_var) else {
            return self.eval_inplace_on_temporary(m, f, args, scope);
        };
        // A `mut`-PARAMETER receiver is fine now: the write lands on the
        // callee frame's binding, and `eval_named_call`'s mut-param copy-out
        // (#1022) carries it back to the caller's slot — the same two-step the
        // backends' MutParamLoweringPass performs (C-132).
        let mut rest = Vec::with_capacity(args.len().saturating_sub(1));
        for a in &args[1..] {
            rest.push(val!(self.eval_expr(a, scope)));
        }
        // The mutation runs against the binding's own storage — `with_slot`
        // rather than get/assign, so `Rc::make_mut` inside can keep a push loop
        // linear. Every argument is already a value by here, so nothing under
        // this closure re-enters the scope.
        match scope.with_slot(recv, |slot| crate::inplace::apply(m, f, slot, rest)) {
            Some(Some(out)) => Flow::val(out),
            Some(None) => Flow::Abort(format!("internal: malformed `{m}.{f}` receiver or args")),
            None => Flow::Abort(format!("internal: `{m}.{f}` on an unbound receiver")),
        }
    }

    /// [`Self::eval_inplace_mutation`] for a receiver that is not a variable.
    /// A TEMPORARY — a call result (`bytes.append_u8(bytes.new(2), 7)`,
    /// #1849) — has no binding to write back to, and none is needed: both
    /// backends evaluate the arguments in order, mutate the temporary and
    /// drop it, so the statement observes nothing but its own argument
    /// evaluation. The mutation runs on the evaluated value here (through
    /// the same `inplace::apply`, so a temporary that ALIASES a live
    /// binding's `Rc` takes the COW copy and the binding is untouched —
    /// the fixture's `same(x)` probe) and the value is dropped with the
    /// call. A record field or an index is a different shape: the backends
    /// write THROUGH it, and this frame has no single slot for it — that
    /// shape still abstains under its own name.
    fn eval_inplace_on_temporary(
        &mut self,
        m: &str,
        f: &str,
        args: &[IrExpr],
        scope: &Scope,
    ) -> Flow {
        let Some(recv) = args.first() else {
            return Flow::Abort(format!("internal: `{m}.{f}` with no receiver"));
        };
        // `h.f` where `h` is a binding of THIS frame: the backends write
        // through the field (a copy-on-write field write, C-033 — #2411 gave
        // the wasm leg that route), and so does this frame — the mutation runs
        // on the field's slot inside the binding's own record storage, through
        // `Rc::make_mut` on the field vector, so an alias of the record taken
        // before the push keeps the old list. A deeper path (`h.a.b`,
        // `xs[i].f`) still abstains below, by name.
        if let IrExprKind::Member { object, field } = &recv.kind {
            if let IrExprKind::Var { id } = &object.kind {
                let mut rest = Vec::with_capacity(args.len().saturating_sub(1));
                for a in &args[1..] {
                    rest.push(val!(self.eval_expr(a, scope)));
                }
                let field = *field;
                return match scope.with_slot(*id, |slot| match slot {
                    Value::Record { fields, .. } => {
                        let fields = std::rc::Rc::make_mut(fields);
                        let target = fields.iter_mut().find(|(k, _)| *k == field)?;
                        crate::inplace::apply(m, f, &mut target.1, rest)
                    }
                    _ => None,
                }) {
                    Some(Some(out)) => Flow::val(out),
                    Some(None) => Flow::Abort(format!(
                        "internal: `{m}.{f}` through a record field the binding does not hold"
                    )),
                    None => Flow::Abort(format!("internal: `{m}.{f}` on an unbound record receiver")),
                };
            }
        }
        let shape = match &recv.kind {
            IrExprKind::Call { .. } => None,
            IrExprKind::Member { .. } => Some("a record field"),
            IrExprKind::IndexAccess { .. } | IrExprKind::TupleIndex { .. } => Some("an index"),
            _ => Some("a non-call expression"),
        };
        if let Some(shape) = shape {
            return Flow::Unsupported(format!(
                "in-place container mutation `{m}.{f}` through {shape} receiver \
                 (the backends write through it; this frame has no single binding \
                 to write back to)"
            ));
        }
        let mut temp = val!(self.eval_expr(recv, scope));
        let mut rest = Vec::with_capacity(args.len().saturating_sub(1));
        for a in &args[1..] {
            rest.push(val!(self.eval_expr(a, scope)));
        }
        match crate::inplace::apply(m, f, &mut temp, rest) {
            Some(out) => Flow::val(out),
            None => Flow::Abort(format!("internal: malformed `{m}.{f}` receiver or args")),
        }
    }

    /// Dispatch a `(module, func)` whose args are already evaluated. Tiers:
    /// The budget prim quartet over the interpreter's deterministic meter —
    /// byte-for-byte the arithmetic of the wasm BudgetEnter/Exit renders and
    /// the native BUDGET_SHIM (see `render_wasm_p2_b.rs` / `render_native.rs`).
    /// Reached from the eval's `RuntimeCall` arm: the fan.bounded/race
    /// frontend lowering emits these symbols directly, pre-codegen.
    pub(crate) fn budget_prim_rt(&mut self, symbol: &str, args: &[Value]) -> Flow {
        let int0 = || match args.first() {
            Some(Value::Int(n)) => *n,
            _ => 0,
        };
        match symbol {
            "almide_rt_prim_budget_enter" => {
                let units = (int0() / almide_lang::time_units::CM1_NS_PER_CHARGE).max(0);
                self.det_entry.set(units);
                let saved = self.det_fuel.get();
                self.det_saved.set(saved);
                if units < saved {
                    self.det_fuel.set(units);
                }
                self.det_region_depth.set(self.det_region_depth.get() + 1);
                Flow::val(Value::Int(saved))
            }
            "almide_rt_prim_budget_exhausted" => {
                // C-320 reap: a det_cut returned from the region fn without
                // reaching the exit prim, leaving det_entry armed (>= 0 while
                // a region is open — the enter clamp's invariant). Perform
                // the missed exit bookkeeping first — same arithmetic, saved
                // taken from det_saved — exactly as the backend renders do.
                if self.det_entry.get() >= 0 {
                    self.det_verdict.set(i64::from(self.det_fuel.get() < 0));
                    let consumed = self.det_entry.get() - self.det_fuel.get();
                    self.det_spend.set(consumed);
                    self.det_fuel.set(self.det_saved.get() - consumed);
                    self.det_entry.set(-1);
                    self.det_region_depth.set(self.det_region_depth.get().saturating_sub(1));
                }
                Flow::val(Value::Int(self.det_verdict.get()))
            }
            "almide_rt_prim_budget_exit" => {
                self.det_verdict.set(i64::from(self.det_fuel.get() < 0));
                let consumed = self.det_entry.get() - self.det_fuel.get();
                self.det_spend.set(consumed);
                self.det_fuel.set(int0() - consumed);
                self.det_entry.set(-1);
                self.det_region_depth.set(self.det_region_depth.get().saturating_sub(1));
                Flow::val(Value::Int(0))
            }
            "almide_rt_prim_budget_spend" => Flow::val(Value::Int(self.det_spend.get())),
            "almide_rt_prim_timeout_enter" => {
                let saved = self.t_deadline.get();
                let now = if Self::omega_replay() >= 0 { 0 } else { self.wall_now_ns() };
                let dl = now.saturating_add(int0());
                if dl < saved {
                    self.t_deadline.set(dl);
                }
                self.det_region_depth.set(self.det_region_depth.get() + 1);
                Flow::val(Value::Int(saved))
            }
            "almide_rt_prim_timeout_exit" => {
                let hit = self.t_hit.get();
                self.t_verdict.set(hit as i64);
                if hit && almide_base::env::flag("ALMIDE_OMEGA_RECORD") {
                    self.stderr.push_str(&format!("__ALMD_OMEGA {}\n", self.t_ord.get()));
                }
                self.t_hit.set(false);
                self.t_deadline.set(int0());
                self.det_region_depth.set(self.det_region_depth.get().saturating_sub(1));
                Flow::val(Value::Int(0))
            }
            "almide_rt_prim_timeout_hit" => Flow::val(Value::Int(self.t_verdict.get())),
            other => Flow::Unsupported(format!("budget prim `{other}`")),
        }
    }

    /// interp-native container ops → the prim floors that hold interpreter
    /// state → the lowered (self-hosted) body → the bridge only where that
    /// body abstains or does not exist (#2185) → unsupported.
    pub(crate) fn dispatch_module_resolved(
        &mut self,
        module: Sym,
        func: Sym,
        args: Vec<Value>,
    ) -> Flow {
        // `process.exit(n)` — terminate with code n, printing nothing extra
        // (the ALS-T18 assert desugar eprintlns its own line first).
        if module.as_str() == "process" && func.as_str() == "exit" {
            let code = match args.first() {
                Some(Value::Int(n)) => *n,
                _ => 1,
            };
            return Flow::Exit(code);
        }
        // Interp-native container ops (non-HOF: structural transforms).
        if let Some(result) = self.eval_container_op(module.as_str(), func.as_str(), &args) {
            return result;
        }

        // The argv floor (Stage 2 BRIDGEABLE burn-down): value-clean prims the
        // STATELESS bridge cannot serve — they read the run's argv, which is
        // interpreter state (`with_args`, empty by default = exactly how the
        // oracle harness runs every fixture on all three legs).
        // `args_get_list` answers argv[1..]; `args_get_list_full` prepends an
        // argv[0] whose only cross-target observable is NONEMPTINESS (the
        // fixtures assert it, never print it — C-181's argv0 normalization).
        if let Some(flow) = self.prim_floor(module.as_str(), func.as_str(), &args) {
            return flow;
        }
        // #2185: the self-hosted body — the SAME source the wasm leg links for
        // this call name, and the definition the native leg compiles — is
        // consulted FIRST, so the third vote is the linked body's, not a
        // hand-mirrored Rust copy of it. The bridge is the FALLBACK, in two
        // cases only: a name with no lowered body (a true `@intrinsic` stub),
        // and a body that abstains from inside (a heap/effect prim outside
        // the scalar floor, or the `mut`-parameter gate below). There the
        // bridge's Rust-std answer is still an implementation independent of
        // both backends' codegen, and without it every string fixture would
        // leave the executable spec — the abstain ledger only shrinks.
        // `bridge_fallbacks` records each such fallback by name, so the arms
        // a body now shadows are measurable (and deletable) rather than
        // assumed.
        // INSIDE the pool tier (a self-hosted body evaluating), the bridge is
        // part of the FLOOR the bodies consume, exactly like `prim.*`: a
        // body's helper calls (`string.len` from the json decoder) are
        // answered over `Value`s here, where nesting another body over the
        // block heap is what the tier cannot yet do faithfully (the 11
        // wrong votes of the first body-first measurement: json paths, glob,
        // header lookup, codec error paths, two unbound-VarId crashes).
        let floor_first = self.pool_depth > 0;
        if floor_first {
            if let Some(result) = crate::bridge::dispatch(module.as_str(), func.as_str(), &args) {
                self.record_bridge_floor(module, func);
                return result;
            }
        }
        // The total fold_lines family is registered as TYPED twins only (`_msi` /
        // `_ls` / `_i` / `_s`): the wasm leg picks one from the accumulator's
        // static type, the interp from the init VALUE's shape — same four cells.
        let func = if self.pool_depth == 0 && module.as_str() == "fs" {
            Self::fold_lines_family_twin(func, &args).unwrap_or(func)
        } else {
            func
        };
        // The body for THIS arity (an overload routes to its suffixed twin,
        // `string.slice2` — see `resolve_body_for_arity`); no body, no twin →
        // the body cannot take this call, and the reason says so.
        let (func, func_def, gate_mut) = match self.resolve_body_for_arity(module, func, args.len()) {
            Ok(found) => found,
            Err(why) => return self.abstain_or_bridge(floor_first, module, func, &args, why),
        };
        // These sites receive EAGERLY-evaluated args, so a `mut` parameter's
        // caller lvalue is already gone — the Named path's copy-out (#1022)
        // cannot run here. A mut-param callee must abstain rather than
        // silently drop the write-back (a wrong third vote).
        if gate_mut && func_def.params.iter().any(|p| p.is_mut) {
            let why = format!(
                "module call `{}.{}` with a `mut` parameter through the \
                 eager dispatch path (no caller lvalue to copy out — #1022)",
                module.as_str(),
                func.as_str()
            );
            return self.abstain_or_bridge(floor_first, module, func, &args, why);
        }
        let root = self.root_scope();
        let flow = self.call_pool_tier(func_def, args.clone(), &root);
        // #1226 RETURN SYNC. A self-hosted body that allocated a block returns
        // the block's ADDRESS (`prim.alloc_str` is an Int), so the value has to
        // be read back out of the arena before it leaves the pool tier —
        // `base64_encode` is exactly `alloc → handle → store → return out`.
        //
        // Driven by the DECLARED return type, never by guessing whether an Int
        // looks like an address: a fn returning a plain Int (`string.len`) must
        // not have its result reinterpreted as a handle. That guess is the
        // wrong-vote trap this whole slice is built to avoid.
        //
        // Applied ONLY at the boundary back OUT of the pool tier
        // (`pool_depth == 0` at the call): inside the tier a heap value IS its
        // address, and an eager rebuild there snapshots blocks the caller is
        // still writing through (see `pool_fns`).
        let flow = self.sync_at_pool_boundary(func_def, flow);
        match flow {
            Flow::Unsupported(why) => self.abstain_or_bridge(floor_first, module, func, &args, why),
            other => other,
        }
    }

    /// The `prim` floors that read or MUTATE per-run interpreter state, which
    /// the stateless `bridge::prim_fn` cannot hold: the block heap, the guarded
    /// abort, argv, the live env, and the sandboxed fs. `None` = not one of
    /// these (or not `prim` at all); the caller falls through to the bridge.
    fn prim_floor(&mut self, module: &str, func: &str, args: &[Value]) -> Option<Flow> {
        if module != "prim" {
            return None;
        }
        // The BLOCK HEAP floor (#1226, heap.rs). Same tier as argv /
        // env / fs and for the same reason: these read and MUTATE per-run
        // interpreter state, which the stateless `bridge::prim_fn` cannot
        // hold. Slice 1 served the flat String/Bytes family; slice 2 adds
        // the slot-block container family (`alloc_list*` / `alloc_set*` /
        // `alloc_map*` / `alloc_value`, `store_str` / `load_str` /
        // `load_handle`, `rc_inc` / `rc_dec`). What a block cannot
        // faithfully spell still falls through to the honest abstain, so
        // this stays a CLOSED family the voting gate can arbitrate.
        if let Some(flow) = self.heap_prim(func, args) {
            return Some(flow);
        }
        match func {
            // `prim.die(prim.handle(msg))` — the guarded-abort floor
            // (Stage 2 BRIDGEABLE burn-down: int_pow_negative_exponent,
            // list_chunk_zero, …). The argument is by construction a
            // handle to the full "Error: <reason>\n" line both backends
            // eprint VERBATIM before exit(1); the interp's Abort contract
            // prints `Error: {msg}\n`, so the bridged message is that line
            // with the frame stripped. A message outside the frame
            // abstains — this floor must never invent a stderr the
            // backends would not produce.
            "die" => Some(self.prim_die(args)),
            "args_get_list" => {
                let items: Vec<Value> =
                    self.args.iter().map(|s| Value::str(s.clone())).collect();
                Some(Flow::val(Value::list(items)))
            }
            "args_get_list_full" => {
                let mut items = vec![Value::str("interp")];
                items.extend(self.args.iter().map(|s| Value::str(s.clone())));
                Some(Flow::val(Value::list(items)))
            }
            // The env floor (same tier as argv, C-133): `prim.env_get`
            // reads the LIVE process environment — exactly what the other
            // two legs observe (native getenv; wasm WASI environ with
            // inherit-env), so all three answer the same bytes. Without
            // this arm the resolve below finds the lowered `env_get`
            // stdlib fn BY BARE NAME — the very fn whose body is this
            // prim — and the interp spun in that cycle until fuel ran
            // out (the module-identity bug class, #1087–#1094, one tier
            // down: a prim resolved from the wrong source).
            "env_get" => {
                let Some(Value::Str(name)) = args.first() else {
                    return Some(Flow::Abort("internal: prim.env_get expects a String".into()));
                };
                Some(Flow::val(match std::env::var(name.as_str()) {
                    Ok(v) => Value::Option(Some(Box::new(Value::str(v)))),
                    Err(_) => Value::Option(None),
                }))
            }
            _ => self.vfs_prim(func, args),
        }
    }

    /// The guarded-abort floor behind `prim.die` — see the arm in
    /// [`Self::prim_floor`] for the contract.
    fn prim_die(&self, args: &[Value]) -> Flow {
        let Some(addr) = heap_addr(args.first()) else {
            return Flow::Unsupported("prim.die with a non-address".into());
        };
        let Some((bytes, _)) = self.heap.block_bytes(addr) else {
            return Flow::Unsupported("prim.die outside this heap's arena".into());
        };
        let msg = String::from_utf8_lossy(&bytes).into_owned();
        let Some(reason) = msg
            .strip_prefix("Error: ")
            .and_then(|m| m.strip_suffix('\n'))
        else {
            return Flow::Unsupported(
                "prim.die with a message outside the Error:-line contract".into(),
            );
        };
        Flow::Abort(reason.to_string())
    }

    /// Run a resolved body, tracking the pool-tier boundary: while a POOL
    /// body (see `pool_fns`) is on the stack, heap values stay addresses and
    /// no return sync fires.
    fn call_pool_tier(
        &mut self,
        func: &'a almide_ir::IrFunction,
        args: Vec<Value>,
        root: &Scope,
    ) -> Flow {
        // Depth is owned by `run_callable`'s per-hop bump (a tail TRANSFER
        // into a pool fn must carry the tier with it — a second bump here
        // would leave the spine-exit sync reading a stale depth and skipping
        // the read-back, the codec `list.len on non-list` leak).
        self.call_function(func, args, root)
    }

    /// The return sync, gated to the pool-tier EXIT boundary. `named` marks
    /// the named-call spelling: there, only a POOL callee syncs — a fixture's
    /// own fn keeps the raw address model its `import prim` code was written
    /// against (pre-slice-2 behavior, which the prim-family fixtures pin).
    fn sync_at_pool_boundary(
        &self,
        func_def: &'a almide_ir::IrFunction,
        flow: Flow,
    ) -> Flow {
        if self.pool_depth > 0 {
            return flow;
        }
        self.sync_block_return(func_def, flow)
    }

    /// Read a returned block address back into the value its declared return
    /// type names. Anything else passes through untouched; an address whose
    /// block CANNOT faithfully spell the declared type abstains — passing the
    /// raw address onward would hand the fixture an Int where its type says
    /// container, and whatever that Int does next is a wrong vote.
    pub(crate) fn sync_block_return(&self, func_def: &'a almide_ir::IrFunction, flow: Flow) -> Flow {
        let Flow::Value(v) = &flow else { return flow };
        match self.sync_value(v, &func_def.ret_ty) {
            Ok(Some(rebuilt)) => Flow::Value(rebuilt),
            Ok(None) => flow,
            Err(why) => Flow::Unsupported(why),
        }
    }

    /// [`Self::sync_block_return`] driven by a bare `Ty` — the closure-boundary
    /// entry (`apply_closure` has a `ret_ty`, not an `IrFunction`).
    pub(crate) fn sync_flow_typed(&self, flow: Flow, ty: &Ty) -> Flow {
        let Flow::Value(v) = &flow else { return flow };
        match self.sync_value(v, ty) {
            Ok(Some(rebuilt)) => Flow::Value(rebuilt),
            Ok(None) => flow,
            Err(why) => Flow::Unsupported(why),
        }
    }
}
