//! The witness gate's inlined-callback rules (#2755 / #2758), split from
//! witness_gate.rs for the file budget.

use almide_ir::{IrExpr, IrExprKind};

use super::{value_subset, Why};

// #2758: a SELF-HOSTED collection HOF (`list.__fallible_map__…`, a mono
// instance surface name, `bytes.map`, `list.push`'s closure element) takes a
// literal callback as a closure VALUE — its env built by the closure hooks
// and handed over under the callee's convention. That is the ordinary
// argument rule `inline_callback_subset` falls back to for any call it does
// not list, so no list of self-hosted names is kept (it was one until the
// fallback and the module-call audit made it redundant, 2026-11-24).

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
/// except an fs walker's and `list.map`'s, whose closure routes are recorded.
///
/// Any OTHER module call with a literal callback is judged as a self-hosted
/// callee's is (each argument by the ordinary value rule, a literal callback
/// a closure value), and the rest is the module-call audit's
/// (`witness_module_result`, calls_modules.rs), which needs a hook for every
/// droppable argument node: an arm that inlines the callback as a predicate
/// activation (`lower_inlined_predicate`: `list.find_index`,
/// `list.partition`, `set.any` / `set.all` through `list.any` / `list.all`)
/// or a linked self-hosted callee that takes it as a closure value
/// (`lower_arg`) hooks it; an arm that inlines it with no activation hooks
/// nothing, and the frame declines there as `module-arm:unaudited`. What
/// decides is the hook the lowering fires, not a list of arm names.
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
        // Any other call: the ordinary argument rule a self-hosted callee's
        // closure argument takes (a literal callback is a closure VALUE, the
        // closure hooks'), and the module-call audit decides the rest — an
        // arm that inlines the callback without an activation declines there.
        _ => return args.iter().find_map(|a| value_subset(a).map(|w| w.inside("call-arg"))),
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
        // `list.map` with a propagating callback links its fallible self-host
        // form (list.rs fn-value route) and passes the literal as the closure
        // VALUE it is: the ordinary argument rule and the module-call audit
        // judge it, as any other self-hosted callee's closure argument.
        // A `fan` callback that still propagates is lowered once as that
        // closure value and called per element (fan.rs): the element's share
        // into the callee is recorded, the call's carrier is the fan's.
        if (module, func) == ("list", "map") || module == "fan" {
            return args.iter().find_map(|a| value_subset(a).map(|w| w.inside("call-arg")));
        }
        return here(":propagating");
    }
    rest.iter()
        .find_map(|a| value_subset(a).map(|w| w.inside("call-arg")))
        .or_else(|| value_subset(body).map(|w| w.inside("callback")))
}
