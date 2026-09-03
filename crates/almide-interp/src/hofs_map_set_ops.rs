// ── hofs_list_ops.rs, part 2: the map / set / option / result ops ──
//
// include!-spliced into `hofs.rs` at module level next to `hofs_list_ops.rs`
// (the 800-line file discipline, #1856). The non-HOF structural container ops
// over everything that is not a list; `eval_container_op` in the list part is
// the single entry that fans out here by module.

impl<'a> Interpreter<'a> {
    // ── map ──
    fn eval_container_op_map(&mut self, func: &str, args: &[Value]) -> Option<Flow> {
        match func {
            // The empty constructor — same value the `[:]` literal evaluates
            // to (eval.rs::EmptyMap). Without this arm the interp descended
            // into the self-host body and abstained on `prim.alloc_map`
            // (C-277's fixture, #1416; `with_capacity`/`set.new` likewise).
            "new" if args.is_empty() => Some(Flow::val(Value::Map(Rc::new(Vec::new())))),
            "len" | "size" => Some(self.map_len(args)),
            "get" => Some(self.map_get(args)),
            // `map.contains` is the stdlib's KEY-membership name
            // (stdlib/map.almd: `fn contains[K, V](m, key) -> Bool`) —
            // same op as the `contains_key`/`has` aliases.
            "contains" | "contains_key" | "has" => Some(self.map_contains_key(args)),
            // `map.set` is FUNCTIONAL (`-> Map`, returns a new map); the
            // mutating `map.insert` (`mut m, .. -> Unit`) is intercepted by the
            // in-place-mutation guard above.
            "set" => Some(self.map_set(args)),
            "keys" => Some(self.map_keys(args)),
            "values" => Some(self.map_values(args)),
            // Served natively so a String-keyed map never reaches the pool's
            // scalar core impl (the wrong-impl guard walled these on
            // Map[String, _], #1226). All three are element-type-blind over
            // the insertion-ordered entry list both backends share.
            "get_or" => Some(self.map_get_or(args)),
            "from_list" => Some(self.map_from_list(args)),
            "entries" | "to_list" => Some(self.map_entries(args)),
            // v0's AlmideMap semantics exactly: remove keeps order, merge is
            // a's entries then b's upserted in (FIRST position, LAST value).
            "remove" => Some(self.map_remove(args)),
            "merge" => Some(self.map_merge(args)),
            _ => None,
        }
    }

    fn map_remove(&mut self, args: &[Value]) -> Flow {
        match (args.first(), args.get(1)) {
            (Some(Value::Map(e)), Some(k)) => Flow::val(Value::Map(Rc::new(
                e.iter().filter(|(ek, _)| ek != k).cloned().collect(),
            ))),
            _ => Flow::Abort("internal: map.remove bad args".into()),
        }
    }

    fn map_merge(&mut self, args: &[Value]) -> Flow {
        match (args.first(), args.get(1)) {
            (Some(Value::Map(a)), Some(Value::Map(b))) => {
                let mut out = (**a).clone();
                for (k, v) in b.iter() {
                    crate::eval::map_insert(&mut out, k.clone(), v.clone());
                }
                Flow::val(Value::Map(Rc::new(out)))
            }
            _ => Flow::Abort("internal: map.merge bad args".into()),
        }
    }

    fn map_get_or(&mut self, args: &[Value]) -> Flow {
        match (args.first(), args.get(1), args.get(2)) {
            (Some(Value::Map(e)), Some(k), Some(d)) => Flow::val(
                e.iter().find(|(ek, _)| ek == k).map(|(_, v)| v.clone()).unwrap_or_else(|| d.clone()),
            ),
            _ => Flow::Abort("internal: map.get_or bad args".into()),
        }
    }

    /// Insertion order with upsert-in-place — v0's AlmideMap `from_list`
    /// (FIRST position, LAST value), the same rule `map_insert` implements.
    fn map_from_list(&mut self, args: &[Value]) -> Flow {
        let Some(Value::List(pairs)) = args.first() else {
            return Flow::Abort("internal: map.from_list bad args".into());
        };
        let mut out: Vec<(Value, Value)> = Vec::with_capacity(pairs.len());
        for p in pairs.iter() {
            let Value::Tuple(kv) = p else {
                return Flow::Abort("internal: map.from_list on a non-tuple element".into());
            };
            let (Some(k), Some(v)) = (kv.first(), kv.get(1)) else {
                return Flow::Abort("internal: map.from_list on a non-pair tuple".into());
            };
            crate::eval::map_insert(&mut out, k.clone(), v.clone());
        }
        Flow::val(Value::Map(Rc::new(out)))
    }

    fn map_entries(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Map(e)) => Flow::val(Value::list(
                e.iter().map(|(k, v)| Value::tuple(vec![k.clone(), v.clone()])).collect(),
            )),
            _ => Flow::Abort("internal: map.entries on non-map".into()),
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
            // The empty constructor — see the `map.new` arm above.
            "new" if args.is_empty() => Some(Flow::val(Value::Set(Rc::new(Vec::new())))),
            // Dedup preserving FIRST-occurrence insertion order — the same
            // order both backends print (spec/wasm_cross/compound_repr_interp
            // pins `set.from_list([3, 1, 2, 1, 3])` as `{3, 1, 2}`), and the
            // same rule `set_insert` below already implements one element at
            // a time.
            "from_list" => Some(self.set_from_list(args)),
            "len" | "size" => Some(self.set_len(args)),
            "contains" | "has" => Some(self.set_contains(args)),
            "insert" | "add" => Some(self.set_insert(args)),
            "to_list" => Some(self.set_to_list(args)),
            // The C-014 order rules the set_insertion_order fixture pins:
            // union is a-order then b's new elements; intersection and
            // difference walk a; symmetric difference is (a-b) then (b-a).
            "union" => Some(self.set_union(args)),
            "intersection" => Some(self.set_intersection(args)),
            "difference" => Some(self.set_difference(args)),
            "symmetric_difference" => Some(self.set_symmetric_difference(args)),
            _ => None,
        }
    }

    fn set_pair<'v>(args: &'v [Value], op: &str) -> Result<(&'v Rc<Vec<Value>>, &'v Rc<Vec<Value>>), Flow> {
        match (args.first(), args.get(1)) {
            (Some(Value::Set(a)), Some(Value::Set(b))) => Ok((a, b)),
            _ => Err(Flow::Abort(format!("internal: set.{op} bad args"))),
        }
    }

    fn set_union(&mut self, args: &[Value]) -> Flow {
        let (a, b) = match Self::set_pair(args, "union") {
            Ok(p) => p,
            Err(f) => return f,
        };
        let mut out = (**a).clone();
        for x in b.iter() {
            if !out.contains(x) {
                out.push(x.clone());
            }
        }
        Flow::val(Value::Set(Rc::new(out)))
    }

    fn set_intersection(&mut self, args: &[Value]) -> Flow {
        let (a, b) = match Self::set_pair(args, "intersection") {
            Ok(p) => p,
            Err(f) => return f,
        };
        let out: Vec<Value> = a.iter().filter(|x| b.contains(x)).cloned().collect();
        Flow::val(Value::Set(Rc::new(out)))
    }

    fn set_difference(&mut self, args: &[Value]) -> Flow {
        let (a, b) = match Self::set_pair(args, "difference") {
            Ok(p) => p,
            Err(f) => return f,
        };
        let out: Vec<Value> = a.iter().filter(|x| !b.contains(x)).cloned().collect();
        Flow::val(Value::Set(Rc::new(out)))
    }

    fn set_symmetric_difference(&mut self, args: &[Value]) -> Flow {
        let (a, b) = match Self::set_pair(args, "symmetric_difference") {
            Ok(p) => p,
            Err(f) => return f,
        };
        let mut out: Vec<Value> = a.iter().filter(|x| !b.contains(x)).cloned().collect();
        out.extend(b.iter().filter(|x| !a.contains(x)).cloned());
        Flow::val(Value::Set(Rc::new(out)))
    }

    fn set_len(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Set(e)) => Flow::val(Value::Int(e.len() as i64)),
            _ => Flow::Abort("internal: set.len on non-set".into()),
        }
    }

    fn set_from_list(&mut self, args: &[Value]) -> Flow {
        match args.first().and_then(|v| v.as_iter_items()) {
            Some(items) => {
                let mut out: Vec<Value> = Vec::new();
                for x in items {
                    if !out.contains(&x) {
                        out.push(x);
                    }
                }
                Flow::val(Value::Set(Rc::new(out)))
            }
            None => Flow::Abort("internal: set.from_list on non-list".into()),
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
            "partition" => Some(self.result_partition(args)),
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

    // partition(List[Result[T,E]]) → (oks, errs) in list order — the substance
    // the removed collect wrapped (ADR-0007).
    fn result_partition(&mut self, args: &[Value]) -> Flow {
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
                                "internal: result.partition non-result element".into(),
                            )
                        }
                    }
                }
                Flow::val(Value::Tuple(std::rc::Rc::new(vec![Value::list(oks), Value::list(errs)])))
            }
            None => Flow::Abort("internal: result.partition on non-list".into()),
        }
    }
}
