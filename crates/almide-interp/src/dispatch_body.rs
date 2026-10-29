// ── dispatch.rs, part 5: the lowered body and the bridge around it (#2185) ──
//
// include!-spliced into `dispatch.rs` at module level (the 800-line file
// discipline, #1856). Where a `(module, func)` call finds its self-hosted
// body (`resolve_lowered_body`, the arity twin) and the two tiers the
// hand-mirrored bridge still serves around that body: the floor a body
// consumes INSIDE the pool tier (`record_bridge_floor`) and the boundary
// fallback where the body abstains or does not exist (`bridge_fallback`) —
// both recorded for `interp_bridge_fallback_ledger`.

impl<'a> Interpreter<'a> {
    /// A POOL body reached by its IMPL name as a bare `Named` call (`string_slice` —
    /// how a lowered module body spells `string.slice`), with its arguments
    /// already evaluated, on the SAME resolution order a module-spelled call
    /// takes: the interp-native container op, then the lowered body, then the
    /// bridge only where the body abstains (#2185). Inside the pool tier the
    /// bridge is the floor a body consumes, as in `dispatch_module_resolved`.
    pub(crate) fn eval_named_stdlib_impl(
        &mut self,
        func: &'a almide_ir::IrFunction,
        m: Sym,
        f: Sym,
        evaled: Vec<Value>,
    ) -> Flow {
        if let Some(result) = self.eval_container_op(m.as_str(), f.as_str(), &evaled) {
            return result;
        }
        let floor_first = self.pool_depth > 0;
        if floor_first
            && let Some(result) = crate::bridge::dispatch(m.as_str(), f.as_str(), &evaled)
        {
            self.record_bridge_floor(m, f);
            return result;
        }
        let root = self.root_scope();
        let flow = self.call_pool_tier(func, evaled.clone(), &root);
        // #1226 return sync at the NAMED spelling too — the same body
        // reachable both ways must read back the same way. `func` is a pool
        // body by the caller's check: a program fn that shares an impl's
        // spelling is the user's code and never reaches this route (#3825).
        let flow = self.sync_at_pool_boundary(func, flow);
        match flow {
            Flow::Unsupported(why) => self.abstain_or_bridge(floor_first, m, f, &evaled, why),
            other => other,
        }
    }

    /// The body `module.func` runs for a call of `arity` arguments: the
    /// resolved body when it takes that many, else its ARITY-suffixed twin.
    /// An arity overload (`string.slice(s, start)` beside `string.slice(s,
    /// start, end)`): the frontend admits the short form without padding it,
    /// and the registry links it as its own impl under the arity suffix
    /// (`string.slice2`) — the same spelling the wasm leg routes to. Binding
    /// the short call to the long body left its last param unbound (`unbound
    /// variable VarId(11)` in the codepoint fixtures). `Err` says why no body
    /// can take this call.
    fn resolve_body_for_arity(
        &self,
        module: Sym,
        func: Sym,
        arity: usize,
    ) -> Result<(Sym, &'a almide_ir::IrFunction, bool), String> {
        let Some((func_def, gate_mut)) = self.resolve_lowered_body(module, func) else {
            // Capability FIRST, qualification after — the shape every other
            // `Flow::Unsupported` reason in this crate uses, and the one the
            // abstain ledger's class patterns key on. Leading with prose put
            // the name where no pattern could see it (#2333).
            return Err(format!("{}.{}: no lowered body", module, func));
        };
        if func_def.params.len() == arity {
            return Ok((func, func_def, gate_mut));
        }
        let twin = almide_base::intern::sym(&format!("{}{}", func.as_str(), arity));
        match self.resolve_lowered_body(module, twin) {
            Some((d, g)) if d.params.len() == arity => Ok((twin, d, g)),
            _ => Err(format!(
                "{}.{} called with {} argument(s); its lowered body takes {}",
                module,
                func,
                arity,
                func_def.params.len()
            )),
        }
    }

    /// The body could not answer (`why`). Inside the pool tier that IS the
    /// abstention (the bridge was already tried as the floor); at the
    /// boundary the bridge gets to answer it as the recorded fallback.
    fn abstain_or_bridge(&mut self, floor_first: bool, module: Sym, func: Sym, args: &[Value], why: String) -> Flow {
        if floor_first {
            Flow::Unsupported(why)
        } else {
            self.bridge_fallback(module, func, args, why)
        }
    }

    /// A bridge answer INSIDE the pool tier (#2185): the floor a body
    /// consumed, recorded under the same ledger as the boundary fallbacks so
    /// an arm no body and no fixture reaches is visibly dead.
    pub(crate) fn record_bridge_floor(&mut self, module: Sym, func: Sym) {
        if module.as_str() == "prim" {
            return;
        }
        let name = format!("{}.{}", module, func);
        let mut seen = self.bridge_fallbacks.borrow_mut();
        if !seen.iter().any(|(n, _)| *n == name) {
            seen.push((name, "floor: consumed inside a self-hosted body".to_string()));
        }
    }

    /// The bridge as the body's fallback (#2185): there is no lowered body, or
    /// it abstained — `why` says which. Answer from the hand-mirrored bridge
    /// when it has this name, recording the fallback so every bridge answer
    /// over the corpus is measurable (`interp_bridge_fallback_ledger`), else
    /// abstain with that reason.
    fn bridge_fallback(&mut self, module: Sym, func: Sym, args: &[Value], why: String) -> Flow {
        self.try_bridge(module, func, args, why).unwrap_or_else(Flow::Unsupported)
    }

    /// [`Self::bridge_fallback`] that hands `why` back (`Err`) when the
    /// bridge does not serve the name, so a later tier can still answer.
    fn try_bridge(&mut self, module: Sym, func: Sym, args: &[Value], why: String) -> Result<Flow, String> {
        match crate::bridge::dispatch(module.as_str(), func.as_str(), args) {
            Some(result) => {
                // The `prim.*` scalar floor is the spec's own vocabulary (the
                // MIR ops both backends render from), consumed BY the bodies —
                // not a mirror of one, so it is not a fallback to ledger.
                if module.as_str() != "prim" {
                    self.bridge_fallbacks.borrow_mut().push((format!("{}.{}", module, func), why));
                }
                Ok(result)
            }
            None => Err(why),
        }
    }

    /// A boundary call (`pool_depth == 0`) no lowered body can take: the
    /// bridge first, exactly as before, then the auto-imported module's own
    /// body (`stdlib_pool::bundled_body`) — the definition the driver loads
    /// for both backends when the registry names no impl (`list.range_step`,
    /// `int8.parse`). Consulted only where nothing else answers, so every
    /// call some tier already served keeps its answer and its provenance.
    /// The body runs as module code over `Value`s, reached with eagerly
    /// evaluated arguments, so a `mut` parameter abstains for the same
    /// reason the module path's gate gives (#1022).
    fn no_body_fallback(&mut self, module: Sym, func: Sym, args: Vec<Value>, why: String) -> Flow {
        let why = match self.try_bridge(module, func, &args, why) {
            Ok(answered) => return answered,
            Err(why) => why,
        };
        let Some(def) = self.bundled_fn(module, func) else {
            return Flow::Unsupported(why);
        };
        if def.params.len() != args.len() || def.params.iter().any(|p| p.is_mut) {
            return Flow::Unsupported(why);
        }
        let root = self.root_scope();
        let flow = self.call_function(def, args, &root);
        self.sync_at_pool_boundary(def, flow)
    }

    /// `module.func`'s auto-imported bundled body, its address recorded so
    /// the hop that runs it resolves bare sibling calls in that module.
    pub(crate) fn bundled_fn(&self, module: Sym, func: Sym) -> Option<&'a almide_ir::IrFunction> {
        let def = crate::stdlib_pool::bundled_body(module, func)?;
        self.bundled_space.borrow_mut().insert(def as *const almide_ir::IrFunction as usize, module);
        Some(def)
    }

    /// The lowered Almide body `module.func` resolves to, paired with whether
    /// the eager-dispatch `mut`-parameter gate applies to it.
    ///
    /// Three sources in order: the module's own fn table; a flattened
    /// top-level fn named exactly `func` (some stdlib helpers flatten) that is
    /// NOT the entry program's own — a `module.func` call never names a
    /// program-root fn, so a user `fn parse` must not capture `json.parse`
    /// (#2058: its body calls `json.parse`, which resolved back to itself and
    /// spun to fuel exhaustion; the module-identity class, #1087–#1094); and
    /// LAST the self-hosted stdlib body from the shared registry (stdlib_pool)
    /// — the SAME source the wasm leg links for this call name, lowered once
    /// and layered into `self.fns` at construction. Consulted after the two
    /// module-owned sources so a user module keeps its vote provenance; what
    /// a pool body itself cannot evaluate (a heap/effect prim outside the
    /// scalar floor) abstains from inside with that prim named — a skip,
    /// never a guess (the caller then tries the bridge, #2185) — so the mut
    /// gate does not apply to it.
    ///
    /// A `Hole` body is an intrinsic stub, not an interpretable definition:
    /// each source skips it and falls through to the next.
    fn resolve_lowered_body(&self, module: Sym, func: Sym) -> Option<(&'a almide_ir::IrFunction, bool)> {
        fn bodied(d: &&almide_ir::IrFunction) -> bool {
            !matches!(d.body.kind, almide_ir::IrExprKind::Hole)
        }
        if let Some(d) = self.module_fns.get(&(module, func)).copied().filter(bodied) {
            return Some((d, true));
        }
        // A flat-table fn another namespace owns never answers `module.func`:
        // the entry program's (space 0), or ANOTHER loaded module's — a
        // module's private `__` helper is flattened by its bare name, and
        // bigint's `__trunc(x: BigInt)` answered mono's `int.__trunc` (the
        // `S.__trunc` requirement of `bigint.from[S]` at S = Int) and read
        // `.neg` off an Int. The module-identity class again (#1087–#1094).
        let foreign = |d: &&almide_ir::IrFunction| {
            match self.fn_space.get(&(*d as *const almide_ir::IrFunction as usize)) {
                Some(0) => true,
                Some(&space) => self.program.modules.get(space as usize - 1).is_some_and(|m| m.name != module),
                None => false,
            }
        };
        if let Some(d) = self.fns.get(&func).copied().filter(bodied).filter(|d| !foreign(d)) {
            return Some((d, true));
        }
        // The registry's impl NAME is the pool's key, not a program-wide one:
        // a program-root fn spelled `int_to_string` is the user's, and taking
        // it from the flat table ran `int.to_string` as that fn (#3825 — a
        // user fn calling the stdlib it is spelled after recursed into
        // itself). The pool body first, by its own table; the flat table only
        // where the pool has none (a source that did not lower), and never
        // the entry program's own fn.
        let impl_name = crate::stdlib_pool::impl_fn(module, func)?;
        crate::stdlib_pool::body(impl_name)
            .or_else(|| self.fns.get(&impl_name).copied().filter(|d| !foreign(d)))
            .filter(bodied)
            .map(|d| (d, false))
    }
}
