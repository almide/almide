// ── hofs.rs, part 6: the map / set / option fallible carriers (#3163) ──
//
// include!-spliced via `hofs_fallible.rs`. A `Value::Map` is its insertion-
// ordered `(k, v)` vec and a `Value::Set` its insertion-ordered element vec
// (the AlmideMap determinism contract), so a walk over the backing vec IS the
// order the stdlib bodies walk, and the first err is the same element.

impl<'a> Interpreter<'a> {
    fn recv_entries(a: &[Value]) -> Result<Rc<Vec<(Value, Value)>>, Flow> {
        match a.first() {
            Some(Value::Map(e)) => Ok(e.clone()),
            _ => Err(Flow::Abort("internal: map carrier receiver not a Map".into())),
        }
    }

    fn eval_try_map_mod(&mut self, hof: &str, a: &[Value]) -> Flow {
        match hof {
            "any" => self.try_pred_entries(a, TryPred::Any),
            "all" => self.try_pred_entries(a, TryPred::All),
            "count" => self.try_pred_entries(a, TryPred::Count),
            "find" => self.try_pred_entries(a, TryPred::FindIndex),
            "filter" => self.try_pred_entries(a, TryPred::Partition),
            "map" => self.hof_try_map_values(a),
            "fold" => self.hof_try_map_fold(a),
            "update" => self.hof_try_map_update(a, false),
            "upsert" => self.hof_try_map_update(a, true),
            _ => Flow::Unsupported(format!("HOF map.__fallible_{hof}")),
        }
    }

    /// The `(k, v) -> Bool` cells. `FindIndex` answers the entry itself and
    /// `Partition` the kept entries, which is `find` / `filter`.
    fn try_pred_entries(&mut self, a: &[Value], p: TryPred) -> Flow {
        let entries = match Self::recv_entries(a) {
            Ok(e) => e,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(a, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let rows = entries.iter().map(|(k, v)| vec![k.clone(), v.clone()]).collect();
        let answers = match self.try_ask(&clo, rows, p) {
            Ok(v) => v,
            Err(f) => return f,
        };
        ok_val(match p {
            TryPred::Any => Value::Bool(answers.contains(&true)),
            TryPred::All => Value::Bool(!answers.contains(&false)),
            TryPred::Count => Value::Int(answers.iter().filter(|b| **b).count() as i64),
            TryPred::FindIndex => match answers.last() {
                Some(true) => {
                    let (k, v) = entries[answers.len() - 1].clone();
                    Value::Option(Some(Box::new(Value::tuple(vec![k, v]))))
                }
                _ => Value::Option(None),
            },
            _ => {
                let kept = entries.iter().zip(answers).filter(|(_, b)| *b).map(|(e, _)| e.clone()).collect();
                Value::Map(Rc::new(kept))
            }
        })
    }

    fn hof_try_map_values(&mut self, a: &[Value]) -> Flow {
        let entries = match Self::recv_entries(a) {
            Ok(e) => e,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(a, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out = Vec::with_capacity(entries.len());
        for (k, v) in entries.iter() {
            match self.try_step(&clo, vec![v.clone()]) {
                Ok(nv) => out.push((k.clone(), nv)),
                Err(f) => return f,
            }
        }
        ok_val(Value::Map(Rc::new(out)))
    }

    fn hof_try_map_fold(&mut self, a: &[Value]) -> Flow {
        let entries = match Self::recv_entries(a) {
            Ok(e) => e,
            Err(f) => return f,
        };
        let Some(mut acc) = a.get(1).cloned() else {
            return Flow::Abort("internal: map.__fallible_fold missing init".into());
        };
        let clo = match Self::recv_closure(a, 2) {
            Ok(c) => c,
            Err(f) => return f,
        };
        for (k, v) in entries.iter() {
            acc = match self.try_step(&clo, vec![acc, k.clone(), v.clone()]) {
                Ok(nv) => nv,
                Err(f) => return f,
            };
        }
        ok_val(acc)
    }

    /// `update(m, key, f)` / `upsert(m, key, init, f)`: a present key's value
    /// through `f` in place; an absent one unchanged (update) or appended as
    /// `init` (upsert), without calling `f`.
    fn hof_try_map_update(&mut self, a: &[Value], upsert: bool) -> Flow {
        let entries = match Self::recv_entries(a) {
            Ok(e) => e,
            Err(f) => return f,
        };
        let Some(key) = a.get(1).cloned() else {
            return Flow::Abort("internal: map carrier missing key".into());
        };
        let clo = match Self::recv_closure(a, if upsert { 3 } else { 2 }) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out = (*entries).clone();
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some(pair) => {
                pair.1 = match self.try_step(&clo, vec![pair.1.clone()]) {
                    Ok(v) => v,
                    Err(f) => return f,
                };
            }
            None if upsert => match a.get(2) {
                Some(init) => out.push((key, init.clone())),
                None => return Flow::Abort("internal: map.__fallible_upsert missing init".into()),
            },
            None => {}
        }
        ok_val(Value::Map(Rc::new(out)))
    }

    fn eval_try_set(&mut self, hof: &str, a: &[Value]) -> Flow {
        match hof {
            "any" => self.try_pred_items(a, TryPred::Any),
            "all" => self.try_pred_items(a, TryPred::All),
            "fold" => self.hof_try_fold(a),
            "map" | "filter" => {
                let flow = if hof == "map" { self.hof_try_map(a) } else { self.hof_try_filter(a) };
                // The list carrier's `ok(list)`, rebuilt as the set the
                // stdlib body's `set.from_list` makes (dedup, first kept).
                match flow {
                    Flow::Value(Value::Result(Ok(v))) => match *v {
                        Value::List(xs) => {
                            let mut out: Vec<Value> = Vec::new();
                            for x in xs.iter() {
                                if !out.contains(x) {
                                    out.push(x.clone());
                                }
                            }
                            ok_val(Value::Set(Rc::new(out)))
                        }
                        other => ok_val(other),
                    },
                    other => other,
                }
            }
            _ => Flow::Unsupported(format!("HOF set.__fallible_{hof}")),
        }
    }

    fn eval_try_option(&mut self, hof: &str, a: &[Value]) -> Flow {
        let Some(Value::Option(o)) = a.first() else {
            return Flow::Abort("internal: option carrier receiver not an Option".into());
        };
        let clo = match Self::recv_closure(a, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let some = |v: Value| Value::Option(Some(Box::new(v)));
        match (hof, o) {
            ("unwrap_or_else", Some(x)) => ok_val((**x).clone()),
            ("or_else", Some(x)) => ok_val(some((**x).clone())),
            ("unwrap_or_else" | "or_else", None) => match self.try_step(&clo, vec![]) {
                Ok(v) => ok_val(v),
                Err(f) => f,
            },
            ("map" | "flat_map" | "filter", None) => ok_val(Value::Option(None)),
            ("map" | "flat_map" | "filter", Some(x)) => match self.try_step(&clo, vec![(**x).clone()]) {
                Ok(v) => ok_val(match hof {
                    "map" => some(v),
                    "flat_map" => v,
                    _ if matches!(v, Value::Bool(true)) => some((**x).clone()),
                    _ => Value::Option(None),
                }),
                Err(f) => f,
            },
            _ => Flow::Unsupported(format!("HOF option.__fallible_{hof}")),
        }
    }
}
