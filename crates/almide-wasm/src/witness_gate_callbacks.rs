//! The witness gate's inlined-callback rules (#2755 / #2758), split from
//! witness_gate.rs for the file budget.

use almide_ir::{IrExpr, IrExprKind};

use super::{value_subset, Why};

/// #2758: the fallible collection HOFs (`list.__fallible_map__…`,
/// `map.` / `set.` / `option.__fallible_*`, the checker's instantiation of a
/// callback that raises — stdlib/{list,map,set,option}.almd) are
/// SELF-HOSTED: an ordinary call
/// to a lifted stdlib body, no native arm inlines the lambda. The literal
/// callback is then a closure VALUE — its env is built by the closure hooks
/// and handed over under the callee's convention like any fresh argument.
///
/// A MONO INSTANCE surface name (`result.filter`'s instance, the checker's
/// instantiation reaching the registry under its instance name) is
/// the same: no native arm matches it, so it lowers as the linked call.
/// Were an arm to inline it after all, the callback node would carry no
/// hook and the module-call audit would decline the frame.
///
/// Two surfaces take a Fn VALUE outright: `bytes.map_each` has no native arm
/// (the linked self-host body calls the closure), and `list.push` stores the
/// closure it is handed as an element (`lower_arg`, Retain).
pub(super) fn is_self_hosted_hof(module: &str, func: &str) -> bool {
    let base = almide_ir::mono_base_or_self(func);
    (matches!(module, "list" | "map" | "set" | "option") && base.starts_with("__fallible_"))
        || (base != func && !base.starts_with("__"))
        || matches!((module, func), ("bytes", "map_each") | ("list", "push"))
}

/// #2755 / #2758: a module call that INLINES a literal callback. Admitted
/// for the arms whose lowering (list.rs) records the callback as a loop
/// activation per element (`witness_callback_open` / `witness_loop_close`):
/// each param is a VIEW of the element it is loaded from, the body's own
/// sites are the ordinary hooks, and what the arm does with the body's value
/// is hooked or carries no RC site:
///
/// - `list.map`: the value is stored into the fresh result spine after the
///   share guard (`witness_store`);
/// - `list.filter`, `any`, `all`, `count`: the value is a Bool;
/// - `list.find`: the value is a Bool, and a hit shares the element into a
///   fresh some-cell (`witness_find_hit`, `am`);
/// - `list.fold`: a scalar accumulator carries no credit; a HEAP one is a
///   loop-carried OWNER (`witness_fold_step`) — the init's credit moves into
///   the loop (the Retain argument, `am` / `im`), each iteration receives the
///   accumulator (`i`), hands the body's value on (`im` / `am`) and releases
///   what it received (`d`), and the fold's result is the owned value its
///   consumer records. A fold over a `list.*` chain may take the fused or
///   enumerate lowering (list_fuse.rs, list_enumerate_fold.rs): one
///   activation per element over every inlined stage (#2755);
/// - #2755: the other list arms (`sort_by`'s keys, `flat_map`, `filter_map`,
///   `take_while`, `drop_while`, `unique_by`'s and `group_by`'s keys, `update`, `reduce`,
///   `scan`, `zip_with`), `matrix.map`, and the map / set arms (`fold`,
///   `find`, `filter`, the predicates, `map`, `update`, `upsert`), each
///   settling the body's value at the instruction that takes it
///   (witness_inline.rs);
/// - #3137: the fs line walkers and the prefetch fans (witness_walkers.rs);
/// - the option / result combinators (sums.rs): the callback runs at most
///   once, on one arm of a branch site (witness_inline.rs); its value is
///   settled by the share guard (`witness_store`), a payload handed out on
///   the other arm takes its share (`witness_payload_share`).
///
/// A body that still PROPAGATES a `!` is not inlined at all (the fn-value
/// route, list.rs), so it declines as `call-arg:Lambda:<arm>:propagating` —
/// except an fs walker's, whose closure route is recorded.
/// Any other arm declines as `call-arg:Lambda:<module>.<fn>`.
pub(super) fn inline_callback_subset(module: &str, func: &str, args: &[IrExpr]) -> Option<Why> {
    let here = |t: &str| Some(Why::Here(format!("Lambda:{module}.{func}{t}")).inside("call-arg"));
    let arity = match (module, func, args) {
        ("list", "map" | "filter" | "find" | "any" | "all" | "count", [_, _]) => 1,
        ("list", "sort_by" | "flat_map" | "filter_map" | "take_while" | "drop_while" | "unique_by" | "group_by", [_, _]) | ("list", "update", [_, _, _]) => 1,
        ("list", "reduce", [_, _]) | ("list", "scan" | "zip_with", [_, _, _]) => 2,
        ("matrix", "map", [_, _]) => 1,
        ("set", "filter" | "map", [_, _]) | ("map", "map", [_, _]) | ("map", "update", [_, _, _]) => 1,
        ("map", "upsert", [_, _, _, _]) => 1,
        ("map", "find" | "filter" | "all" | "any" | "count", [_, _]) => 2,
        ("set", "fold", [_, _, _]) => 2,
        ("map", "fold", [_, _, _]) => 3,
        // The option / result combinators run the callback at most once, on
        // one arm of a branch site (witness_inline.rs).
        ("result", "unwrap_or_else" | "map" | "map_err" | "flat_map", [_, _])
        | ("option", "map" | "flat_map" | "filter", [_, _]) => 1,
        ("option", "unwrap_or_else" | "or_else", [_, _]) => 0,
        // The prefetch forms (fan.rs) never lower the body: each awaited
        // read's Result carrier is born in the frame and settled as the
        // sequential fan's (witness_walkers.rs).
        ("fan", "map" | "any" | "any_map", [_, _]) => 1,
        // #3137: the fs line walkers (fs.rs, fs_meta.rs, fs_range.rs): one
        // activation per line, the line a block the walk allocates and
        // releases around it, a heap accumulator carried as `list.fold`'s
        // (witness_walkers.rs). A fallible walker's compound body is called
        // as a closure (`:propagating` below).
        ("fs", "fold_lines", [_, _, _]) | ("fs", "fold_lines_chunked", [_, _, _, _]) => 2,
        ("fs", "fold_lines_range", [_, _, _, _, _]) => 2,
        ("fs", "for_each_line", [_, _]) => 1,
        ("fs", f, [_, _, _]) if almide_ir::mono_base_or_self(f).starts_with("__fallible_fold_lines") => 2,
        ("fs", f, [_, _]) if almide_ir::mono_base_or_self(f).starts_with("__fallible_for_each_line") => 1,
        // A fold over a `list.*` chain may take the fused or enumerate
        // lowering (list_fuse.rs, list_enumerate_fold.rs): one activation per
        // element over every inlined stage, whose callbacks are the chain's
        // own `list.map` / `list.filter` arguments, judged below as such.
        ("list", "fold", [_, _, _]) => 2,
        _ => return here(""),
    };
    let (cb, rest) = args.split_last()?;
    let IrExprKind::Lambda { params, body, .. } = &cb.kind else {
        return here(":wrapped");
    };
    if params.len() != arity {
        return here(":arity");
    }
    // fan.rs lowers the body with its top-level `!` stripped (the
    // accumulator performs the first-err semantics itself), and only a body
    // that still propagates after the strip takes the closure route.
    let body = if module == "fan" { crate::fan::strip_callback_try(body) } else { body };
    if crate::fs_meta::expr_propagates(body) {
        // #2755: an fs walker calls a compound callback as the closure value
        // it is (fs_meta.rs / fs_fallible.rs closure routes): the env is an
        // ordinary closure argument, each line an activation that shares the
        // line (and a heap accumulator) into the callee (witness_walkers.rs).
        if module == "fs" {
            return args.iter().find_map(|a| value_subset(a).map(|w| w.inside("call-arg")));
        }
        return here(":propagating");
    }
    rest.iter()
        .find_map(|a| value_subset(a).map(|w| w.inside("call-arg")))
        .or_else(|| value_subset(body).map(|w| w.inside("callback")))
}
