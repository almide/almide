// ── hofs.rs, part 3: the option / result / set HOFs ──
//
// include!-spliced into `hofs.rs` at module level (#1856). The closure-taking
// combinators over the carrier types (`option.map`, `result.map_err`, …) and
// over sets.

impl<'a> Interpreter<'a> {
    // ── option HOFs ────────────────────────────────────────────

    fn hof_option_map(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Option(Some(v))) => {
                let clo = match Self::recv_closure(args, 1) {
                    Ok(c) => c,
                    Err(f) => return f,
                };
                let r = val!(self.apply_closure(&clo, vec![(**v).clone()]));
                Flow::val(Value::Option(Some(Box::new(r))))
            }
            Some(Value::Option(None)) => Flow::val(Value::Option(None)),
            _ => Flow::Abort("internal: option.map on non-Option".into()),
        }
    }

    fn hof_option_flat_map(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Option(Some(v))) => {
                let clo = match Self::recv_closure(args, 1) {
                    Ok(c) => c,
                    Err(f) => return f,
                };
                self.apply_closure(&clo, vec![(**v).clone()])
            }
            Some(Value::Option(None)) => Flow::val(Value::Option(None)),
            _ => Flow::Abort("internal: option.flat_map on non-Option".into()),
        }
    }

    fn hof_option_filter(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Option(Some(v))) => {
                let clo = match Self::recv_closure(args, 1) {
                    Ok(c) => c,
                    Err(f) => return f,
                };
                let keep = val!(self.apply_closure(&clo, vec![(**v).clone()]));
                if matches!(keep, Value::Bool(true)) {
                    Flow::val(Value::Option(Some(v.clone())))
                } else {
                    Flow::val(Value::Option(None))
                }
            }
            Some(Value::Option(None)) => Flow::val(Value::Option(None)),
            _ => Flow::Abort("internal: option.filter on non-Option".into()),
        }
    }

    fn hof_option_unwrap_or_else(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Option(Some(v))) => Flow::val((**v).clone()),
            Some(Value::Option(None)) => {
                let clo = match Self::recv_closure(args, 1) {
                    Ok(c) => c,
                    Err(f) => return f,
                };
                self.apply_closure(&clo, vec![])
            }
            _ => Flow::Abort("internal: option.unwrap_or_else on non-Option".into()),
        }
    }

    fn hof_option_or_else(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(opt @ Value::Option(Some(_))) => Flow::val(opt.clone()),
            Some(Value::Option(None)) => {
                let clo = match Self::recv_closure(args, 1) {
                    Ok(c) => c,
                    Err(f) => return f,
                };
                self.apply_closure(&clo, vec![])
            }
            _ => Flow::Abort("internal: option.or_else on non-Option".into()),
        }
    }

    // ── result HOFs ────────────────────────────────────────────

    fn hof_result_map(&mut self, args: &[Value], map_err: bool) -> Flow {
        match args.first() {
            Some(Value::Result(res)) => {
                let clo = match Self::recv_closure(args, 1) {
                    Ok(c) => c,
                    Err(f) => return f,
                };
                match (res, map_err) {
                    (Ok(v), false) => {
                        let r = val!(self.apply_closure(&clo, vec![(**v).clone()]));
                        Flow::val(Value::Result(Ok(Box::new(r))))
                    }
                    (Err(e), true) => {
                        let r = val!(self.apply_closure(&clo, vec![(**e).clone()]));
                        Flow::val(Value::Result(Err(Box::new(r))))
                    }
                    (other, _) => Flow::val(Value::Result(other.clone())),
                }
            }
            _ => Flow::Abort("internal: result.map on non-Result".into()),
        }
    }

    // Ok(v) kept iff pred(v), else Err(err_val — args[2]); Err(e) propagated.
    // pred never runs on the Err arm (result_map.almd result_filter's contract;
    // the wasm twins _h/base share it).
    fn hof_result_filter(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Result(Ok(v))) => {
                let clo = match Self::recv_closure(args, 1) {
                    Ok(c) => c,
                    Err(f) => return f,
                };
                let Some(err_val) = args.get(2).cloned() else {
                    return Flow::Abort("internal: result.filter missing err_val".into());
                };
                let verdict = val!(self.apply_closure(&clo, vec![(**v).clone()]));
                match verdict {
                    Value::Bool(true) => Flow::val(Value::Result(Ok(v.clone()))),
                    Value::Bool(false) => Flow::val(Value::Result(Err(Box::new(err_val)))),
                    _ => Flow::Abort("internal: result.filter pred returned non-Bool".into()),
                }
            }
            Some(Value::Result(Err(e))) => Flow::val(Value::Result(Err(e.clone()))),
            _ => Flow::Abort("internal: result.filter on non-Result".into()),
        }
    }

    fn hof_result_flat_map(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Result(Ok(v))) => {
                let clo = match Self::recv_closure(args, 1) {
                    Ok(c) => c,
                    Err(f) => return f,
                };
                self.apply_closure(&clo, vec![(**v).clone()])
            }
            Some(Value::Result(Err(e))) => {
                Flow::val(Value::Result(Err(e.clone())))
            }
            _ => Flow::Abort("internal: result.flat_map on non-Result".into()),
        }
    }

    // Ok(v) kept; Err(e) → f(e), the recovery closure's Result returned as-is
    // (runtime/rs result.rs or_else — flat_map's Err-side twin).
    fn hof_result_or_else(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Result(Ok(v))) => Flow::val(Value::Result(Ok(v.clone()))),
            Some(Value::Result(Err(e))) => {
                let clo = match Self::recv_closure(args, 1) {
                    Ok(c) => c,
                    Err(f) => return f,
                };
                self.apply_closure(&clo, vec![(**e).clone()])
            }
            _ => Flow::Abort("internal: result.or_else on non-Result".into()),
        }
    }

    fn hof_result_unwrap_or_else(&mut self, args: &[Value]) -> Flow {
        match args.first() {
            Some(Value::Result(Ok(v))) => Flow::val((**v).clone()),
            Some(Value::Result(Err(e))) => {
                let clo = match Self::recv_closure(args, 1) {
                    Ok(c) => c,
                    Err(f) => return f,
                };
                self.apply_closure(&clo, vec![(**e).clone()])
            }
            _ => Flow::Abort("internal: result.unwrap_or_else on non-Result".into()),
        }
    }

    // ── set HOFs ───────────────────────────────────────────────

    fn hof_set_map(&mut self, args: &[Value]) -> Flow {
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out: Vec<Value> = Vec::new();
        for item in items {
            let r = val!(self.apply_closure(&clo, vec![item]));
            if !out.contains(&r) {
                out.push(r);
            }
        }
        Flow::val(Value::Set(Rc::new(out)))
    }

    fn hof_set_filter(&mut self, args: &[Value]) -> Flow {
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out = Vec::new();
        for item in items {
            let keep = val!(self.apply_closure(&clo, vec![item.clone()]));
            if matches!(keep, Value::Bool(true)) {
                out.push(item);
            }
        }
        Flow::val(Value::Set(Rc::new(out)))
    }
}
