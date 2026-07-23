// ── Interp-native container ops (non-HOF structural transforms) ──

impl<'a> Interpreter<'a> {
    /// Handle a non-HOF `(module, func)` container op. Returns `None` if this
    /// op is not interp-native (the caller falls through to the bridge / an
    /// almide-bodied fn).
    pub(crate) fn eval_container_op(
        &mut self,
        module: &str,
        func: &str,
        args: &[Value],
    ) -> Option<Flow> {
        // In-place container MUTATION cannot be faithfully modeled here: the
        // dispatch path evaluates every arg by VALUE before reaching us, so the
        // `var` binding identity of the receiver is already lost. These stdlib
        // ops have a `mut` receiver and (mostly) return Unit — the program reads
        // the EFFECT on the variable, not a returned value (e.g.
        // `for i in 0..100 { list.push(xs, ..) }` then indexes `xs`). Modeling
        // them functionally (returning a fresh container that the caller drops)
        // is silently WRONG and would emit a misleading third vote into the
        // cross-target oracle — strictly worse than an honest skip. So we report
        // `Unsupported`, which the 3-way gate logs as a reasoned skip. (The
        // FUNCTIONAL siblings that return a new container — `list.set`,
        // `list.insert`, `map.set`, `set.insert` — are NOT here and stay
        // supported.) Index-assign (`xs[i] = v`) keeps its binding and IS
        // modeled correctly via `IrStmtKind::IndexAssign`.
        if is_inplace_mutating_op(module, func) {
            return Some(Flow::Unsupported(format!(
                "in-place container mutation `{module}.{func}` (mut receiver; \
                 interp args are by-value so the binding cannot be written back)"
            )));
        }
        // Per-module dispatch below — grouping by `module` first (rather than
        // one flat `(module, func)` match) is behavior-preserving because every
        // arm was already keyed by a unique `(module, func)` literal pair, so
        // regrouping by the first element changes nothing observable.
        match module {
            "list" => self.eval_container_op_list(func, args),
            "map" => self.eval_container_op_map(func, args),
            "set" => self.eval_container_op_set(func, args),
            "option" => self.eval_container_op_option(func, args),
            "result" => self.eval_container_op_result(func, args),
            _ => None,
        }
    }

    // ── list ── each arm is a thin one-line dispatch to its own op method
    // (mirroring the pre-existing list_get/list_sum/… style below), so the
    // router itself stays a flat table instead of re-accumulating the
    // combined cyclomatic weight of every op's internal branching.
    fn eval_container_op_list(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        match func {
            "len" | "length" => Some(self.list_len(args)),
            "is_empty" => Some(self.list_is_empty(args)),
            "reverse" => Some(self.list_reverse(args)),
            "first" | "head" => Some(self.list_first(args)),
            "last" => Some(self.list_last(args)),
            "get" => Some(self.list_get(args)),
            "get_or" => Some(self.list_get_or(args)),
            "binary_search" => Some(self.list_binary_search(args)),
            "contains" => Some(self.list_contains(args)),
            // `list.append` is the FUNCTIONAL append (returns a new list); the
            // mutating `list.push` is intercepted by the in-place-mutation guard
            // above and never reaches here.
            "append" => Some(self.list_append(args)),
            "concat" => Some(self.list_concat(args)),
            // The aggregate/ordering ops are a second-tier sub-router — purely
            // to keep this router's own arm count (and cyclomatic weight)
            // under the per-function threshold; `func` still uniquely selects
            // exactly one op either way.
            _ => self.eval_container_op_list_agg(func, args),
        }
    }

    fn eval_container_op_list_agg(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        match func {
            "sum" => Some(self.list_sum(args)),
            "product" => Some(self.list_product(args)),
            "min" => Some(self.list_min_max(args, false)),
            "max" => Some(self.list_min_max(args, true)),
            "join" => Some(self.list_join(args)),
            "sort" => Some(self.list_sort(args)),
            "enumerate" => Some(self.list_enumerate(args)),
            _ => None,
        }
    }

    fn list_len(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(v) => match v.as_iter_items() {
                Some(items) => Flow::val(Value::Int(items.len() as i64)),
                None => Flow::Abort("internal: list.len on non-list".into()),
            },
            None => Flow::Abort("internal: list.len no arg".into()),
        }
    }

    fn list_is_empty(&mut self, args: &[Value]) -> Flow {
        match args.first().and_then(|v| v.as_iter_items()) {
            Some(items) => Flow::val(Value::Bool(items.is_empty())),
            None => Flow::Abort("internal: list.is_empty on non-list".into()),
        }
    }

    fn list_reverse(&mut self, args: &[Value]) -> Flow {
        match args.first().and_then(|v| v.as_iter_items()) {
            Some(mut items) => {
                items.reverse();
                Flow::val(Value::list(items))
            }
            None => Flow::Abort("internal: list.reverse on non-list".into()),
        }
    }

    fn list_first(&mut self, args: &[Value]) -> Flow {
        match args.first().and_then(|v| v.as_iter_items()) {
            Some(items) => Flow::val(Value::Option(items.first().cloned().map(Box::new))),
            None => Flow::Abort("internal: list.first on non-list".into()),
        }
    }

    fn list_last(&mut self, args: &[Value]) -> Flow {
        match args.first().and_then(|v| v.as_iter_items()) {
            Some(items) => Flow::val(Value::Option(items.last().cloned().map(Box::new))),
            None => Flow::Abort("internal: list.last on non-list".into()),
        }
    }

    // The native oracle IS Rust std's branchless binary_search (C-159: the
    // duplicate-key index is pinned to it on both backends), so the third
    // vote calls it directly on the extracted i64s.
    fn list_binary_search(&mut self, args: &[Value]) -> Flow {
        match (args.first().and_then(|v| v.as_iter_items()), args.get(1)) {
            (Some(items), Some(Value::Int(target))) => {
                let xs: Option<Vec<i64>> = items
                    .iter()
                    .map(|v| if let Value::Int(n) = v { Some(*n) } else { None })
                    .collect();
                match xs {
                    Some(xs) => Flow::val(Value::Option(
                        xs.binary_search(target).ok().map(|i| Box::new(Value::Int(i as i64))),
                    )),
                    None => Flow::Unsupported("list.binary_search non-Int elements".into()),
                }
            }
            _ => Flow::Abort("internal: list.binary_search bad args".into()),
        }
    }

    fn list_contains(&mut self, args: &[Value]) -> Flow {
        match (args.first().and_then(|v| v.as_iter_items()), args.get(1)) {
            (Some(items), Some(x)) => Flow::val(Value::Bool(items.contains(x))),
            _ => Flow::Abort("internal: list.contains bad args".into()),
        }
    }

    fn list_append(&mut self, args: &[Value]) -> Flow {
        match (args.first().and_then(|v| v.as_iter_items()), args.get(1)) {
            (Some(mut items), Some(x)) => {
                items.push(x.clone());
                Flow::val(Value::list(items))
            }
            _ => Flow::Abort("internal: list.append bad args".into()),
        }
    }

    fn list_concat(&mut self, args: &[Value]) -> Flow {
        match (
            args.first().and_then(|v| v.as_iter_items()),
            args.get(1).and_then(|v| v.as_iter_items()),
        ) {
            (Some(mut a), Some(b)) => {
                a.extend(b);
                Flow::val(Value::list(a))
            }
            _ => Flow::Abort("internal: list.concat bad args".into()),
        }
    }

    fn list_enumerate(&mut self, args: &[Value]) -> Flow {
        match args.first().and_then(|v| v.as_iter_items()) {
            Some(items) => Flow::val(Value::list(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| Value::tuple(vec![Value::Int(i as i64), v]))
                    .collect(),
            )),
            None => Flow::Abort("internal: list.enumerate on non-list".into()),
        }
    }

    // ── map ──
    fn eval_container_op_map(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        match func {
            "len" | "size" => Some(self.map_len(args)),
            "get" => Some(self.map_get(args)),
            "contains_key" | "has" => Some(self.map_contains_key(args)),
            // `map.set` is FUNCTIONAL (`-> Map`, returns a new map); the
            // mutating `map.insert` (`mut m, .. -> Unit`) is intercepted by the
            // in-place-mutation guard above.
            "set" => Some(self.map_set(args)),
            "keys" => Some(self.map_keys(args)),
            "values" => Some(self.map_values(args)),
            _ => None,
        }
    }

    fn map_len(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Map(e)) => Flow::val(Value::Int(e.len() as i64)),
            _ => Flow::Abort("internal: map.len on non-map".into()),
        }
    }

    fn map_get(&mut self, args: &[Value]) -> Flow {
        match (args.first(), args.get(1)) {
            (Some(Value::Map(e)), Some(k)) => Flow::val(Value::Option(
                e.iter().find(|(ek, _)| ek == k).map(|(_, v)| Box::new(v.clone())),
            )),
            _ => Flow::Abort("internal: map.get bad args".into()),
        }
    }

    fn map_contains_key(&mut self, args: &[Value]) -> Flow {
        match (args.first(), args.get(1)) {
            (Some(Value::Map(e)), Some(k)) => Flow::val(Value::Bool(e.iter().any(|(ek, _)| ek == k))),
            _ => Flow::Abort("internal: map.contains_key bad args".into()),
        }
    }

    fn map_set(&mut self, args: &[Value]) -> Flow {
        match (args.first(), args.get(1), args.get(2)) {
            (Some(Value::Map(e)), Some(k), Some(v)) => {
                let mut new = (**e).clone();
                crate::eval::map_insert(&mut new, k.clone(), v.clone());
                Flow::val(Value::Map(Rc::new(new)))
            }
            _ => Flow::Abort("internal: map.set bad args".into()),
        }
    }

    fn map_keys(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Map(e)) => Flow::val(Value::list(e.iter().map(|(k, _)| k.clone()).collect())),
            _ => Flow::Abort("internal: map.keys on non-map".into()),
        }
    }

    fn map_values(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Map(e)) => Flow::val(Value::list(e.iter().map(|(_, v)| v.clone()).collect())),
            _ => Flow::Abort("internal: map.values on non-map".into()),
        }
    }

    // ── set ──
    fn eval_container_op_set(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        match func {
            "len" | "size" => Some(self.set_len(args)),
            "contains" | "has" => Some(self.set_contains(args)),
            "insert" | "add" => Some(self.set_insert(args)),
            "to_list" => Some(self.set_to_list(args)),
            _ => None,
        }
    }

    fn set_len(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Set(e)) => Flow::val(Value::Int(e.len() as i64)),
            _ => Flow::Abort("internal: set.len on non-set".into()),
        }
    }

    fn set_contains(&mut self, args: &[Value]) -> Flow {
        match (args.first(), args.get(1)) {
            (Some(Value::Set(e)), Some(x)) => Flow::val(Value::Bool(e.contains(x))),
            _ => Flow::Abort("internal: set.contains bad args".into()),
        }
    }

    fn set_insert(&mut self, args: &[Value]) -> Flow {
        match (args.first(), args.get(1)) {
            (Some(Value::Set(e)), Some(x)) => {
                let mut new = (**e).clone();
                if !new.contains(x) {
                    new.push(x.clone());
                }
                Flow::val(Value::Set(Rc::new(new)))
            }
            _ => Flow::Abort("internal: set.insert bad args".into()),
        }
    }

    fn set_to_list(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Set(e)) => Flow::val(Value::list((**e).clone())),
            _ => Flow::Abort("internal: set.to_list on non-set".into()),
        }
    }

    // ── option ──
    fn eval_container_op_option(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        match func {
            "is_some" => Some(self.option_is_some(args)),
            "is_none" => Some(self.option_is_none(args)),
            "unwrap_or" => Some(self.option_unwrap_or(args)),
            "to_list" => Some(self.option_to_list(args)),
            "to_result" => Some(self.option_to_result(args)),
            _ => None,
        }
    }

    fn option_is_some(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Option(o)) => Flow::val(Value::Bool(o.is_some())),
            _ => Flow::Abort("internal: option.is_some on non-option".into()),
        }
    }

    fn option_is_none(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Option(o)) => Flow::val(Value::Bool(o.is_none())),
            _ => Flow::Abort("internal: option.is_none on non-option".into()),
        }
    }

    fn option_unwrap_or(&mut self, args: &[Value]) -> Flow {
        match (args.first(), args.get(1)) {
            (Some(Value::Option(Some(v))), _) => Flow::val((**v).clone()),
            (Some(Value::Option(None)), Some(d)) => Flow::val(d.clone()),
            _ => Flow::Abort("internal: option.unwrap_or bad args".into()),
        }
    }

    // some(v) → [v], none → [] (runtime/rs option.rs to_list)
    fn option_to_list(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Option(Some(v))) => Flow::val(Value::list(vec![(**v).clone()])),
            Some(Value::Option(None)) => Flow::val(Value::list(vec![])),
            _ => Flow::Abort("internal: option.to_list on non-option".into()),
        }
    }

    // some(v) → ok(v), none → err(msg) (runtime/rs option.rs to_result)
    fn option_to_result(&mut self, args: &[Value]) -> Flow {
        match (args.first(), args.get(1)) {
            (Some(Value::Option(Some(v))), _) => Flow::val(Value::Result(Ok(v.clone()))),
            (Some(Value::Option(None)), Some(msg)) => {
                Flow::val(Value::Result(Err(Box::new(msg.clone()))))
            }
            _ => Flow::Abort("internal: option.to_result bad args".into()),
        }
    }

    // ── result ──
    fn eval_container_op_result(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        match func {
            "is_ok" => Some(self.result_is_ok(args)),
            "is_err" => Some(self.result_is_err(args)),
            "unwrap_or" => Some(self.result_unwrap_or(args)),
            "to_option" => Some(self.result_to_option(args)),
            "to_err_option" => Some(self.result_to_err_option(args)),
            "flatten" => Some(self.result_flatten(args)),
            "collect" => Some(self.result_collect(args)),
            _ => None,
        }
    }

    fn result_is_ok(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Result(r)) => Flow::val(Value::Bool(r.is_ok())),
            _ => Flow::Abort("internal: result.is_ok on non-result".into()),
        }
    }

    fn result_is_err(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Result(r)) => Flow::val(Value::Bool(r.is_err())),
            _ => Flow::Abort("internal: result.is_err on non-result".into()),
        }
    }

    fn result_unwrap_or(&mut self, args: &[Value]) -> Flow {
        match (args.first(), args.get(1)) {
            (Some(Value::Result(Ok(v))), _) => Flow::val((**v).clone()),
            (Some(Value::Result(Err(_))), Some(d)) => Flow::val(d.clone()),
            _ => Flow::Abort("internal: result.unwrap_or bad args".into()),
        }
    }

    // ok(v) → some(v), err(_) → none (runtime/rs result.rs to_option)
    fn result_to_option(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Result(Ok(v))) => Flow::val(Value::Option(Some(v.clone()))),
            Some(Value::Result(Err(_))) => Flow::val(Value::Option(None)),
            _ => Flow::Abort("internal: result.to_option on non-result".into()),
        }
    }

    // ok(_) → none, err(e) → some(e) (runtime/rs result.rs to_err_option)
    fn result_to_err_option(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Result(Ok(_))) => Flow::val(Value::Option(None)),
            Some(Value::Result(Err(e))) => Flow::val(Value::Option(Some(e.clone()))),
            _ => Flow::Abort("internal: result.to_err_option on non-result".into()),
        }
    }

    // ok(inner) → inner, err(e) → err(e) (runtime/rs result.rs flatten)
    fn result_flatten(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Result(Ok(inner))) => Flow::val((**inner).clone()),
            Some(Value::Result(Err(e))) => Flow::val(Value::Result(Err(e.clone()))),
            _ => Flow::Abort("internal: result.flatten on non-result".into()),
        }
    }

    // collect(List[Result[T,E]]) → all ok → ok(List[T]), else err(List[E]) of
    // EVERY err (the native runtime's partition-style collect).
    fn result_collect(&mut self, args: &[Value]) -> Flow {
        match args.first().and_then(|v| v.as_iter_items()) {
            Some(items) => {
                let mut oks = Vec::new();
                let mut errs = Vec::new();
                for it in items {
                    match it {
                        Value::Result(Ok(v)) => oks.push((*v).clone()),
                        Value::Result(Err(e)) => errs.push((*e).clone()),
                        _ => {
                            return Flow::Abort(
                                "internal: result.collect non-result element".into(),
                            )
                        }
                    }
                }
                if errs.is_empty() {
                    Flow::val(Value::Result(Ok(Box::new(Value::list(oks)))))
                } else {
                    Flow::val(Value::Result(Err(Box::new(Value::list(errs)))))
                }
            }
            None => Flow::Abort("internal: result.collect on non-list".into()),
        }
    }

    // get_or(xs, i, default) — the OOB/negative index yields the default
    // (runtime/rs list.rs get_or; the Value-level twin of list_get's some/none).
    pub(crate) fn list_get_or(&mut self, args: &[Value]) -> Flow {
        match (args.first().and_then(|v| v.as_iter_items()), args.get(1), args.get(2)) {
            (Some(items), Some(Value::Int(i)), Some(default)) => {
                let i = *i;
                if i < 0 || (i as usize) >= items.len() {
                    Flow::val(default.clone())
                } else {
                    Flow::val(items[i as usize].clone())
                }
            }
            _ => Flow::Abort("internal: list.get_or bad args".into()),
        }
    }

    fn list_get(&mut self, args: &[Value]) -> Flow {
        match (args.first().and_then(|v| v.as_iter_items()), args.get(1)) {
            (Some(items), Some(Value::Int(i))) => {
                let i = *i;
                if i < 0 || (i as usize) >= items.len() {
                    Flow::val(Value::Option(None))
                } else {
                    Flow::val(Value::Option(Some(Box::new(items[i as usize].clone()))))
                }
            }
            _ => Flow::Abort("internal: list.get bad args".into()),
        }
    }

    fn list_sum(&mut self, args: &[Value]) -> Flow {
        match args.first().and_then(|v| v.as_iter_items()) {
            Some(items) => {
                // Int sum or Float sum depending on element kind.
                if items.iter().all(|v| matches!(v, Value::Int(_))) {
                    let s: i64 = items
                        .iter()
                        .map(|v| if let Value::Int(n) = v { *n } else { 0 })
                        .fold(0i64, |a, b| a.wrapping_add(b));
                    Flow::val(Value::Int(s))
                } else {
                    let s: f64 = items
                        .iter()
                        .map(|v| match v {
                            Value::Float(f) => *f,
                            Value::Int(n) => *n as f64,
                            _ => 0.0,
                        })
                        .sum();
                    Flow::val(Value::Float(s))
                }
            }
            None => Flow::Abort("internal: list.sum on non-list".into()),
        }
    }

    /// `list.product` — two's-complement WRAPPING fold (identity 1), mirroring
    /// the native `almide_rt_list_product` (`fold(1, wrapping_mul)`) and the
    /// wasm `i64.mul` accumulator. The wrapping fold is the language's
    /// integer-overflow law (C-056); std `.product()` would diverge under
    /// debug overflow-checks. `list.product`'s stdlib type is `List[Int]`, so
    /// the Float arm is unreachable in practice but kept symmetric with
    /// `list_sum` for non-typed/error-recovery IR.
    fn list_product(&mut self, args: &[Value]) -> Flow {
        match args.first().and_then(|v| v.as_iter_items()) {
            Some(items) => {
                if items.iter().all(|v| matches!(v, Value::Int(_))) {
                    let p: i64 = items
                        .iter()
                        .map(|v| if let Value::Int(n) = v { *n } else { 1 })
                        .fold(1i64, |a, b| a.wrapping_mul(b));
                    Flow::val(Value::Int(p))
                } else {
                    let p: f64 = items
                        .iter()
                        .map(|v| match v {
                            Value::Float(f) => *f,
                            Value::Int(n) => *n as f64,
                            _ => 1.0,
                        })
                        .product();
                    Flow::val(Value::Float(p))
                }
            }
            None => Flow::Abort("internal: list.product on non-list".into()),
        }
    }

    fn list_join(&mut self, args: &[Value]) -> Flow {
        match (args.first().and_then(|v| v.as_iter_items()), args.get(1)) {
            (Some(items), Some(Value::Str(sep))) => {
                let parts: Vec<String> = items.iter().map(|v| v.display_bare()).collect();
                Flow::val(Value::str(parts.join(sep.as_str())))
            }
            _ => Flow::Abort("internal: list.join bad args".into()),
        }
    }

    fn list_sort(&mut self, args: &[Value]) -> Flow {
        match args.first().and_then(|v| v.as_iter_items()) {
            Some(mut items) => {
                let mut ok = true;
                // TOTAL order (C-055): Float compares by `total_cmp`, matching
                // the backends' `List[Float]` totalOrder. `partial_cmp_val`
                // would leave NaNs in place and break agreement with native ==
                // wasm.
                items.sort_by(|a, b| {
                    a.total_cmp_val(b).unwrap_or_else(|| {
                        ok = false;
                        std::cmp::Ordering::Equal
                    })
                });
                if ok {
                    Flow::val(Value::list(items))
                } else {
                    Flow::Abort("internal: list.sort on non-comparable elements".into())
                }
            }
            None => Flow::Abort("internal: list.sort on non-list".into()),
        }
    }

    /// `list.min` / `list.max` over a totally-ordered element list → Option[A].
    /// Float uses totalOrder (`total_cmp_val`), so NaN is the max and `-0.0`
    /// the lesser of the two zeros — agreeing with the backends' `_float`
    /// runtime variants and the scalar-`float.min`/`max` asymmetry (those keep
    /// C-049 NaN-ignoring). Empty → none. C-055.
    fn list_min_max(&mut self, args: &[Value], want_max: bool) -> Flow {
        match args.first().and_then(|v| v.as_iter_items()) {
            Some(items) => {
                let mut best: Option<Value> = None;
                for v in items {
                    let take = match &best {
                        None => true,
                        Some(b) => match v.total_cmp_val(b) {
                            Some(ord) => if want_max { ord.is_gt() } else { ord.is_lt() },
                            // Non-comparable element: abstain rather than vote
                            // wrong (a wrong third vote is worse than a skip).
                            None => return Flow::Unsupported("list.min/max on non-comparable elements".into()),
                        },
                    };
                    if take { best = Some(v); }
                }
                Flow::val(Value::Option(best.map(Box::new)))
            }
            None => Flow::Abort("internal: list.min/max on non-list".into()),
        }
    }
}

/// The stdlib container ops with a `mut` receiver — they mutate the receiver
/// IN PLACE and the program observes the effect on the bound `var`, not a
/// returned value. The interp cannot model these (args arrive by value, so the
/// binding is unreachable from the dispatch point); it reports `Unsupported`
/// for them so the 3-way oracle records an honest skip instead of a wrong vote.
///
/// Source of truth: the `mut`-receiver Unit/Option-returning functions in
/// `stdlib/{list,map,string,bytes}.almd`. The FUNCTIONAL siblings (`list.set`,
/// `list.insert`, `map.set`, `set.insert` — all `-> NewContainer`) are NOT
/// listed and remain fully supported.
fn is_inplace_mutating_op(module: &str, func: &str) -> bool {
    matches!(
        (module, func),
        ("list", "push")
            | ("list", "pop")
            | ("list", "clear")
            | ("map", "insert")
            | ("map", "delete")
            | ("map", "clear")
            | ("string", "push")
            | ("string", "clear")
            | ("bytes", "push")
            | ("bytes", "set_at")
            | ("bytes", "copy_within")
    )
}
