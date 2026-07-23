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

        // Per-module dispatch — same behavior-preserving regrouping as
        // `eval_container_op`: every arm was already keyed by a unique
        // `(m, f)` literal pair, so splitting on `m` first changes nothing
        // observable, and keeps each group under the per-function
        // complexity threshold instead of one 30-armed match.
        match m {
            "list" => self.eval_hof_list(f, &evaled),
            "option" => self.eval_hof_option(f, &evaled),
            "result" => self.eval_hof_result(f, &evaled),
            "set" => self.eval_hof_set(f, &evaled),
            _ => Flow::Unsupported(format!("HOF {}.{}", m, f)),
        }
    }

    fn eval_hof_list(&mut self, f: &str, evaled: &[Value]) -> Flow {
        match f {
            "map" => self.hof_map(evaled),
            "filter" => self.hof_filter(evaled, true),
            "find" => self.hof_find(evaled),
            "find_index" => self.hof_find_index(evaled),
            "any" => self.hof_any_all(evaled, true),
            "all" => self.hof_any_all(evaled, false),
            "count" => self.hof_count(evaled),
            "flat_map" => self.hof_flat_map(evaled),
            "filter_map" => self.hof_filter_map(evaled),
            "fold" => self.hof_fold(evaled),
            _ => self.eval_hof_list2(f, evaled),
        }
    }

    fn eval_hof_list2(&mut self, f: &str, evaled: &[Value]) -> Flow {
        match f {
            "reduce" => self.hof_reduce(evaled),
            "take_while" => self.hof_take_drop_while(evaled, true),
            "drop_while" => self.hof_take_drop_while(evaled, false),
            "partition" => self.hof_partition(evaled),
            "sort_by" => self.hof_sort_by(evaled),
            "each" => self.hof_each(evaled),
            "zip_with" => self.hof_zip_with(evaled),
            "unique_by" => self.hof_unique_by(evaled),
            "scan" => self.hof_scan(evaled),
            _ => Flow::Unsupported(format!("HOF list.{}", f)),
        }
    }

    fn eval_hof_option(&mut self, f: &str, evaled: &[Value]) -> Flow {
        match f {
            "map" => self.hof_option_map(evaled),
            "flat_map" => self.hof_option_flat_map(evaled),
            "filter" => self.hof_option_filter(evaled),
            "unwrap_or_else" => self.hof_option_unwrap_or_else(evaled),
            "or_else" => self.hof_option_or_else(evaled),
            _ => Flow::Unsupported(format!("HOF option.{}", f)),
        }
    }

    fn eval_hof_result(&mut self, f: &str, evaled: &[Value]) -> Flow {
        match f {
            "map" => self.hof_result_map(evaled, false),
            "map_err" => self.hof_result_map(evaled, true),
            "flat_map" => self.hof_result_flat_map(evaled),
            "unwrap_or_else" => self.hof_result_unwrap_or_else(evaled),
            "or_else" => self.hof_result_or_else(evaled),
            _ => Flow::Unsupported(format!("HOF result.{}", f)),
        }
    }

    // set HOFs operate on the ordered backing vec.
    fn eval_hof_set(&mut self, f: &str, evaled: &[Value]) -> Flow {
        match f {
            "map" => self.hof_set_map(evaled),
            "filter" => self.hof_set_filter(evaled),
            "any" => self.hof_any_all(evaled, true),
            "all" => self.hof_any_all(evaled, false),
            "fold" => self.hof_fold(evaled),
            _ => Flow::Unsupported(format!("HOF set.{}", f)),
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

    // unique_by(xs, key) — keep the FIRST element of each distinct key (the
    // native first-kept discipline; keys compared by Value equality).
    fn hof_unique_by(&mut self, args: &[Value]) -> Flow {
        let xs = match args.first().and_then(|v| v.as_iter_items()) {
            Some(i) => i,
            None => return Flow::Abort("internal: unique_by arg 0 not iterable".into()),
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut seen: Vec<Value> = Vec::new();
        let mut out = Vec::new();
        for x in xs {
            let k = val!(self.apply_closure(&clo, vec![x.clone()]));
            if !seen.iter().any(|s| s == &k) {
                seen.push(k);
                out.push(x);
            }
        }
        Flow::val(Value::list(out))
    }

    // scan(xs, init, f) — a fold that collects every running accumulator
    // (length n; the init itself is NOT emitted).
    fn hof_scan(&mut self, args: &[Value]) -> Flow {
        let xs = match args.first().and_then(|v| v.as_iter_items()) {
            Some(i) => i,
            None => return Flow::Abort("internal: scan arg 0 not iterable".into()),
        };
        let mut acc = match args.get(1) {
            Some(v) => v.clone(),
            None => return Flow::Abort("internal: scan missing init".into()),
        };
        let clo = match Self::recv_closure(args, 2) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out = Vec::new();
        for x in xs {
            acc = val!(self.apply_closure(&clo, vec![acc.clone(), x]));
            out.push(acc.clone());
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

include!("hofs_p2.rs");
