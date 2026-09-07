// ── lib.rs, part 2: running a program ──
//
// include!-spliced into `lib.rs` at crate-root level (the 800-line file
// discipline, #1856). lib.rs keeps the `Interpreter` state, its constructor
// and the deterministic/wall meters; this part is the run side: `run_main`
// on the big-stack thread, global initialisation, the call entry points and
// the tail-call trampoline (`run_callable`), plus the free helpers the
// trampoline hops through (`bind_hop_frame`, `TailCallee`, `SpineOutcome`)
// and the public `interpret`.

impl<'a> Interpreter<'a> {
    /// Run the program's `main` entry point and return the observable outcome.
    ///
    /// The evaluation runs on a dedicated [`INTERP_STACK_SIZE`]-byte thread so
    /// the [`MAX_DEPTH`] recursion bound is decoupled from the *caller's* thread
    /// stack: a deeply-recursive program reports a clean `FuelExhausted` instead
    /// of a native stack overflow whether it is driven from a 2 MiB cargo-test
    /// worker, the 8 MiB main thread, or any other host stack. Only the
    /// `Send + Sync` `&IrProgram` crosses into the thread and the `Send`
    /// `RunOutcome` crosses back — the `Rc`/`Cell` evaluator state never leaves
    /// it. A `std::thread::scope` borrows the program in place (no `'static`
    /// requirement) and joins before returning, so the borrow is sound.
    ///
    /// The fuel budget is captured *before* spawning (the `Interpreter` itself is
    /// not `Send`); the worker rebuilds a fresh interpreter over the same program
    /// inside the big-stack thread — `Interpreter::new` only indexes the program,
    /// so this is cheap and observationally identical.
    pub fn run_main(self) -> RunOutcome {
        let program: &'a IrProgram = self.program;
        let fuel = self.fuel.get();
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .name("almide-interp".to_string())
                // The whole point: a big, KNOWN stack so MAX_DEPTH — not the
                // host thread's stack — is the binding recursion bound.
                .stack_size(INTERP_STACK_SIZE)
                .spawn_scoped(scope, move || {
                    Interpreter::new(program).with_fuel(fuel).run_main_on_stack()
                })
                .expect("failed to spawn almide-interp worker thread")
                .join()
                // A panic inside the evaluator is a genuine interpreter bug, not
                // an out-of-scope skip — re-raise it so it is never swallowed.
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        })
    }

    /// The actual `main`-driving logic. Runs on the big-stack worker thread
    /// spawned by [`run_main`](Self::run_main). Never call this directly off the
    /// dedicated thread — it carries the deep recursion that [`run_main`]'s stack
    /// is sized for.
    fn run_main_on_stack(mut self) -> RunOutcome {
        let main = match self.fns.get(&almide_base::intern::sym("main")) {
            Some(f) => *f,
            None => {
                // No entry point: a program of pure definitions. Treat as a
                // clean no-op run (matches `almide run` on a fn-only file,
                // which also produces no output).
                return RunOutcome {
                    status: RunStatus::Ok,
                    stdout: self.stdout,
                    stderr: self.stderr,
                };
            }
        };

        // Seed top-level lets into the shared global scope before main runs.
        if let Err(flow) = self.ensure_globals() {
            return self.outcome_from_flow(flow);
        }
        let root = self.globals.clone();

        match self.call_function(main, Vec::new(), &root) {
            // A `main` body whose value is an unhandled `Err`/`None` terminates
            // the program with `Error: <inner>` + exit 1 (the unhandled-main-
            // error termination contract). An `Ok`/`Some`/other value is a
            // clean exit. `call_function` already collapses a body-level
            // `Return` into `Value`.
            Flow::Value(v) => match unhandled_main_error(&v) {
                Some(msg) => self.outcome_from_flow(Flow::Abort(msg)),
                None => RunOutcome {
                    status: RunStatus::Ok,
                    stdout: self.stdout,
                    stderr: self.stderr,
                },
            },
            other => self.outcome_from_flow(other),
        }
    }

    /// Evaluate top-level lets into the shared `globals` scope exactly once.
    /// Idempotent: a second call is a no-op (so nested calls that trigger it do
    /// not re-run any effectful top-let).
    pub(crate) fn ensure_globals(&mut self) -> Result<(), Flow> {
        if self.globals_ready.get() {
            return Ok(());
        }
        // Mark ready up front so a top-let that calls a fn (which itself wants
        // globals) does not recurse into re-seeding.
        self.globals_ready.set(true);
        // DEPENDENCY-ORDERED init (#632, C-007): a top-let whose initializer reads a
        // LATER-declared global (directly or through a fn it calls — `BANNER =
        // make_banner()` reading `APP_NAME`) must see it already bound. Both backends
        // interprocedurally topo-sort the declaration order; evaluating in bare
        // declaration order here left the forward-referenced global unbound — a WRONG
        // third vote vs the native==wasm consensus. Reuse the SAME ordering utility so
        // the interp matches by construction.
        //
        // SPACED identities throughout (#1602): every top-let is a `GVar =
        // (space, VarId)` — separately-lowered modules each restart `VarId`s
        // at 0, so a bare-`VarId` index here silently collided (last module
        // wins). Each decl evaluates in ITS OWN space's frame, and every
        // module-origin alias of it is bound into the alias's space
        // IMMEDIATELY after — a topo-later initializer in another space reads
        // through its own alias `VarId`, so the bind cannot wait.
        use almide_ir::top_let_storage::{
            build_global_tables_spaced, dependency_init_order_spaced, GVar,
        };
        let (_globals_info, alias, _offenders) = build_global_tables_spaced(self.program);
        // Increment-1 boundary: a MUTABLE global aliased across spaces cannot
        // be modeled by per-space value binds (an assignment through one
        // alias would not propagate to the others) — abstain by name rather
        // than vote wrong. Immutable aliases are sound: the bound Value is a
        // snapshot of a binding that can never change.
        let mut mutable_decls: std::collections::HashSet<GVar> =
            std::collections::HashSet::new();
        for tl in &self.program.top_lets {
            if tl.mutable {
                mutable_decls.insert((0, tl.var));
            }
        }
        for (i, m) in self.program.modules.iter().enumerate() {
            for tl in &m.top_lets {
                if tl.mutable {
                    mutable_decls.insert((i as u32 + 1, tl.var));
                }
            }
        }
        let mut aliases_of: std::collections::HashMap<GVar, Vec<GVar>> =
            std::collections::HashMap::new();
        for (&site, &decl) in &alias {
            if site.0 != decl.0 && mutable_decls.contains(&decl) {
                return Err(Flow::Unsupported(format!(
                    "cross-space alias of mutable global `{}`",
                    almide_ir::top_let_storage::spaced_var(self.program, decl)
                        .name
                        .as_str()
                )));
            }
            aliases_of.entry(decl).or_default().push(site);
        }
        let order = dependency_init_order_spaced(self.program, &alias);
        // Index every top-let by its GVar so the sorted order can fetch its
        // initializer. A GVar in `order` but absent here (unreachable) is
        // skipped; a top-let absent from `order` (defensive) falls back to
        // decl order.
        let mut by_var: std::collections::HashMap<GVar, &almide_ir::IrExpr> =
            std::collections::HashMap::new();
        for tl in &self.program.top_lets {
            by_var.insert((0, tl.var), &tl.value);
        }
        for (i, m) in self.program.modules.iter().enumerate() {
            for tl in &m.top_lets {
                by_var.insert((i as u32 + 1, tl.var), &tl.value);
            }
        }
        let mut seen: std::collections::HashSet<GVar> = std::collections::HashSet::new();
        let ordered: Vec<(GVar, &almide_ir::IrExpr)> = order
            .iter()
            .filter_map(|g| by_var.get(g).map(|e| (*g, *e)))
            .chain(
                // Any top-let the sort omitted (a self-referential cycle the topo-sort
                // dropped) is appended in declaration order — never silently unbound.
                self.program
                    .top_lets
                    .iter()
                    .map(|tl| ((0, tl.var), &tl.value))
                    .chain(self.program.modules.iter().enumerate().flat_map(|(i, m)| {
                        m.top_lets.iter().map(move |tl| ((i as u32 + 1, tl.var), &tl.value))
                    })),
            )
            .filter(|(g, _)| seen.insert(*g))
            .collect();
        for ((space, var), value) in ordered {
            let frame = self.space_scope(space).clone();
            match self.eval_expr(value, &frame) {
                Flow::Value(v) => {
                    if let Some(sites) = aliases_of.get(&(space, var)) {
                        for &(s, sv) in sites {
                            self.space_scope(s).bind(sv, v.clone());
                        }
                    }
                    frame.bind(var, v);
                }
                other => return Err(other),
            }
        }
        Ok(())
    }

    /// The globals frame whose bindings a `VarId` in `space` resolves against
    /// (#1602): 0 = the program root (`self.globals`), i+1 = `modules[i]`.
    pub(crate) fn space_scope(&self, space: u32) -> &env::Scope {
        if space == 0 {
            &self.globals
        } else {
            &self.module_globals[(space - 1) as usize]
        }
    }

    /// The space whose `VarTable` `f`'s body indexes. Pool fns (self-contained,
    /// no top-lets) are absent from the map and resolve to the root frame.
    pub(crate) fn fn_space_of(&self, f: &IrFunction) -> u32 {
        self.fn_space
            .get(&(f as *const IrFunction as usize))
            .copied()
            .unwrap_or(0)
    }

    fn outcome_from_flow(&self, flow: Flow) -> RunOutcome {
        match flow {
            Flow::Value(_) | Flow::Return(_) | Flow::Break | Flow::Continue => RunOutcome {
                status: RunStatus::Ok,
                stdout: self.stdout.clone(),
                stderr: self.stderr.clone(),
            },
            Flow::Abort(msg) => {
                let mut stderr = self.stderr.clone();
                // The unhandled-error / abort termination contract: a single
                // `Error: <msg>` line on stderr, exit 1 (matches both backends'
                // main-error termination).
                stderr.push_str(&format!("Error: {}\n", msg));
                RunOutcome {
                    status: RunStatus::Aborted,
                    stdout: self.stdout.clone(),
                    stderr,
                }
            }
            Flow::Exit(code) => RunOutcome {
                status: match code {
                    0 => RunStatus::Ok,
                    1 => RunStatus::Aborted,
                    n => RunStatus::Exited(n as i32),
                },
                stdout: self.stdout.clone(),
                stderr: self.stderr.clone(),
            },
            Flow::Fuel => RunOutcome {
                status: RunStatus::FuelExhausted,
                stdout: self.stdout.clone(),
                stderr: self.stderr.clone(),
            },
            Flow::Unsupported(what) => RunOutcome {
                status: RunStatus::Unsupported(what),
                stdout: self.stdout.clone(),
                stderr: self.stderr.clone(),
            },
        }
    }

    /// Burn one unit of fuel; returns `Err(Flow::Fuel)` when exhausted.
    pub(crate) fn step(&self) -> Result<(), Flow> {
        let f = self.fuel.get();
        if f == 0 {
            return Err(Flow::Fuel);
        }
        self.fuel.set(f - 1);
        Ok(())
    }

    /// Bind a function's params and evaluate its body in a fresh frame parented
    /// at the program root scope (`base`). Top-level fns do not close over the
    /// caller's locals — only over top-level lets — so `base` is the root.
    pub(crate) fn call_function(
        &mut self,
        func: &'a IrFunction,
        args: Vec<Value>,
        base: &env::Scope,
    ) -> Flow {
        self.call_function_keeping_frame(func, args, base).0
    }

    /// [`Self::call_function`], but the callee's FRAME survives the call so the
    /// caller can read the final values of `mut` parameters back out — the
    /// copy-out half of the backends' mut-param lowering (C-132: the callee
    /// returns the buffer and every call position writes it back). See
    /// `eval_named_call`'s write-back (#1022).
    pub(crate) fn call_function_keeping_frame(
        &mut self,
        func: &'a IrFunction,
        args: Vec<Value>,
        base: &env::Scope,
    ) -> (Flow, env::Scope) {
        let (flow, frame) = self.run_callable(TailCallee::Fn(func), args, base);
        // GREENFIELD (#1226 unlock follow-up, adjudicated by the run-parity
        // manifest): `!`-propagation always travels as Result(Err(..)) — the
        // codegen lowers Option `!` to `ok_or("none")?` — but a fn DECLARED
        // `-> T?` propagates `none` at its boundary on both backends
        // (spec/wasm_cross/pure_bang_propagation.almd). Normalize the
        // carrier here, ret-type-driven — but PURE fns only: a declared-Option
        // EFFECT fn rides AUTO_WRAP and must propagate the Err with its
        // message (spec/wasm_fail/int_parse_err_propagates.almd — the exact
        // cell where the wasm leg once swallowed the error, #1410/#1411).
        let flow = match flow {
            Flow::Value(crate::value::Value::Result(Err(_)))
                if func.ret_ty.is_option() && !func.is_effect =>
            {
                Flow::Value(crate::value::Value::Option(None))
            }
            Flow::Return(crate::value::Value::Result(Err(_)))
                if func.ret_ty.is_option() && !func.is_effect =>
            {
                Flow::Return(crate::value::Value::Option(None))
            }
            other => other,
        };
        (flow, frame)
    }

    /// The tail-call trampoline: the shared engine under every function and
    /// closure application.
    ///
    /// The backends convert tail calls to loops (tco_rewrite; C-178's
    /// `return_call`/`return_call_indirect` on wasm), so `sum_to(1_000_000, 0)`
    /// runs in O(1) stack there — while a frame-per-call evaluator dies at
    /// [`MAX_DEPTH`] and abstains on programs the contracts DEFINE as
    /// constant-stack. This loop is the interp's mirror of that guarantee:
    /// each hop binds a fresh frame, walks the body's TAIL SPINE
    /// (`eval_body_spine`: Block tails, If branches, the effect `Try{Call}`
    /// wrapper), and when the spine ends in a call to a lowered fn or a
    /// closure, RE-ENTERS with the callee instead of recursing.
    ///
    /// What is preserved exactly:
    ///   - resolution order — the spine walker re-checks builtins / variant
    ///     ctors / Endian before the fn table, same as `eval_named_call`;
    ///   - the deterministic meter — the per-hop entry charge fires each
    ///     transfer, matching the backends' charge-inside-the-loop placement;
    ///   - `Try`/`Unwrap` — a transfer through the effect wrapper records the
    ///     marker's node type (run-length compressed), and the final base
    ///     value is folded through the SAME normalization `eval_try_unwrap`
    ///     applies, innermost-first, so N∘…∘N is computed, not approximated;
    ///   - mut-param copy-out — a callee with a `mut` param DECLINES the
    ///     transfer (evaluated via the plain nested path), because copy-out
    ///     needs the caller's lvalue, which a transferred frame no longer has.
    fn run_callable(
        &mut self,
        mut callee: TailCallee<'a>,
        mut args: Vec<Value>,
        base: &env::Scope,
    ) -> (Flow, env::Scope) {
        let d = self.depth.get();
        if d >= MAX_DEPTH {
            return (Flow::Fuel, base.child());
        }
        self.depth.set(d + 1);
        let det_was_user = self.det_in_user.get();
        let space_was = self.cur_space.get();
        // C-320: the meter's region depth at call entry — if the callee
        // leaves it HIGHER, a det cut skipped a region's budget_exit and
        // the exit bookkeeping runs here (exhausted ⇒ Err, never stale).


        // First hop's frame — what mut-param copy-out reads. Meaningful only
        // when no transfer happened, and a transfer implies no mut params.
        let mut first_frame: Option<env::Scope> = None;
        // (marker Option-identity bit, run length) — pending `Try`
        // normalizations (the bit is the only fact the fold reads, #1232).
        let mut pending: Vec<(bool, u32)> = Vec::new();

        let result = 'tramp: loop {
            if let Some(cut) = self.charge_hop_entry(&callee) {
                break 'tramp cut;
            }

            let fn_base = self.hop_base_scope(&callee, base);
            let (frame, fn_body, clo_body) = bind_hop_frame(&callee, &mut args, &fn_base);
            if first_frame.is_none() {
                first_frame = Some(frame.clone());
            }
            // #1226: a TAIL TRANSFER into a POOL body must carry the
            // address-uniform tier with it — without this, a pool callee
            // reached through the trampoline ran at the caller's depth, its
            // internal module calls were boundary-synced mid-flight (the
            // set_union eager-snapshot class), and its own return skipped
            // the read-back entirely (codec decode handed fixture code a raw
            // block address; `list.len on non-list`).
            let hop_pool = matches!(&callee, TailCallee::Fn(f) if self.pool_fns.contains(&f.name));
            if hop_pool {
                self.pool_depth += 1;
            }
            let outcome = match (&fn_body, &clo_body) {
                (Some(b), _) => self.eval_body_spine(b, &frame),
                (_, Some(c)) => self.eval_body_spine(&c.body, &frame),
                _ => unreachable!("one body source is always set"),
            };
            if hop_pool {
                self.pool_depth -= 1;
            }
            match outcome {
                SpineOutcome::Done(flow) => break 'tramp flow,
                SpineOutcome::Transfer { next, next_args, try_marker } => {
                    if let Some(ty) = try_marker {
                        match pending.last_mut() {
                            Some((last, n)) if *last == ty => *n += 1,
                            _ => pending.push((ty, 1)),
                        }
                    }
                    callee = next;
                    args = next_args;
                }
            }
        };

        let result = self.sync_final_hop(&callee, result);
        let result = normalize_option_fn_return(&callee, result);
        let result = self.fold_pending_try(result, &pending);
        self.det_in_user.set(det_was_user);
        self.cur_space.set(space_was);
        self.depth.set(self.depth.get() - 1);
        (result, first_frame.unwrap_or_else(|| base.child()))
    }

    /// #1602: a lowered fn's body indexes ITS space's VarTable, so its
    /// frame parents off that space's globals frame — never the
    /// caller's chain, whose same-numbered VarIds may belong to a
    /// different table. Closures keep their captured chain (which was
    /// built under this rule at creation).
    fn hop_base_scope(&self, callee: &TailCallee<'a>, base: &env::Scope) -> env::Scope {
        match callee {
            TailCallee::Fn(f) => {
                let space = self.fn_space_of(f);
                self.cur_space.set(space);
                self.space_scope(space).clone()
            }
            TailCallee::Clo(_) => base.clone(),
        }
    }

    /// #1226 read-back for the FINAL callee when the spine ended inside a
    /// pool fn at the tier boundary (tail transfers bypass the dispatch
    /// tails where the sync normally lives). Idempotent with the outer
    /// dispatch sync: a rebuilt value is no longer a block address.
    fn sync_final_hop(&self, callee: &TailCallee<'a>, result: Flow) -> Flow {
        match callee {
            TailCallee::Fn(f)
                if self.pool_depth == 0 && self.pool_fns.contains(&f.name) =>
            {
                // A body ending in an early exit hands back `Flow::Return` —
                // sync the carried value the same way and keep the flow kind
                // (the fold below is what resolves Return at the boundary).
                match result {
                    Flow::Value(_) => self.sync_block_return(f, result),
                    Flow::Return(v) => match self.sync_block_return(f, Flow::Value(v)) {
                        Flow::Value(v2) => Flow::Return(v2),
                        other => other,
                    },
                    other => other,
                }
            }
            _ => result,
        }
    }

    /// The per-hop entry charge — the backends charge INSIDE the loop, so each
    /// transfer pays, not just the first call. `Some(flow)` means the
    /// deterministic meter cut the spine and the trampoline must break with it.
    fn charge_hop_entry(&self, callee: &TailCallee<'a>) -> Option<Flow> {
        match callee {
            TailCallee::Fn(f) => {
                // Only USER frames burn deterministic fuel; the flag is restored
                // by the caller so a nested spine cannot leak its own answer.
                let det_is_user = self.user_fn_names.contains(&f.name);
                self.det_in_user.set(det_is_user);
                if !det_is_user {
                    return None;
                }
                // Loop-free non-recursive fns are inlined by the shared MIR,
                // entry charge included — skip the entry here too (the fn's
                // own dyn charges still count via `det_in_user`).
                if !self.det_entry_exempt(f.name) {
                    self.det_fuel.set(self.det_fuel.get().wrapping_sub(1));
                }
            }
            TailCallee::Clo(_) => self.det_charge(),
        }
        self.det_cut().then(|| Flow::Value(Value::Int(0)))
    }

    /// Fold the spine's pending `Try` normalizations over its final value,
    /// INNERMOST-FIRST, so `N∘…∘N` is computed rather than approximated. A
    /// function-body `Return` resolves to the returned value at the fn
    /// boundary first — and at each level, a `Return(x)` means "that level's fn
    /// returns x", which is the next level's call VALUE. Anything that is not a
    /// value (an abort, a fuel cut) stops the fold where it is.
    fn fold_pending_try(&mut self, result: Flow, pending: &[(bool, u32)]) -> Flow {
        let mut result = match result {
            Flow::Return(v) | Flow::Value(v) => Flow::Value(v),
            other => other,
        };
        for (ty, n) in pending.iter().rev() {
            for _ in 0..*n {
                match result {
                    Flow::Value(v) => {
                        result = match self.try_unwrap_value_flag(v, *ty) {
                            Flow::Return(v) | Flow::Value(v) => Flow::Value(v),
                            other => other,
                        }
                    }
                    stop => return stop,
                }
            }
        }
        result
    }

    /// Apply a closure value to arguments. Used by the in-interp HOFs and by
    /// `Computed` call targets. Rides the same trampoline as named fns, so a
    /// lambda whose tail is a call (C-178's indirect-recursion cycle) costs
    /// O(1) evaluator depth per chain, matching the backends' `return_call`.
    pub(crate) fn apply_closure(&mut self, clo: &Rc<Closure>, args: Vec<Value>) -> Flow {
        let root = self.root_scope();
        let flow = self.run_callable(TailCallee::Clo(Rc::clone(clo)), args, &root).0;
        // #1226 return sync at the CLOSURE boundary too: an FnRef of a pool
        // fn (`result.map(r, value.as_array)`) forwards through a closure,
        // and without the read-back its block address rode into native list
        // ops as a raw Int (codec decode aborted `list.len on non-list`).
        // Same depth gate as every other boundary: pool-internal closures
        // stay address-uniform.
        if self.pool_depth == 0 {
            if let Some(ty) = &clo.ret_ty {
                return self.sync_flow_typed(flow, &ty.clone());
            }
        }
        flow
    }
}

/// Bind one trampoline hop's frame and hand back the body to walk.
///
/// A named callee's frame is a child of the SPINE's base scope; a closure's is
/// a child of its own captured environment — that difference is the whole
/// reason the two arms exist. `args` is drained, so the caller's vector is
/// reusable for the next hop. The returned `Rc<Closure>` is not decoration: it
/// keeps a closure hop's body alive for the duration of the spine walk, since
/// `callee` is overwritten the moment a transfer happens.

fn bind_hop_frame<'a>(
    callee: &TailCallee<'a>,
    args: &mut Vec<Value>,
    base: &env::Scope,
) -> (env::Scope, Option<&'a IrExpr>, Option<Rc<Closure>>) {
    match callee {
        TailCallee::Fn(f) => {
            let frame = base.child();
            for (param, arg) in f.params.iter().zip(args.drain(..)) {
                frame.bind(param.var, arg);
            }
            (frame, Some(&f.body), None)
        }
        TailCallee::Clo(c) => {
            let frame = c.captured.child();
            for (param, arg) in c.params.iter().zip(args.drain(..)) {
                frame.bind(*param, arg);
            }
            (frame, None, Some(Rc::clone(c)))
        }
    }
}

/// A tail-transferable callee: a lowered named function (program-lifetime
/// borrow) or a closure value (owned Rc). What [`Interpreter::run_callable`]
/// loops over.
pub(crate) enum TailCallee<'a> {
    Fn(&'a IrFunction),
    Clo(Rc<Closure>),
}

/// C-211 (#1067): `!` on a None inside a PURE Option-returning fn propagates
/// as `none`, not as the Result-fn `err("none")`. `try_unwrap_value` cannot
/// see the enclosing fn, so it always manufactures the Result-fn shape and
/// this boundary — where the declared return type IS known — translates it.
/// Exact by construction: in a pure Option fn the checker rejects `err(..)`,
/// so a returned `Result(Err("none"))` has no other source. Effect fns keep
/// the Err (their fail channel IS the effect Result, C-216), and closures are
/// left as-is (an effect lambda is indistinguishable from a pure one here).
fn normalize_option_fn_return(callee: &TailCallee<'_>, flow: Flow) -> Flow {
    use almide_lang::types::constructor::TypeConstructorId as C;
    let TailCallee::Fn(f) = callee else { return flow };
    if f.is_effect || !matches!(&f.ret_ty, Ty::Applied(C::Option, a) if a.len() == 1) {
        return flow;
    }
    match flow {
        Flow::Return(Value::Result(Err(e)))
            if matches!(&*e, Value::Str(s) if s.as_str() == "none") =>
        {
            Flow::Return(Value::Option(None))
        }
        other => other,
    }
}

/// One hop's verdict from the tail-spine walker.
pub(crate) enum SpineOutcome<'a> {
    /// The body does not end in a transferable call — this flow is the hop's
    /// result (a `Return` is resolved at the engine's fn boundary).
    Done(Flow),
    /// The body's tail is a call to `next` — re-enter the trampoline.
    /// `try_marker` carries the `Try`/`Unwrap` marker node's type when the
    /// tail was the effect wrapper `Try{Call}`, so the engine can fold the
    /// normalization over the final value.
    Transfer { next: TailCallee<'a>, next_args: Vec<Value>, try_marker: Option<bool> },
}

/// If `main`'s result value is an unhandled error, return the message that the
/// program should terminate with (`Error: <msg>`). An `Err(e)` yields `e`
/// displayed bare (a String error prints raw, matching native
/// `Error: invalid digit found in string`); a `None` yields a generic message.
/// Any other value (incl. `Ok`/`Some`/`Unit`) is a clean exit (`None`).
fn unhandled_main_error(v: &Value) -> Option<String> {
    match v {
        Value::Result(Err(e)) => Some(e.display_bare()),
        Value::Option(None) => Some("called `Option::unwrap()` on a `None` value".to_string()),
        _ => None,
    }
}

/// Convenience: build an interpreter for `program` and run `main`.
pub fn interpret(program: &IrProgram) -> RunOutcome {
    Interpreter::new(program).run_main()
}
