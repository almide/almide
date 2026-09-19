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
    /// A stdlib IMPL name reached as a bare `Named` call (`string_slice` —
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
        // reachable both ways must read back the same way. Pool bodies only:
        // a program fn that happens to share an impl name keeps the fixture
        // tier's raw address model.
        let flow = if self.pool_fns.contains(&func.name) {
            self.sync_at_pool_boundary(func, flow)
        } else {
            flow
        };
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
        match crate::bridge::dispatch(module.as_str(), func.as_str(), args) {
            Some(result) => {
                // The `prim.*` scalar floor is the spec's own vocabulary (the
                // MIR ops both backends render from), consumed BY the bodies —
                // not a mirror of one, so it is not a fallback to ledger.
                if module.as_str() != "prim" {
                    self.bridge_fallbacks.borrow_mut().push((format!("{}.{}", module, func), why));
                }
                result
            }
            None => Flow::Unsupported(why),
        }
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
        let program_root = |d: &&almide_ir::IrFunction| {
            self.fn_space.get(&(*d as *const almide_ir::IrFunction as usize)) == Some(&0)
        };
        if let Some(d) = self.fns.get(&func).copied().filter(bodied).filter(|d| !program_root(d)) {
            return Some((d, true));
        }
        let impl_name = crate::stdlib_pool::impl_fn(module, func)?;
        self.fns.get(&impl_name).copied().filter(bodied).map(|d| (d, false))
    }
}
