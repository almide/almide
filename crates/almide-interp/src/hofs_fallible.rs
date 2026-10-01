// ── hofs.rs, part 5: the fallible-matrix carriers beyond the first seven ──
//
// include!-spliced into `hofs.rs` (the 800-line file discipline). #3163 widened
// ADR-0006's fallible form from seven `list` cells to every callback-taking
// fn of list / map / set / option (`almide_types::fallible_hofs`). Each body
// here is its total sibling plus the FIRST-ERR SHORT-CIRCUIT `try_step` gives:
// the first `err` the callback answers is the whole call's `err`, no element
// after it reaches the callback, and a full pass is `ok`-wrapped. The bodies
// in `stdlib/{list,map,set,option}.almd` are the specification.

/// What a predicate carrier does with the answers it collected, and where it
/// stops asking.
#[derive(Clone, Copy, PartialEq)]
enum TryPred {
    Any,
    All,
    Count,
    FindIndex,
    Partition,
    TakeWhile,
    DropWhile,
}

impl TryPred {
    /// Does this answer end the traversal (the total form's own short-circuit)?
    fn stops_at(self, b: bool) -> bool {
        match self {
            TryPred::Any | TryPred::FindIndex => b,
            TryPred::All | TryPred::TakeWhile | TryPred::DropWhile => !b,
            TryPred::Count | TryPred::Partition => false,
        }
    }
}

fn ok_val(v: Value) -> Flow {
    Flow::val(Value::Result(Ok(Box::new(v))))
}

impl<'a> Interpreter<'a> {
    /// Route a `module.__fallible_<hof>` that is not one of the first seven.
    fn eval_hof_fallible_matrix(&mut self, m: &str, f: &str, a: &[Value]) -> Flow {
        let Some(hof) = f.strip_prefix("__fallible_") else {
            return Flow::Unsupported(format!("HOF {m}.{f}"));
        };
        match m {
            "list" => self.eval_try_list(hof, a),
            "map" => self.eval_try_map_mod(hof, a),
            "set" => self.eval_try_set(hof, a),
            "option" => self.eval_try_option(hof, a),
            _ => Flow::Unsupported(format!("HOF {m}.{f}")),
        }
    }

    fn eval_try_list(&mut self, hof: &str, a: &[Value]) -> Flow {
        let pred = match hof {
            "any" => Some(TryPred::Any),
            "all" => Some(TryPred::All),
            "count" => Some(TryPred::Count),
            "find_index" => Some(TryPred::FindIndex),
            "partition" => Some(TryPred::Partition),
            "take_while" => Some(TryPred::TakeWhile),
            "drop_while" => Some(TryPred::DropWhile),
            _ => None,
        };
        if let Some(p) = pred {
            return self.try_pred_items(a, p);
        }
        match hof {
            "reduce" => self.hof_try_reduce(a),
            "scan" => self.hof_try_scan(a),
            "zip_with" => self.hof_try_zip_with(a),
            "update" => self.hof_try_list_update(a),
            "iterate" => self.hof_try_iterate(a),
            "sort_by" | "group_by" | "unique_by" => self.hof_try_keyed(hof, a),
            _ => Flow::Unsupported(format!("HOF list.__fallible_{hof}")),
        }
    }

    /// Ask the predicate per element (`arg_rows[i]` is its argument list),
    /// stopping where `p` stops or at the first err. `Ok(answers)` for the
    /// visited prefix.
    fn try_ask(&mut self, clo: &Rc<crate::Closure>, arg_rows: Vec<Vec<Value>>, p: TryPred) -> Result<Vec<bool>, Flow> {
        let mut answers = Vec::new();
        for row in arg_rows {
            let b = matches!(self.try_step(clo, row)?, Value::Bool(true));
            answers.push(b);
            if p.stops_at(b) {
                break;
            }
        }
        Ok(answers)
    }

    /// The predicate cells over a list / set receiver.
    fn try_pred_items(&mut self, a: &[Value], p: TryPred) -> Flow {
        let items = match Self::recv_items(a) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(a, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let rows = items.iter().map(|x| vec![x.clone()]).collect();
        let answers = match self.try_ask(&clo, rows, p) {
            Ok(v) => v,
            Err(f) => return f,
        };
        let split = answers.iter().position(|b| !b).unwrap_or(items.len());
        ok_val(match p {
            TryPred::Any => Value::Bool(answers.contains(&true)),
            TryPred::All => Value::Bool(!answers.contains(&false)),
            TryPred::Count => Value::Int(answers.iter().filter(|b| **b).count() as i64),
            TryPred::FindIndex => match answers.last() {
                Some(true) => Value::Option(Some(Box::new(Value::Int(answers.len() as i64 - 1)))),
                _ => Value::Option(None),
            },
            TryPred::Partition => {
                let (mut yes, mut no) = (Vec::new(), Vec::new());
                for (x, b) in items.into_iter().zip(answers) {
                    if b { yes.push(x) } else { no.push(x) }
                }
                Value::tuple(vec![Value::list(yes), Value::list(no)])
            }
            TryPred::TakeWhile => Value::list(items[..split].to_vec()),
            TryPred::DropWhile => Value::list(items[split..].to_vec()),
        })
    }

    fn hof_try_reduce(&mut self, a: &[Value]) -> Flow {
        let items = match Self::recv_items(a) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(a, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut iter = items.into_iter();
        let Some(mut acc) = iter.next() else {
            return ok_val(Value::Option(None));
        };
        for x in iter {
            acc = match self.try_step(&clo, vec![acc, x]) {
                Ok(v) => v,
                Err(f) => return f,
            };
        }
        ok_val(Value::Option(Some(Box::new(acc))))
    }

    fn hof_try_scan(&mut self, a: &[Value]) -> Flow {
        let items = match Self::recv_items(a) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let Some(mut acc) = a.get(1).cloned() else {
            return Flow::Abort("internal: __fallible_scan missing init".into());
        };
        let clo = match Self::recv_closure(a, 2) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out = Vec::new();
        for x in items {
            acc = match self.try_step(&clo, vec![acc, x]) {
                Ok(v) => v,
                Err(f) => return f,
            };
            out.push(acc.clone());
        }
        ok_val(Value::list(out))
    }

    fn hof_try_zip_with(&mut self, a: &[Value]) -> Flow {
        let (Some(xs), Some(ys)) = (
            a.first().and_then(|v| v.as_iter_items()),
            a.get(1).and_then(|v| v.as_iter_items()),
        ) else {
            return Flow::Abort("internal: __fallible_zip_with receivers not iterable".into());
        };
        let clo = match Self::recv_closure(a, 2) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out = Vec::new();
        for (x, y) in xs.into_iter().zip(ys) {
            match self.try_step(&clo, vec![x, y]) {
                Ok(v) => out.push(v),
                Err(f) => return f,
            }
        }
        ok_val(Value::list(out))
    }

    fn hof_try_list_update(&mut self, a: &[Value]) -> Flow {
        let mut items = match Self::recv_items(a) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let Some(Value::Int(i)) = a.get(1) else {
            return Flow::Abort("internal: __fallible_update index not an Int".into());
        };
        let clo = match Self::recv_closure(a, 2) {
            Ok(c) => c,
            Err(f) => return f,
        };
        if let Some(slot) = usize::try_from(*i).ok().filter(|&i| i < items.len()) {
            items[slot] = match self.try_step(&clo, vec![items[slot].clone()]) {
                Ok(v) => v,
                Err(f) => return f,
            };
        }
        ok_val(Value::list(items))
    }

    /// `__fallible_iterate(seed, f, n)` — `f` runs exactly `n` times, as in
    /// `list.iterate`; the last result is computed and dropped.
    fn hof_try_iterate(&mut self, a: &[Value]) -> Flow {
        let Some(mut x) = a.first().cloned() else {
            return Flow::Abort("internal: __fallible_iterate missing seed".into());
        };
        let clo = match Self::recv_closure(a, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let Some(Value::Int(n)) = a.get(2) else {
            return Flow::Abort("internal: __fallible_iterate count not an Int".into());
        };
        let mut out = Vec::new();
        for _ in 0..(*n).max(0) {
            out.push(x.clone());
            x = match self.try_step(&clo, vec![x]) {
                Ok(v) => v,
                Err(f) => return f,
            };
        }
        ok_val(Value::list(out))
    }

    /// `sort_by` / `group_by` / `unique_by`: every key first, once per element
    /// in list order (the first err stops there), then the total form's own
    /// arrangement over the computed keys.
    fn hof_try_keyed(&mut self, hof: &str, a: &[Value]) -> Flow {
        let items = match Self::recv_items(a) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(a, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut keyed: Vec<(Value, Value)> = Vec::with_capacity(items.len());
        for x in items {
            match self.try_step(&clo, vec![x.clone()]) {
                Ok(k) => keyed.push((k, x)),
                Err(f) => return f,
            }
        }
        ok_val(match hof {
            "sort_by" => {
                let order = crate::value::TotalOrder::new(&self.variant_tags);
                keyed.sort_by(|(ka, _), (kb, _)| order.cmp(ka, kb).unwrap_or(std::cmp::Ordering::Equal));
                Value::list(keyed.into_iter().map(|(_, x)| x).collect())
            }
            "group_by" => {
                let mut groups: Vec<(Value, Vec<Value>)> = Vec::new();
                for (k, x) in keyed {
                    match groups.iter_mut().find(|(gk, _)| *gk == k) {
                        Some((_, g)) => g.push(x),
                        None => groups.push((k, vec![x])),
                    }
                }
                Value::Map(Rc::new(groups.into_iter().map(|(k, g)| (k, Value::list(g))).collect()))
            }
            _ => {
                let mut seen: Vec<Value> = Vec::new();
                let mut out = Vec::new();
                for (k, x) in keyed {
                    if !seen.contains(&k) {
                        seen.push(k);
                        out.push(x);
                    }
                }
                Value::list(out)
            }
        })
    }
}

include!("hofs_fallible_containers.rs");
