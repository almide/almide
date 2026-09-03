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
            "map" => self.eval_hof_map_mod(f, &evaled),
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
            "update" => self.hof_list_update(evaled),
            "group_by" => self.hof_group_by(evaled),
            _ => self.eval_hof_list_try(f, evaled),
        }
    }

    /// `list.group_by(xs, (x) -> K)` — a Map of first-occurrence-ordered keys
    /// to the sub-lists in source order (runtime/rs/src/list.rs's
    /// insert-or-append walk, verbatim).
    fn hof_group_by(&mut self, args: &[Value]) -> Flow {
        let xs = match args.first() {
            Some(Value::List(e)) => e.clone(),
            _ => return Flow::Abort("internal: list.group_by receiver not a List".into()),
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out: Vec<(Value, Value)> = Vec::new();
        for x in xs.iter() {
            let k = val!(self.apply_closure(&clo, vec![x.clone()]));
            match out.iter_mut().find(|(ek, _)| *ek == k) {
                Some((_, Value::List(group))) => {
                    let mut g = (**group).clone();
                    g.push(x.clone());
                    *group = std::rc::Rc::new(g);
                }
                Some(_) => unreachable!("group values are lists by construction"),
                None => out.push((k, Value::list(vec![x.clone()]))),
            }
        }
        Flow::val(Value::Map(std::rc::Rc::new(out)))
    }

    /// The `__fallible_*` carriers (ADR-0006): the fallibility-polymorphic form of
    /// the HOFs above, instantiated when a callback propagates with `!`. Each
    /// is its plain sibling plus FIRST-ERR SHORT-CIRCUIT — the callback yields
    /// `Result[_, E]`, the first `err` becomes the whole call's `err` and stops
    /// the traversal, and a full pass wraps its result in `ok`.
    ///
    /// These are almide-bodied (`stdlib/list.almd`) but cannot reach that body
    /// here for the same reason the plain HOFs cannot: an interp `Closure` is
    /// not the `Rc<dyn Fn>` the generic path wants. Without these arms the
    /// whole family abstained, which silently removed the third oracle from
    /// the idiom ADR-0006 makes the DEFAULT way to write a fallible traversal.
    fn eval_hof_list_try(&mut self, f: &str, evaled: &[Value]) -> Flow {
        match f {
            "__fallible_map" => self.hof_try_map(evaled),
            "__fallible_filter" => self.hof_try_filter(evaled),
            "__fallible_filter_map" => self.hof_try_filter_map(evaled),
            "__fallible_flat_map" => self.hof_try_flat_map(evaled),
            "__fallible_find" => self.hof_try_find(evaled),
            "__fallible_fold" => self.hof_try_fold(evaled),
            "__fallible_each" => self.hof_try_each(evaled),
            _ => Flow::Unsupported(format!("HOF list.{}", f)),
        }
    }

    /// The Map-MODULE HOFs (Stage 2 BRIDGEABLE burn-down). The receiver is a
    /// `Value::Map` of insertion-ordered `(k, v)` entries — the AlmideMap
    /// determinism contract — so a sequential walk over the backing vec IS
    /// the spec order. Remaining map HOFs on the `is_hof` allowlist keep the
    /// explicit Unsupported abstain until a fixture exercises them.
    fn eval_hof_map_mod(&mut self, f: &str, evaled: &[Value]) -> Flow {
        match f {
            "fold" => self.hof_map_fold(evaled),
            "map" => self.hof_map_map(evaled),
            "filter" => self.hof_map_filter(evaled),
            "find" => self.hof_map_find(evaled),
            "update" => self.hof_map_update(evaled),
            "upsert" => self.hof_map_upsert(evaled),
            _ => Flow::Unsupported(format!("HOF map.{}", f)),
        }
    }

    /// `map.upsert(m, key, init, (v) -> V)` — a PRESENT key's value rewritten
    /// via `f` in place (position kept), an ABSENT key appended as `(key,
    /// init)` — v0's `almide_rt_map_upsert` intrinsic behavior, and the
    /// `map_upsert_str` self-host (contains → update, else set).
    fn hof_map_upsert(&mut self, args: &[Value]) -> Flow {
        let entries = match args.first() {
            Some(Value::Map(e)) => e.clone(),
            _ => return Flow::Abort("internal: map.upsert receiver not a Map".into()),
        };
        let Some(key) = args.get(1).cloned() else {
            return Flow::Abort("internal: map.upsert missing key".into());
        };
        let Some(init) = args.get(2).cloned() else {
            return Flow::Abort("internal: map.upsert missing init".into());
        };
        let clo = match Self::recv_closure(args, 3) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out = (*entries).clone();
        let mut found = false;
        for pair in out.iter_mut() {
            if pair.0 == key {
                let nv = val!(self.apply_closure(&clo, vec![pair.1.clone()]));
                pair.1 = nv;
                found = true;
                break;
            }
        }
        if !found {
            out.push((key, init));
        }
        Flow::val(Value::Map(std::rc::Rc::new(out)))
    }

    /// `map.update(m, key, (v) -> V)` — the value at `key` rewritten in
    /// place (position kept), a missing key returns the map unchanged —
    /// map_core's `__map_find_soft` + `__map_update_at` behavior.
    fn hof_map_update(&mut self, args: &[Value]) -> Flow {
        let entries = match args.first() {
            Some(Value::Map(e)) => e.clone(),
            _ => return Flow::Abort("internal: map.update receiver not a Map".into()),
        };
        let Some(key) = args.get(1).cloned() else {
            return Flow::Abort("internal: map.update missing key".into());
        };
        let clo = match Self::recv_closure(args, 2) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out = (*entries).clone();
        for pair in out.iter_mut() {
            if pair.0 == key {
                let nv = val!(self.apply_closure(&clo, vec![pair.1.clone()]));
                pair.1 = nv;
                break;
            }
        }
        Flow::val(Value::Map(std::rc::Rc::new(out)))
    }

    /// `map.map(m, (v) -> B)` — VALUES rewritten, keys and entry order kept
    /// (`stdlib/map.almd`'s 1-ary callback signature).
    fn hof_map_map(&mut self, args: &[Value]) -> Flow {
        let entries = match args.first() {
            Some(Value::Map(e)) => e.clone(),
            _ => return Flow::Abort("internal: map.map receiver not a Map".into()),
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out = Vec::with_capacity(entries.len());
        for (k, v) in entries.iter() {
            let nv = val!(self.apply_closure(&clo, vec![v.clone()]));
            out.push((k.clone(), nv));
        }
        Flow::val(Value::Map(std::rc::Rc::new(out)))
    }

    /// `map.filter(m, (k, v) -> Bool)` — entries kept in order where the
    /// 2-ary callback answers true.
    fn hof_map_filter(&mut self, args: &[Value]) -> Flow {
        let entries = match args.first() {
            Some(Value::Map(e)) => e.clone(),
            _ => return Flow::Abort("internal: map.filter receiver not a Map".into()),
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let mut out = Vec::new();
        for (k, v) in entries.iter() {
            let keep = val!(self.apply_closure(&clo, vec![k.clone(), v.clone()]));
            if matches!(keep, Value::Bool(true)) {
                out.push((k.clone(), v.clone()));
            }
        }
        Flow::val(Value::Map(std::rc::Rc::new(out)))
    }

    /// `map.find(m, (k, v) -> Bool)` — first entry (insertion order) the
    /// 2-ary callback accepts, as `some((k, v))`; a full pass answers `none`.
    fn hof_map_find(&mut self, args: &[Value]) -> Flow {
        let entries = match args.first() {
            Some(Value::Map(e)) => e.clone(),
            _ => return Flow::Abort("internal: map.find receiver not a Map".into()),
        };
        let clo = match Self::recv_closure(args, 1) {
            Ok(c) => c,
            Err(f) => return f,
        };
        for (k, v) in entries.iter() {
            let hit = val!(self.apply_closure(&clo, vec![k.clone(), v.clone()]));
            if matches!(hit, Value::Bool(true)) {
                return Flow::val(Value::Option(Some(Box::new(Value::tuple(vec![
                    k.clone(),
                    v.clone(),
                ])))));
            }
        }
        Flow::val(Value::Option(None))
    }

    /// `map.fold(m, init, (acc, k, v) -> acc)` — the 3-ary callback form (the
    /// list fold's callback is 2-ary, so it cannot be reused).
    fn hof_map_fold(&mut self, args: &[Value]) -> Flow {
        let entries = match args.first() {
            Some(Value::Map(e)) => e.clone(),
            _ => return Flow::Abort("internal: map.fold receiver not a Map".into()),
        };
        let mut acc = match args.get(1) {
            Some(v) => v.clone(),
            None => return Flow::Abort("internal: map.fold missing init".into()),
        };
        let clo = match Self::recv_closure(args, 2) {
            Ok(c) => c,
            Err(f) => return f,
        };
        for (k, v) in entries.iter() {
            acc = val!(self.apply_closure(&clo, vec![acc, k.clone(), v.clone()]));
        }
        Flow::val(acc)
    }

    /// `list.update(xs, i, f)` — a copy with element `i` replaced by `f(xs[i])`;
    /// an out-of-range index returns the list unchanged (the stdlib's total
    /// contract — no abort channel in the signature).
    fn hof_list_update(&mut self, args: &[Value]) -> Flow {
        let mut items = match Self::recv_items(args) {
            Ok(i) => i,
            Err(f) => return f,
        };
        let idx = match args.get(1) {
            Some(Value::Int(i)) => *i,
            _ => return Flow::Abort("internal: list.update index not an Int".into()),
        };
        let clo = match Self::recv_closure(args, 2) {
            Ok(c) => c,
            Err(f) => return f,
        };
        if idx >= 0 && (idx as usize) < items.len() {
            let i = idx as usize;
            let updated = val!(self.apply_closure(&clo, vec![items[i].clone()]));
            items[i] = updated;
        }
        Flow::val(Value::list(items))
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
            "filter" => self.hof_result_filter(evaled),
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
}

include!("hofs_list.rs");
include!("hofs_carrier.rs");
include!("hofs_list_ops.rs");
include!("hofs_map_set_ops.rs");
