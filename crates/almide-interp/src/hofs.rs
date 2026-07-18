//! In-interp higher-order functions and interp-native container ops.
//!
//! The HOFs (map/filter/fold/…) MUST be interpreted natively: an interp
//! `Closure` cannot be coerced into the `Rc<dyn Fn>` a generic runtime HOF
//! demands. Each iterates the receiver and calls `apply_closure` per element.
//!
//! The container ops (non-HOF list/map/set/option/result transforms) are also
//! interp-native because they are generic — the runtime versions monomorphize
//! and cannot take a dynamic `Value`. They reproduce the same structural
//! transforms.

use std::rc::Rc;

use almide_base::intern::Sym;
use almide_ir::IrExpr;

use crate::env::Scope;
use crate::value::Value;
use crate::{Flow, Interpreter};

macro_rules! val {
    ($flow:expr) => {
        match $flow {
            Flow::Value(v) => v,
            other => return other,
        }
    };
}

impl<'a> Interpreter<'a> {
    /// Evaluate a higher-order `(module, func)` call. The closure argument is
    /// applied per element via `apply_closure`.
    pub(crate) fn eval_hof(
        &mut self,
        module: Sym,
        func: Sym,
        args: &[IrExpr],
        scope: &Scope,
    ) -> Flow {
        let m = module.as_str();
        let f = func.as_str();

        // Evaluate all args (the receiver + the closure[s]) eagerly.
        let mut evaled = Vec::with_capacity(args.len());
        for a in args {
            evaled.push(val!(self.eval_expr(a, scope)));
        }

        match (m, f) {
            // ── list HOFs ──
            ("list", "map") => self.hof_map(&evaled),
            ("list", "filter") => self.hof_filter(&evaled, true),
            ("list", "find") => self.hof_find(&evaled),
            ("list", "find_index") => self.hof_find_index(&evaled),
            ("list", "any") => self.hof_any_all(&evaled, true),
            ("list", "all") => self.hof_any_all(&evaled, false),
            ("list", "count") => self.hof_count(&evaled),
            ("list", "flat_map") => self.hof_flat_map(&evaled),
            ("list", "filter_map") => self.hof_filter_map(&evaled),
            ("list", "fold") => self.hof_fold(&evaled),
            ("list", "reduce") => self.hof_reduce(&evaled),
            ("list", "take_while") => self.hof_take_drop_while(&evaled, true),
            ("list", "drop_while") => self.hof_take_drop_while(&evaled, false),
            ("list", "partition") => self.hof_partition(&evaled),
            ("list", "sort_by") => self.hof_sort_by(&evaled),
            ("list", "each") => self.hof_each(&evaled),
            ("list", "zip_with") => self.hof_zip_with(&evaled),

            // ── option HOFs ──
            ("option", "map") => self.hof_option_map(&evaled),
            ("option", "flat_map") => self.hof_option_flat_map(&evaled),
            ("option", "filter") => self.hof_option_filter(&evaled),
            ("option", "unwrap_or_else") => self.hof_option_unwrap_or_else(&evaled),
            ("option", "or_else") => self.hof_option_or_else(&evaled),

            // ── result HOFs ──
            ("result", "map") => self.hof_result_map(&evaled, false),
            ("result", "map_err") => self.hof_result_map(&evaled, true),
            ("result", "flat_map") => self.hof_result_flat_map(&evaled),
            ("result", "unwrap_or_else") => self.hof_result_unwrap_or_else(&evaled),

            // ── set HOFs (operate on the ordered backing vec) ──
            ("set", "map") => self.hof_set_map(&evaled),
            ("set", "filter") => self.hof_set_filter(&evaled),
            ("set", "any") => self.hof_any_all(&evaled, true),
            ("set", "all") => self.hof_any_all(&evaled, false),
            ("set", "fold") => self.hof_fold(&evaled),

            _ => Flow::Unsupported(format!("HOF {}.{}", m, f)),
        }
    }

    // ── list HOFs ──────────────────────────────────────────────

    fn recv_items(args: &[Value]) -> Result<Vec<Value>, Flow> {
        match args.first().and_then(|v| v.as_iter_items()) {
            Some(items) => Ok(items),
            None => Err(Flow::Abort("internal: HOF receiver not iterable".into())),
        }
    }

    fn recv_closure(args: &[Value], idx: usize) -> Result<Rc<crate::Closure>, Flow> {
        match args.get(idx) {
            Some(Value::Closure(c)) => Ok(c.clone()),
            _ => Err(Flow::Abort(format!(
                "internal: HOF arg {} not a closure",
                idx
            ))),
        }
    }

    fn hof_map(&mut self, args: &[Value]) -> Flow {
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(val!(self.apply_closure(&clo, vec![item])));
        }
        Flow::val(Value::list(out))
    }

    fn hof_filter(&mut self, args: &[Value], keep_true: bool) -> Flow {
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
            if matches!(keep, Value::Bool(b) if b == keep_true) {
                out.push(item);
            }
        }
        Flow::val(Value::list(out))
    }

    fn hof_find(&mut self, args: &[Value]) -> Flow {
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        for item in items {
            let hit = val!(self.apply_closure(&clo, vec![item.clone()]));
            if matches!(hit, Value::Bool(true)) {
                return Flow::val(Value::Option(Some(Box::new(item))));
            }
        }
        Flow::val(Value::Option(None))
    }

    fn hof_find_index(&mut self, args: &[Value]) -> Flow {
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        for (i, item) in items.into_iter().enumerate() {
            let hit = val!(self.apply_closure(&clo, vec![item]));
            if matches!(hit, Value::Bool(true)) {
                return Flow::val(Value::Option(Some(Box::new(Value::Int(i as i64)))));
            }
        }
        Flow::val(Value::Option(None))
    }

    fn hof_any_all(&mut self, args: &[Value], is_any: bool) -> Flow {
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        for item in items {
            let r = val!(self.apply_closure(&clo, vec![item]));
            let b = matches!(r, Value::Bool(true));
            if is_any && b {
                return Flow::val(Value::Bool(true));
            }
            if !is_any && !b {
                return Flow::val(Value::Bool(false));
            }
        }
        Flow::val(Value::Bool(!is_any))
    }

    fn hof_count(&mut self, args: &[Value]) -> Flow {
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut n = 0i64;
        for item in items {
            let r = val!(self.apply_closure(&clo, vec![item]));
            if matches!(r, Value::Bool(true)) {
                n += 1;
            }
        }
        Flow::val(Value::Int(n))
    }

    fn hof_flat_map(&mut self, args: &[Value]) -> Flow {
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
            let r = val!(self.apply_closure(&clo, vec![item]));
            match r.as_iter_items() {
                Some(sub) => out.extend(sub),
                None => {
                    return Flow::Abort("internal: flat_map closure did not return a list".into())
                }
            }
        }
        Flow::val(Value::list(out))
    }

    fn hof_filter_map(&mut self, args: &[Value]) -> Flow {
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
            let r = val!(self.apply_closure(&clo, vec![item]));
            if let Value::Option(Some(v)) = r {
                out.push(*v);
            }
        }
        Flow::val(Value::list(out))
    }

    fn hof_fold(&mut self, args: &[Value]) -> Flow {
        // fold(receiver, init, (acc, x) => ...)
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let mut acc = match args.get(1) {
            Some(v) => v.clone(),
            None => return Flow::Abort("internal: fold missing init".into()),
        };
        let clo = match Self::recv_closure(args, 2) {
            Ok(c) => c,
            Err(f) => return f,
        };
        for item in items {
            acc = val!(self.apply_closure(&clo, vec![acc, item]));
        }
        Flow::val(acc)
    }

    fn hof_reduce(&mut self, args: &[Value]) -> Flow {
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut iter = items.into_iter();
        let mut acc = match iter.next() {
            Some(v) => v,
            None => return Flow::val(Value::Option(None)),
        };
        for item in iter {
            acc = val!(self.apply_closure(&clo, vec![acc, item]));
        }
        Flow::val(Value::Option(Some(Box::new(acc))))
    }

    fn hof_take_drop_while(&mut self, args: &[Value], take: bool) -> Flow {
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut split = items.len();
        for (i, item) in items.iter().enumerate() {
            let r = val!(self.apply_closure(&clo, vec![item.clone()]));
            if !matches!(r, Value::Bool(true)) {
                split = i;
                break;
            }
        }
        let out: Vec<Value> = if take {
            items[..split].to_vec()
        } else {
            items[split..].to_vec()
        };
        Flow::val(Value::list(out))
    }

    fn hof_partition(&mut self, args: &[Value]) -> Flow {
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let (mut yes, mut no) = (Vec::new(), Vec::new());
        for item in items {
            let r = val!(self.apply_closure(&clo, vec![item.clone()]));
            if matches!(r, Value::Bool(true)) {
                yes.push(item);
            } else {
                no.push(item);
            }
        }
        Flow::val(Value::tuple(vec![Value::list(yes), Value::list(no)]))
    }

    fn hof_sort_by(&mut self, args: &[Value]) -> Flow {
        // `sort_by[A, B](xs: List[A], f: (A) -> B) -> List[A]` is KEY-EXTRACTION,
        // NOT a comparator. The native runtime is the oracle:
        //   runtime/rs/src/list.rs:
        //     xs.sort_by_key(|x| f(x.clone()))   (B: Ord)
        // i.e. apply the 1-arg closure to each element to get its sort key, then
        // STABLY sort the elements by their keys' natural `Ord`. The WASM backend
        // (emit_wasm/calls_list_closure.rs `sort_by`) reproduces this with a
        // stable bubble sort that swaps only on key[j] > key[j+1] (strict — equal
        // keys never swap → stable). Both verified byte-identical on Int / String
        // keys, ties preserved in input order (probe /tmp/sorti.almd).
        //
        // Key ordering: native `B: Ord` means the only key types a *compilable*
        // program can use are Ord types — Int, String, Bool (and Ord-composites).
        // A Float key is a hard compile error in BOTH backends (`f64: !Ord`,
        // verified: the type checker rejects `sort_by(xs, (x: Float) => x)` with
        // an `Ord` bound failure), so a Float-key sort_by never reaches a runnable
        // program and therefore never reaches this interpreter on the 3-way
        // oracle. `Value::partial_cmp_val` still orders floats sensibly (and falls
        // back to "Equal → no swap", preserving stability) so a defensive path
        // never panics, but it casts no third vote a backend could disagree with.
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        // Compute each element's key once (a single application per element,
        // matching native's `sort_by_key`, which does NOT re-call the key fn on
        // every comparison).
        let mut keyed: Vec<(Value, Value)> = Vec::with_capacity(items.len());
        for item in items {
            let key = val!(self.apply_closure(&clo, vec![item.clone()]));
            keyed.push((key, item));
        }
        // Stable sort by key. `slice::sort_by` is stable (mirrors native
        // `sort_by_key`); compare keys with the same TOTAL order the backends
        // use (`total_cmp_val`: Int/Bool/String → natural `cmp`, Float →
        // IEEE-754 totalOrder so a Float key now sorts byte-identically to the
        // native `_float` variant and the wasm bit-trick — C-055). Genuinely
        // incomparable keys collapse to `Equal` (stable → input order).
        keyed.sort_by(|(ka, _), (kb, _)| {
            ka.total_cmp_val(kb).unwrap_or(std::cmp::Ordering::Equal)
        });
        Flow::val(Value::list(keyed.into_iter().map(|(_, item)| item).collect()))
    }

    fn hof_each(&mut self, args: &[Value]) -> Flow {
        let items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        for item in items {
            val!(self.apply_closure(&clo, vec![item]));
        }
        Flow::val(Value::Unit)
    }

    fn hof_zip_with(&mut self, args: &[Value]) -> Flow {
        let a = match args.first().and_then(|v| v.as_iter_items()) {
            Some(i) => i,
            None => return Flow::Abort("internal: zip_with arg 0 not iterable".into()),
        };
        let b = match args.get(1).and_then(|v| v.as_iter_items()) {
            Some(i) => i,
            None => return Flow::Abort("internal: zip_with arg 1 not iterable".into()),
        };
        let clo = match Self::recv_closure(args, 2) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out = Vec::new();
        for (x, y) in a.into_iter().zip(b.into_iter()) {
            out.push(val!(self.apply_closure(&clo, vec![x, y])));
        }
        Flow::val(Value::list(out))
    }

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
        match (module, func) {
            // ── list ──
            ("list", "len") | ("list", "length") => Some(match args.first() {
                Some(v) => match v.as_iter_items() {
                    Some(items) => Flow::val(Value::Int(items.len() as i64)),
                    None => Flow::Abort("internal: list.len on non-list".into()),
                },
                None => Flow::Abort("internal: list.len no arg".into()),
            }),
            ("list", "is_empty") => Some(match args.first().and_then(|v| v.as_iter_items()) {
                Some(items) => Flow::val(Value::Bool(items.is_empty())),
                None => Flow::Abort("internal: list.is_empty on non-list".into()),
            }),
            ("list", "reverse") => Some(match args.first().and_then(|v| v.as_iter_items()) {
                Some(mut items) => {
                    items.reverse();
                    Flow::val(Value::list(items))
                }
                None => Flow::Abort("internal: list.reverse on non-list".into()),
            }),
            ("list", "first") | ("list", "head") => Some(
                match args.first().and_then(|v| v.as_iter_items()) {
                    Some(items) => Flow::val(Value::Option(
                        items.first().cloned().map(Box::new),
                    )),
                    None => Flow::Abort("internal: list.first on non-list".into()),
                },
            ),
            ("list", "last") => Some(match args.first().and_then(|v| v.as_iter_items()) {
                Some(items) => Flow::val(Value::Option(items.last().cloned().map(Box::new))),
                None => Flow::Abort("internal: list.last on non-list".into()),
            }),
            ("list", "get") => Some(self.list_get(args)),
            ("list", "contains") => Some(match (args.first().and_then(|v| v.as_iter_items()), args.get(1)) {
                (Some(items), Some(x)) => Flow::val(Value::Bool(items.contains(x))),
                _ => Flow::Abort("internal: list.contains bad args".into()),
            }),
            // `list.append` is the FUNCTIONAL append (returns a new list); the
            // mutating `list.push` is intercepted by the in-place-mutation guard
            // above and never reaches here.
            ("list", "append") => Some(match (args.first().and_then(|v| v.as_iter_items()), args.get(1)) {
                (Some(mut items), Some(x)) => {
                    items.push(x.clone());
                    Flow::val(Value::list(items))
                }
                _ => Flow::Abort("internal: list.append bad args".into()),
            }),
            ("list", "concat") => Some(match (args.first().and_then(|v| v.as_iter_items()), args.get(1).and_then(|v| v.as_iter_items())) {
                (Some(mut a), Some(b)) => {
                    a.extend(b);
                    Flow::val(Value::list(a))
                }
                _ => Flow::Abort("internal: list.concat bad args".into()),
            }),
            ("list", "sum") => Some(self.list_sum(args)),
            ("list", "product") => Some(self.list_product(args)),
            ("list", "min") => Some(self.list_min_max(args, false)),
            ("list", "max") => Some(self.list_min_max(args, true)),
            ("list", "join") => Some(self.list_join(args)),
            ("list", "sort") => Some(self.list_sort(args)),
            ("list", "enumerate") => Some(match args.first().and_then(|v| v.as_iter_items()) {
                Some(items) => Flow::val(Value::list(
                    items
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| Value::tuple(vec![Value::Int(i as i64), v]))
                        .collect(),
                )),
                None => Flow::Abort("internal: list.enumerate on non-list".into()),
            }),

            // ── map ──
            ("map", "len") | ("map", "size") => Some(match args.first() {
                Some(Value::Map(e)) => Flow::val(Value::Int(e.len() as i64)),
                _ => Flow::Abort("internal: map.len on non-map".into()),
            }),
            ("map", "get") => Some(match (args.first(), args.get(1)) {
                (Some(Value::Map(e)), Some(k)) => Flow::val(Value::Option(
                    e.iter().find(|(ek, _)| ek == k).map(|(_, v)| Box::new(v.clone())),
                )),
                _ => Flow::Abort("internal: map.get bad args".into()),
            }),
            ("map", "contains_key") | ("map", "has") => Some(match (args.first(), args.get(1)) {
                (Some(Value::Map(e)), Some(k)) => {
                    Flow::val(Value::Bool(e.iter().any(|(ek, _)| ek == k)))
                }
                _ => Flow::Abort("internal: map.contains_key bad args".into()),
            }),
            // `map.set` is FUNCTIONAL (`-> Map`, returns a new map); the
            // mutating `map.insert` (`mut m, .. -> Unit`) is intercepted by the
            // in-place-mutation guard above.
            ("map", "set") => Some(match (args.first(), args.get(1), args.get(2)) {
                (Some(Value::Map(e)), Some(k), Some(v)) => {
                    let mut new = (**e).clone();
                    crate::eval::map_insert(&mut new, k.clone(), v.clone());
                    Flow::val(Value::Map(Rc::new(new)))
                }
                _ => Flow::Abort("internal: map.set bad args".into()),
            }),
            ("map", "keys") => Some(match args.first() {
                Some(Value::Map(e)) => {
                    Flow::val(Value::list(e.iter().map(|(k, _)| k.clone()).collect()))
                }
                _ => Flow::Abort("internal: map.keys on non-map".into()),
            }),
            ("map", "values") => Some(match args.first() {
                Some(Value::Map(e)) => {
                    Flow::val(Value::list(e.iter().map(|(_, v)| v.clone()).collect()))
                }
                _ => Flow::Abort("internal: map.values on non-map".into()),
            }),

            // ── set ──
            ("set", "len") | ("set", "size") => Some(match args.first() {
                Some(Value::Set(e)) => Flow::val(Value::Int(e.len() as i64)),
                _ => Flow::Abort("internal: set.len on non-set".into()),
            }),
            ("set", "contains") | ("set", "has") => Some(match (args.first(), args.get(1)) {
                (Some(Value::Set(e)), Some(x)) => Flow::val(Value::Bool(e.contains(x))),
                _ => Flow::Abort("internal: set.contains bad args".into()),
            }),
            ("set", "insert") | ("set", "add") => Some(match (args.first(), args.get(1)) {
                (Some(Value::Set(e)), Some(x)) => {
                    let mut new = (**e).clone();
                    if !new.contains(x) {
                        new.push(x.clone());
                    }
                    Flow::val(Value::Set(Rc::new(new)))
                }
                _ => Flow::Abort("internal: set.insert bad args".into()),
            }),
            ("set", "to_list") => Some(match args.first() {
                Some(Value::Set(e)) => Flow::val(Value::list((**e).clone())),
                _ => Flow::Abort("internal: set.to_list on non-set".into()),
            }),

            // ── option ──
            ("option", "is_some") => Some(match args.first() {
                Some(Value::Option(o)) => Flow::val(Value::Bool(o.is_some())),
                _ => Flow::Abort("internal: option.is_some on non-option".into()),
            }),
            ("option", "is_none") => Some(match args.first() {
                Some(Value::Option(o)) => Flow::val(Value::Bool(o.is_none())),
                _ => Flow::Abort("internal: option.is_none on non-option".into()),
            }),
            ("option", "unwrap_or") => Some(match (args.first(), args.get(1)) {
                (Some(Value::Option(Some(v))), _) => Flow::val((**v).clone()),
                (Some(Value::Option(None)), Some(d)) => Flow::val(d.clone()),
                _ => Flow::Abort("internal: option.unwrap_or bad args".into()),
            }),

            // ── result ──
            ("result", "is_ok") => Some(match args.first() {
                Some(Value::Result(r)) => Flow::val(Value::Bool(r.is_ok())),
                _ => Flow::Abort("internal: result.is_ok on non-result".into()),
            }),
            ("result", "is_err") => Some(match args.first() {
                Some(Value::Result(r)) => Flow::val(Value::Bool(r.is_err())),
                _ => Flow::Abort("internal: result.is_err on non-result".into()),
            }),
            ("result", "unwrap_or") => Some(match (args.first(), args.get(1)) {
                (Some(Value::Result(Ok(v))), _) => Flow::val((**v).clone()),
                (Some(Value::Result(Err(_))), Some(d)) => Flow::val(d.clone()),
                _ => Flow::Abort("internal: result.unwrap_or bad args".into()),
            }),
            // ok(v) → some(v), err(_) → none (runtime/rs result.rs to_option)
            ("result", "to_option") => Some(match args.first() {
                Some(Value::Result(Ok(v))) => Flow::val(Value::Option(Some(v.clone()))),
                Some(Value::Result(Err(_))) => Flow::val(Value::Option(None)),
                _ => Flow::Abort("internal: result.to_option on non-result".into()),
            }),
            // ok(_) → none, err(e) → some(e) (runtime/rs result.rs to_err_option)
            ("result", "to_err_option") => Some(match args.first() {
                Some(Value::Result(Ok(_))) => Flow::val(Value::Option(None)),
                Some(Value::Result(Err(e))) => Flow::val(Value::Option(Some(e.clone()))),
                _ => Flow::Abort("internal: result.to_err_option on non-result".into()),
            }),

            _ => None,
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

