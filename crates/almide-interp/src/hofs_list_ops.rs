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
        // In-place container MUTATION cannot be modeled from HERE: by the time
        // args are values, the `var` binding identity of the receiver is gone,
        // and these stdlib ops have a `mut` receiver that (mostly) returns Unit
        // — the program reads the EFFECT on the variable, not a returned value
        // (e.g. `for i in 0..100 { list.push(xs, ..) }` then indexes `xs`).
        // Modeling them functionally (returning a fresh container the caller
        // drops) is silently WRONG and would emit a misleading third vote into
        // the cross-target oracle — strictly worse than an honest skip.
        //
        // `eval_module_call` therefore intercepts the whole family one step
        // EARLIER, while the receiver is still an expression, and writes the
        // mutation back into its binding (`inplace.rs`). Reaching this arm means
        // the call arrived on a path with no receiver expression left to name —
        // the residual-UFCS `Method` target, which evaluates its object first.
        // That stays an honest skip. (The FUNCTIONAL siblings that return a new
        // container — `list.set`, `list.insert`, `map.set`, `set.insert` — are
        // NOT in the predicate and are handled below either way.)
        if is_inplace_mutating_op(module, func) {
            return Some(Flow::Unsupported(format!(
                "in-place container mutation `{module}.{func}` reached through a \
                 residual-UFCS method call (the receiver was evaluated to a value \
                 before dispatch, so its binding cannot be written back)"
            )));
        }
        // ADDRESS-UNIFORM receivers stay in the pool tier (#1226): a live
        // block address reaching a native container op means a POOL body is
        // mid-flight (its values are addresses by design) — the native impls
        // expect real containers and would abort. Falling through hands the
        // call to the self-host body, which reads the SAME arena through the
        // prim floor (`list_len` = handle + load32). First-arg only: every
        // container op's receiver is its first argument, and a genuinely
        // scalar first arg under a container module name never collides
        // (fall-through is the pre-existing behavior for unknown ops anyway).
        if let Some(Value::Int(i)) = args.first() {
            if u32::try_from(*i).ok().and_then(|a| self.heap.kind(a)).is_some() {
                return None;
            }
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
            // First index of an equal element — `PartialEq` equality, the same
            // relation `contains` uses (measured against native on the corpus;
            // found abstaining via args.option's `list.index_of(args, long)`,
            // #1217 — the gap predates the args work and was simply unexercised).
            "index_of" => Some(self.list_index_of(args)),
            // `list.append` is the FUNCTIONAL append (returns a new list); the
            // mutating `list.push` is intercepted by the in-place-mutation guard
            // above and never reaches here.
            "append" => Some(self.list_append(args)),
            // A fresh EMPTY list on every leg: native clamps and reserves
            // (runtime/rs/src/list.rs::almide_rt_list_with_capacity) and the
            // wasm self-host ignores `cap` outright (stdlib/list_make.almd) —
            // the reservation is unobservable (C-034), so the cap value never
            // matters here either. Without this arm the interp descended into
            // the self-host body and abstained on `prim.alloc_list` (C-277's
            // fixture, #1416).
            "with_capacity" => Some(Flow::val(Value::list(Vec::new()))),
            "concat" => Some(self.list_concat(args)),
            // Structural, element-type-blind — served natively so a heap-element
            // list never reaches the pool's Int-declared core impl (the
            // wrong-impl guard walled chunk/windows on List[String], #1226).
            // Semantics mirror stdlib/list_chunk.almd EXACTLY: chunk(0) /
            // windows(0) abort in the T6 form; a negative chunk size is one
            // chunk holding everything; windows past the length are none.
            "chunk" => Some(self.list_chunk(args)),
            "windows" | "window" => Some(self.list_windows(args)),
            // The aggregate/ordering ops are a second-tier sub-router — purely
            // to keep this router's own arm count (and cyclomatic weight)
            // under the per-function threshold; `func` still uniquely selects
            // exactly one op either way.
            _ => self.eval_container_op_list_agg(func, args),
        }
    }

    /// `list.chunk(xs, n)` — stdlib/list_chunk.almd's exact domain rules:
    /// n == 0 aborts with v0's line, n < 0 is `chunks(huge usize)` = one
    /// chunk holding everything, and the count is the overflow-proof
    /// `total/n + (total % n != 0)`.
    fn list_chunk(&mut self, args: &[Value]) -> Flow {
        let (Some(Value::List(xs)), Some(Value::Int(n))) = (args.first(), args.get(1)) else {
            return Flow::Abort("internal: list.chunk bad args".into());
        };
        if *n == 0 {
            return Flow::Abort("chunk size must be positive".into());
        }
        let total = xs.len() as i64;
        let n = if *n < 0 { total.max(1) } else { *n };
        let out: Vec<Value> = xs
            .chunks(usize::try_from(n).unwrap_or(usize::MAX).max(1))
            .map(|c| Value::list(c.to_vec()))
            .collect();
        Flow::val(Value::list(out))
    }

    /// `list.windows(xs, n)` — n == 0 aborts, n < 0 or n > len is the empty
    /// list, else the len-n+1 overlapping sub-slices.
    fn list_windows(&mut self, args: &[Value]) -> Flow {
        let (Some(Value::List(xs)), Some(Value::Int(n))) = (args.first(), args.get(1)) else {
            return Flow::Abort("internal: list.windows bad args".into());
        };
        if *n == 0 {
            return Flow::Abort("window size must be positive".into());
        }
        let total = xs.len() as i64;
        if *n < 0 || *n > total {
            return Flow::val(Value::list(Vec::new()));
        }
        let out: Vec<Value> = xs
            .windows(*n as usize)
            .map(|w| Value::list(w.to_vec()))
            .collect();
        Flow::val(Value::list(out))
    }

    fn list_dedup(&mut self, args: &[Value]) -> Flow {
        let Some(Value::List(xs)) = args.first() else {
            return Flow::Abort("internal: list.dedup bad args".into());
        };
        let mut r: Vec<Value> = Vec::new();
        for x in xs.iter() {
            if r.last() != Some(x) {
                r.push(x.clone());
            }
        }
        Flow::val(Value::list(r))
    }

    fn list_unique(&mut self, args: &[Value]) -> Flow {
        let Some(Value::List(xs)) = args.first() else {
            return Flow::Abort("internal: list.unique bad args".into());
        };
        let mut r: Vec<Value> = Vec::new();
        for x in xs.iter() {
            if !r.contains(x) {
                r.push(x.clone());
            }
        }
        Flow::val(Value::list(r))
    }

    fn list_flatten(&mut self, args: &[Value]) -> Flow {
        let Some(Value::List(xss)) = args.first() else {
            return Flow::Abort("internal: list.flatten bad args".into());
        };
        let mut r: Vec<Value> = Vec::new();
        for xs in xss.iter() {
            match xs {
                Value::List(inner) => r.extend(inner.iter().cloned()),
                other => {
                    return Flow::Abort(format!(
                        "internal: list.flatten over a {} element",
                        other.type_name()
                    ))
                }
            }
        }
        Flow::val(Value::list(r))
    }

    fn list_intersperse(&mut self, args: &[Value]) -> Flow {
        let (Some(Value::List(xs)), Some(sep)) = (args.first(), args.get(1)) else {
            return Flow::Abort("internal: list.intersperse bad args".into());
        };
        let mut r: Vec<Value> = Vec::with_capacity(xs.len().saturating_mul(2));
        for (i, x) in xs.iter().enumerate() {
            if i > 0 {
                r.push(sep.clone());
            }
            r.push(x.clone());
        }
        Flow::val(Value::list(r))
    }

    fn list_zip(&mut self, args: &[Value]) -> Flow {
        let (Some(Value::List(xs)), Some(Value::List(ys))) = (args.first(), args.get(1)) else {
            return Flow::Abort("internal: list.zip bad args".into());
        };
        let r: Vec<Value> = xs
            .iter()
            .zip(ys.iter())
            .map(|(a, b)| Value::tuple(vec![a.clone(), b.clone()]))
            .collect();
        Flow::val(Value::list(r))
    }

    fn eval_container_op_list_agg(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        match func {
            "sum" => Some(self.list_sum(args)),
            // Structural, element-type-blind, mirroring the v0 runtime
            // exactly (runtime/rs/src/list.rs): dedup drops CONSECUTIVE
            // equals, unique keeps first occurrences, both under the same
            // `PartialEq` the interp's `==` already models (f64 NaN != NaN).
            "dedup" => Some(self.list_dedup(args)),
            "unique" => Some(self.list_unique(args)),
            "flatten" => Some(self.list_flatten(args)),
            "intersperse" => Some(self.list_intersperse(args)),
            "zip" => Some(self.list_zip(args)),
            "product" => Some(self.list_product(args)),
            "min" => Some(self.list_min_max(args, false)),
            "max" => Some(self.list_min_max(args, true)),
            "join" => Some(self.list_join(args)),
            "sort" => Some(self.list_sort(args)),
            "enumerate" => Some(self.list_enumerate(args)),
            _ => self.eval_container_op_list_slice(func, args),
        }
    }

    /// The SLICE / MODIFIER family — index-keyed structural ops that build a new
    /// list. Added so the fixtures for C-034/C-155/C-163/C-164 evaluate on the
    /// interp oracle instead of abstaining: an abstention is a hole in the
    /// executable spec, and the ledger exists to make widening the glue the
    /// preferred fix rather than recording the hole (see crates/almide-interp/CLAUDE.md).
    ///
    /// Every clamp below mirrors the C-034 rule the other two legs implement: an
    /// UNSIGNED count saturates (a negative one is enormous as a `usize`, so
    /// `take(-1)` is the whole list and `drop(-1)` is empty), and an INDEX is
    /// unsigned too, so a negative or huge index takes the no-op path.
    fn eval_container_op_list_slice(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        self.eval_list_slice_a(func, args)
            .or_else(|| self.eval_list_slice_b(func, args))
    }

    /// The first half of `eval_container_op_list_slice`'s arm table.
    ///
    /// Extracted from `eval_container_op_list_slice` (arm-table halving): arms verbatim and in
    /// source order, so the router's order is the only ordering that matters.
    fn eval_list_slice_a(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        let items = args.first().and_then(|v| v.as_iter_items())?;
        let n = items.len();
        // An unsigned count, saturated to `n` — `-1 as usize` is enormous.
        let count = |i: i64| -> usize { if i < 0 { n } else { (i as usize).min(n) } };
        let int_arg = |k: usize| -> Option<i64> {
            match args.get(k) { Some(Value::Int(i)) => Some(*i), _ => None }
        };
        let out = |v: Vec<Value>| Some(Flow::val(Value::list(v)));
        match func {
            "take" => out(items[..count(int_arg(1)?)].to_vec()),
            "drop" => out(items[count(int_arg(1)?)..].to_vec()),
            "take_end" => { let k = count(int_arg(1)?); out(items[n - k..].to_vec()) }
            "drop_end" => { let k = count(int_arg(1)?); out(items[..n - k].to_vec()) }
            "tail" => out(if n == 0 { Vec::new() } else { items[1..].to_vec() }),
            _ => None,
        }
    }

    /// The second half of `eval_container_op_list_slice`'s arm table.
    ///
    /// Extracted from `eval_container_op_list_slice` (arm-table halving): arms verbatim and in
    /// source order, so the router's order is the only ordering that matters.
    fn eval_list_slice_b(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        let items = args.first().and_then(|v| v.as_iter_items())?;
        let n = items.len();
        // An unsigned count, saturated to `n` — `-1 as usize` is enormous.
        let count = |i: i64| -> usize { if i < 0 { n } else { (i as usize).min(n) } };
        // An unsigned index; `None` when out of range (the no-op / default path).
        let index = |i: i64| -> Option<usize> {
            if i < 0 { None } else { let u = i as usize; (u < n).then_some(u) }
        };
        let int_arg = |k: usize| -> Option<i64> {
            match args.get(k) { Some(Value::Int(i)) => Some(*i), _ => None }
        };
        let out = |v: Vec<Value>| Some(Flow::val(Value::list(v)));
        match func {
            "slice" => {
                let start = count(int_arg(1)?);
                let end = count(int_arg(2)?).max(start);
                out(items[start..end].to_vec())
            }
            "set" => {
                let mut v = items.clone();
                if let Some(i) = index(int_arg(1)?) { v[i] = args.get(2)?.clone(); }
                out(v)
            }
            "remove_at" => {
                let mut v = items.clone();
                if let Some(i) = index(int_arg(1)?) { v.remove(i); }
                out(v)
            }
            "insert" => {
                // v0 clamps the position to [0, n]; a negative one appends.
                let raw = int_arg(1)?;
                let at = if raw < 0 { n } else { (raw as usize).min(n) };
                let mut v = items.clone();
                v.insert(at, args.get(2)?.clone());
                out(v)
            }
            "swap" => {
                let mut v = items.clone();
                match (index(int_arg(1)?), index(int_arg(2)?)) {
                    (Some(i), Some(j)) => v.swap(i, j),
                    // An out-of-range index is a no-op, not an abort.
                    _ => {}
                }
                out(v)
            }
            _ => None,
        }
    }

    // Read-only accessors borrow via `as_iter_slice` (no per-call clone of the
    // whole container — see the doc there); the `as_iter_items` arm keeps the
    // Range fallback byte-identical.
    fn list_len(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(v) => match v.as_iter_slice().map(<[Value]>::len).or_else(|| v.as_iter_items().map(|i| i.len())) {
                Some(n) => Flow::val(Value::Int(n as i64)),
                // A Range that as_iter_items refuses is the OVER-CAP span
                // (C-197): both backends materialize and take the defined OOM
                // abort, so any answer here is a wrong vote — abstain.
                None if matches!(v, Value::Range { .. }) => Flow::Unsupported(
                    "range materialization beyond the interp cap (both backends take the C-197 resource path)".into(),
                ),
                None => Flow::Abort("internal: list.len on non-list".into()),
            },
            None => Flow::Abort("internal: list.len no arg".into()),
        }
    }

    fn list_is_empty(&mut self, args: &[Value]) -> Flow {
        match args.first().and_then(|v| v.as_iter_slice().map(<[Value]>::is_empty).or_else(|| v.as_iter_items().map(|i| i.is_empty()))) {
            Some(b) => Flow::val(Value::Bool(b)),
            None if matches!(args.first(), Some(Value::Range { .. })) => Flow::Unsupported(
                "range materialization beyond the interp cap (both backends take the C-197 resource path)".into(),
            ),
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
        match args.first() {
            Some(v) => match v.as_iter_slice() {
                Some(items) => Flow::val(Value::Option(items.first().cloned().map(Box::new))),
                None => match v.as_iter_items() {
                    Some(items) => Flow::val(Value::Option(items.first().cloned().map(Box::new))),
                    None => Flow::Abort("internal: list.first on non-list".into()),
                },
            },
            None => Flow::Abort("internal: list.first on non-list".into()),
        }
    }

    fn list_last(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(v) => match v.as_iter_slice() {
                Some(items) => Flow::val(Value::Option(items.last().cloned().map(Box::new))),
                None => match v.as_iter_items() {
                    Some(items) => Flow::val(Value::Option(items.last().cloned().map(Box::new))),
                    None => Flow::Abort("internal: list.last on non-list".into()),
                },
            },
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
        match (args.first(), args.get(1)) {
            (Some(v), Some(x)) => match v.as_iter_slice() {
                Some(items) => Flow::val(Value::Bool(items.contains(x))),
                None => match v.as_iter_items() {
                    Some(items) => Flow::val(Value::Bool(items.contains(x))),
                    None => Flow::Abort("internal: list.contains bad args".into()),
                },
            },
            _ => Flow::Abort("internal: list.contains bad args".into()),
        }
    }

    fn list_index_of(&mut self, args: &[Value]) -> Flow {
        fn pos(items: &[Value], x: &Value) -> Flow {
            Flow::val(Value::Option(
                items.iter().position(|e| e == x).map(|i| Box::new(Value::Int(i as i64))),
            ))
        }
        match (args.first(), args.get(1)) {
            (Some(v), Some(x)) => match v.as_iter_slice() {
                Some(items) => pos(items, x),
                None => match v.as_iter_items() {
                    Some(items) => pos(&items, x),
                    None => Flow::Abort("internal: list.index_of bad args".into()),
                },
            },
            _ => Flow::Abort("internal: list.index_of bad args".into()),
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

    // get_or(xs, i, default) — the OOB/negative index yields the default
    // (runtime/rs list.rs get_or; the Value-level twin of list_get's some/none).
    pub(crate) fn list_get_or(&mut self, args: &[Value]) -> Flow {
        fn at(items: &[Value], i: i64, default: &Value) -> Flow {
            if i < 0 || (i as usize) >= items.len() {
                Flow::val(default.clone())
            } else {
                Flow::val(items[i as usize].clone())
            }
        }
        match (args.first(), args.get(1), args.get(2)) {
            (Some(v), Some(Value::Int(i)), Some(default)) => match v.as_iter_slice() {
                Some(items) => at(items, *i, default),
                None => match v.as_iter_items() {
                    Some(items) => at(&items, *i, default),
                    None => Flow::Abort("internal: list.get_or bad args".into()),
                },
            },
            _ => Flow::Abort("internal: list.get_or bad args".into()),
        }
    }

    fn list_get(&mut self, args: &[Value]) -> Flow {
        fn at(items: &[Value], i: i64) -> Flow {
            if i < 0 || (i as usize) >= items.len() {
                Flow::val(Value::Option(None))
            } else {
                Flow::val(Value::Option(Some(Box::new(items[i as usize].clone()))))
            }
        }
        match (args.first(), args.get(1)) {
            (Some(v), Some(Value::Int(i))) => match v.as_iter_slice() {
                Some(items) => at(items, *i),
                None => match v.as_iter_items() {
                    Some(items) => at(&items, *i),
                    None => Flow::Abort("internal: list.get bad args".into()),
                },
            },
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
/// returned value. They are intercepted BEFORE the eager argument evaluation, so
/// the receiver is still an expression whose binding can be written back
/// (`inplace.rs`); a receiver shape with no binding to write to reports
/// `Unsupported`, so the 3-way oracle records an honest skip, never a wrong vote.
///
/// Source of truth: the receiver-mutating Unit/Option-returning functions in
/// `stdlib/{list,map,string,bytes}.almd`. The FUNCTIONAL siblings (`list.set`,
/// `list.insert`, `map.set`, `set.insert` — all `-> NewContainer`) are NOT
/// listed and remain fully supported.
///
/// `bytes` is matched by PREFIX rather than by name. Only three of its mutators
/// (`push`, `set_at`, `copy_within`) spell `mut` on the receiver; the ~40 in the
/// `set_*` / `append_*` / `write_*` families do not — they are `@intrinsic`s whose
/// mutation lives in the native signature (`&mut Vec<u8>`), invisible to the `.almd`
/// declaration. So an enumerated list here had nothing to be checked against and
/// silently fell behind: `bytes.set_f32_le` reported the generic "unknown
/// capability" abstain instead of naming what it actually is, which is what the
/// ledger gate caught on `mutable_global_bytes_arena`. Every `set_*`/`append_*`/
/// `write_*` in `stdlib/bytes.almd` returns Unit and mutates in place, and the
/// functional `bytes.set(b, i, v) -> Bytes` is excluded by the underscore — the
/// prefix is the whole rule, and a new family member is covered on arrival.
///
/// NOT here: `bytes.heap_save` / `bytes.heap_restore`. They take a checkpoint
/// Int, not a buffer, so there is no receiver to write back — they are the
/// wasm-only arena pair, and native's no-ops are the vote the bridge casts
/// (`bridge.rs::bytes_fn`).
pub(crate) fn is_inplace_mutating_op(module: &str, func: &str) -> bool {
    if module == "bytes"
        && (func.starts_with("set_") || func.starts_with("append_") || func.starts_with("write_"))
    {
        return true;
    }
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
            | ("bytes", "clear")
            | ("bytes", "fill")
            | ("bytes", "copy_from")
            | ("bytes", "copy_within")
    )
}
