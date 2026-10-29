// ── hofs.rs, part 7: the order-matrix carriers and the sequence cells ──
//
// include!-spliced via `hofs_fallible.rs`. The list `_by` / `_with` cells of
// sort / min / max / binary_search (R-list-06, -17, -19) in their fallible
// form, and the set / map sequence cells (R-L0-42), which the stdlib bodies
// write once over `set.to_list` / `map.to_list` — so here too they hand the
// receiver's insertion-ordered vec to the list carrier. Each list body below
// walks the same loop as its `stdlib/list.almd` carrier, so the callback sees
// the same calls in the same order and the first err is the same one.

impl<'a> Interpreter<'a> {
    /// `set.__fallible_<hof>` / `map.__fallible_<hof>` for a sequence cell —
    /// a cell the receiver's own arms do not serve that the list matrix has
    /// (`almide_types::fallible_hofs`, not a name list here): the list carrier
    /// over the members / the `(k, v)` entry tuples.
    fn try_sequence_cell(&mut self, hof: &str, a: &[Value]) -> Flow {
        let items = match a.first() {
            Some(Value::Set(xs)) => (**xs).clone(),
            Some(Value::Map(es)) => es.iter().map(|(k, v)| Value::tuple(vec![k.clone(), v.clone()])).collect(),
            _ => return Flow::Abort(format!("internal: __fallible_{hof} receiver not a Set or Map")),
        };
        let mut args = a.to_vec();
        args[0] = Value::list(items);
        self.eval_hof_list_try(&format!("__fallible_{hof}"), &args)
    }

    /// `__key_order` of stdlib/list.almd: equal keys are 0, otherwise the
    /// order `list.sort_by` sorts them in.
    fn key_order(&self, a: &Value, b: &Value) -> i64 {
        if a == b {
            return 0;
        }
        match crate::value::TotalOrder::new(&self.variant_tags).cmp(a, b) {
            Some(std::cmp::Ordering::Less) => -1,
            _ => 1,
        }
    }

    fn eval_try_order(&mut self, hof: &str, a: &[Value]) -> Flow {
        match hof {
            "sort_with" => self.hof_try_sort_with(a),
            "min_by" => self.hof_try_pick_by(a, -1),
            "max_by" => self.hof_try_pick_by(a, 1),
            "min_with" => self.hof_try_pick_with(a, -1),
            "max_with" => self.hof_try_pick_with(a, 1),
            "binary_search_by" => self.hof_try_binary_search(a, true),
            "binary_search_with" => self.hof_try_binary_search(a, false),
            _ => Flow::Unsupported(format!("HOF list.__fallible_{hof}")),
        }
    }

    fn try_cmp(&mut self, clo: &Rc<crate::Closure>, x: &Value, y: &Value) -> Result<i64, Flow> {
        match self.try_step(clo, vec![x.clone(), y.clone()])? {
            Value::Int(c) => Ok(c),
            other => Err(Flow::Abort(format!("internal: comparator returned {other:?}, not an Int"))),
        }
    }

    /// The top-down merge sort of `sort_with`: halves at n / 2, and an element
    /// of the right run goes first only when `cmp(left, right) > 0`.
    fn try_merge_sort(&mut self, clo: &Rc<crate::Closure>, xs: Vec<Value>) -> Result<Vec<Value>, Flow> {
        if xs.len() <= 1 {
            return Ok(xs);
        }
        let half = xs.len() / 2;
        let left = self.try_merge_sort(clo, xs[..half].to_vec())?;
        let right = self.try_merge_sort(clo, xs[half..].to_vec())?;
        let (mut i, mut j) = (0, 0);
        let mut out = Vec::with_capacity(left.len() + right.len());
        while i < left.len() || j < right.len() {
            match (left.get(i), right.get(j)) {
                (Some(x), Some(y)) => {
                    if self.try_cmp(clo, x, y)? > 0 {
                        out.push(y.clone());
                        j += 1;
                    } else {
                        out.push(x.clone());
                        i += 1;
                    }
                }
                (Some(x), None) => {
                    out.push(x.clone());
                    i += 1;
                }
                (None, Some(y)) => {
                    out.push(y.clone());
                    j += 1;
                }
                (None, None) => break,
            }
        }
        Ok(out)
    }

    fn hof_try_sort_with(&mut self, a: &[Value]) -> Flow {
        let items = match Self::recv_items(a) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(a, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        match self.try_merge_sort(&clo, items) {
            Ok(out) => ok_val(Value::list(out)),
            Err(f) => f,
        }
    }

    /// `min_by` (want = -1) / `max_by` (want = 1): every key first, in order,
    /// then the first element whose key no later key is strictly past.
    fn hof_try_pick_by(&mut self, a: &[Value], want: i64) -> Flow {
        let items = match Self::recv_items(a) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(a, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut keys = Vec::with_capacity(items.len());
        for x in &items {
            match self.try_step(&clo, vec![x.clone()]) {
                Ok(k) => keys.push(k),
                Err(f) => return f,
            }
        }
        let mut best: Option<usize> = None;
        for (i, k) in keys.iter().enumerate() {
            if best.is_none_or(|b| self.key_order(k, &keys[b]) == want) {
                best = Some(i);
            }
        }
        ok_val(Value::Option(best.map(|i| Box::new(items[i].clone()))))
    }

    /// `min_with` (want = -1) / `max_with` (want = 1): `cmp(x, held)` per later
    /// element, replacing the held one only when strictly past it.
    fn hof_try_pick_with(&mut self, a: &[Value], want: i64) -> Flow {
        let items = match Self::recv_items(a) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(a, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut best: Option<Value> = None;
        for x in items {
            best = match best {
                None => Some(x),
                Some(b) => match self.try_cmp(&clo, &x, &b) {
                    Ok(c) if c * want > 0 => Some(x),
                    Ok(_) => Some(b),
                    Err(f) => return f,
                },
            };
        }
        ok_val(Value::Option(best.map(Box::new)))
    }

    /// `binary_search_by(xs, target, f)` / `binary_search_with(xs, target,
    /// cmp)`: the branchless loop of `list.binary_search`, then one check.
    fn hof_try_binary_search(&mut self, a: &[Value], by_key: bool) -> Flow {
        let items = match Self::recv_items(a) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let Some(target) = a.get(1).cloned() else {
            return Flow::Abort("internal: binary search missing target".into());
        };
        let clo = match Self::recv_closure(a, 2) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let order = |s: &mut Self, e: &Value| -> Result<i64, Flow> {
            if by_key {
                let k = s.try_step(&clo, vec![e.clone()])?;
                Ok(s.key_order(&k, &target))
            } else {
                s.try_cmp(&clo, e, &target)
            }
        };
        let (mut base, mut size) = (0usize, items.len());
        while size > 1 {
            let half = size / 2;
            let mid = base + half;
            match order(self, &items[mid]) {
                Ok(c) if c > 0 => {}
                Ok(_) => base = mid,
                Err(f) => return f,
            }
            size -= half;
        }
        if items.is_empty() {
            return ok_val(Value::Option(None));
        }
        match order(self, &items[base]) {
            Ok(0) => ok_val(Value::Option(Some(Box::new(Value::Int(base as i64))))),
            Ok(_) => ok_val(Value::Option(None)),
            Err(f) => f,
        }
    }
}
