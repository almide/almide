//! Recursion in the self-hosted stdlib must not grow with the input's length.
//!
//! The stdlib bodies in `stdlib/*.almd` run on every leg, and nothing about a
//! call like `string.index_of(s, "x")` tells the caller that the library
//! recurses. So a body that recurses once per byte, element or repetition
//! turns an ordinary input into a stack overflow — at a size that differs per
//! leg (wasmtime's stack is far smaller than native's 8 MB), which makes it a
//! cross-target divergence as well as a crash:
//!
//! - #2291 `string.index_of` / `string.count(s, "")`: `__cp_count` was
//!   `step + __cp_count(..)`, one frame per byte of the prefix — wasm trapped
//!   past ~16 KB while native answered.
//! - #2306 `list.iterate` and the fallible carriers behind
//!   `list.map(xs, (x) => f(x)!)!`: `[x] + self(..)` and
//!   `match self(..) { ok(rest) => .. }`, one frame per element — wasm trapped
//!   at ~10,000 elements, native at ~50,000.
//!
//! A tail self-call is fine: both legs turn it into a loop. This gate finds
//! every NON-tail call inside a recursive group — a self-call, or a call
//! between functions of one file that reach each other — and requires each
//! one to be listed in `BOUNDED` with the reason its depth does not follow the
//! input's length (digits of an Int, nesting of a document, ...). A new
//! input-length recursion therefore fails here, in `cargo test`, instead of on
//! a user's 20 KB file. A listed call that no longer exists fails too, so the
//! list only shrinks.
//!
//! Scope: calls by bare name within one file (how every stdlib helper is
//! called). Recursion through a module-qualified name or across files is not
//! seen; none exists today.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use almide::ast::{visit_expr, Decl, Expr, ExprKind, Stmt};
use almide::lexer::Lexer;
use almide::parser::Parser;

/// One frame per decimal digit: `n / 10` each level, so at most 19 for an Int.
const DIGITS: &str = "one frame per decimal digit of an Int (n / 10 per level): at most 19";
/// Nested JSON: one frame per level of array/object nesting in the text.
const JSON_NESTING: &str = "one frame per level of array/object nesting in the JSON text, not per element";
/// Nested `Value`: one frame per level of array/object nesting.
const VALUE_NESTING: &str = "one frame per level of array/object nesting in the Value, not per element";
/// A JSON path: one frame per segment of the path.
const PATH_SEGMENTS: &str = "one frame per segment of the JSON path, not per element of the document";
/// A directory tree / glob: one frame per path segment.
const DIR_DEPTH: &str = "one frame per directory level / path segment, not per directory entry";
/// The regex engine's backtracking points. Bounded by the pattern's size for
/// every shape but a repeated group, which stacks one choice point per
/// repetition — the open input-length recursion #2307. When #2307 replaces
/// these with an explicit backtrack stack the entries go stale and this gate
/// makes the fix delete them.
const REGEX_BACKTRACK: &str =
    "a regex backtracking point: bounded by the pattern's size, EXCEPT under a repeated group (one per repetition — #2307, open)";

const BOUNDED: &[(&str, &str, &str, &str)] = &[
    // (file, function, non-tail callee, why its depth does not follow the input's length)
    ("datetime_format.almd", "__iso_digits", "__iso_digits", DIGITS),
    ("float_to_string.almd", "__sf_pow10_small", "__sf_pow10_small",
        "called only with j < 9 (`__sf_pow10_loop` takes 10^9 steps above that): at most 8 frames"),
    ("float_to_string.almd", "__sf_getbits", "__sf_getbits",
        "re-enters at most once: a negative `pos` recurses with `pos` = 0"),
    ("fs_walk.almd", "__walk_entries", "__walk_into", DIR_DEPTH),
    ("fs_walk.almd", "__glob_dstar_match", "__glob_segs_match", DIR_DEPTH),
    ("int_to_string.almd", "__itos_fill", "__itos_fill", DIGITS),
    ("int_to_string.almd", "__itos_fill_neg", "__itos_fill_neg", DIGITS),
    ("io_read_n_bytes.almd", "__read_n_chunked", "__read_n_chunked",
        "one frame per 64 MiB chunk of the requested length"),
    ("json_parse.almd", "__jp_array", "__jp_value", JSON_NESTING),
    ("json_parse.almd", "__jp_object", "__jp_value", JSON_NESTING),
    ("json_path.almd", "__jps_items", "json_path_set_go", PATH_SEGMENTS),
    ("json_path.almd", "__jps_pairs", "json_path_set_go", PATH_SEGMENTS),
    ("json_path.almd", "json_path_set_field", "__jps_pairs", PATH_SEGMENTS),
    ("json_path.almd", "json_path_set_field", "json_path_set_go", PATH_SEGMENTS),
    ("json_path.almd", "json_path_set_index", "__jps_items", PATH_SEGMENTS),
    ("json_path.almd", "__jpr_items", "json_path_rm_go", PATH_SEGMENTS),
    ("json_path.almd", "__jpr_pairs", "json_path_rm_go", PATH_SEGMENTS),
    ("json_path.almd", "json_path_rm_field", "__jpr_pairs", PATH_SEGMENTS),
    ("json_path.almd", "json_path_rm_index", "__jpr_items", PATH_SEGMENTS),
    ("list_to_string.almd", "__lts_fill_digits", "__lts_fill_digits", DIGITS),
    ("list_to_string.almd", "__lts_fill_neg", "__lts_fill_neg", DIGITS),
    ("map_to_string.almd", "__mts_fill_digits", "__mts_fill_digits", DIGITS),
    ("map_to_string.almd", "__mts_fill_neg", "__mts_fill_neg", DIGITS),
    ("option_to_string_li.almd", "__otsli_fill_digits", "__otsli_fill_digits", DIGITS),
    ("option_to_string_li.almd", "__otsli_fill_neg", "__otsli_fill_neg", DIGITS),
    ("option_to_string_msi.almd", "__omsi_fill_digits", "__omsi_fill_digits", DIGITS),
    ("option_to_string_msi.almd", "__omsi_fill_neg", "__omsi_fill_neg", DIGITS),
    ("option_to_string_nested.almd", "__onest_fill_digits", "__onest_fill_digits", DIGITS),
    ("option_to_string_nested.almd", "__onest_fill_neg", "__onest_fill_neg", DIGITS),
    ("regex_engine.almd", "__rx_ngroups", "__rx_ngroups",
        "one frame per capture group written in the pattern text, not per input character"),
    ("regex_engine.almd", "__rx_alts", "__rx_seq", REGEX_BACKTRACK),
    ("regex_engine.almd", "__rx_rep_group", "__rx_seq", REGEX_BACKTRACK),
    ("regex_engine.almd", "__rx_rep_group", "__rx_group", REGEX_BACKTRACK),
    ("regex_engine.almd", "__rx_rep_lazy", "__rx_seq", REGEX_BACKTRACK),
    ("regex_engine.almd", "__rx_repi_try", "__rx_seq", REGEX_BACKTRACK),
    ("regex_engine.almd", "__rx_run_group_end", "__rx_run", REGEX_BACKTRACK),
    ("result_to_string.almd", "__rts_fill_digits", "__rts_fill_digits", DIGITS),
    ("result_to_string.almd", "__rts_fill_neg", "__rts_fill_neg", DIGITS),
    ("set_to_string.almd", "__sts_fill_digits", "__sts_fill_digits", DIGITS),
    ("set_to_string.almd", "__sts_fill_neg", "__sts_fill_neg", DIGITS),
    ("value_core.almd", "__varr_eq", "value_eq", VALUE_NESTING),
    ("value_core.almd", "__vobj_eq", "value_eq", VALUE_NESTING),
    ("value_core.almd", "__drop_value", "__vdrop_arr", VALUE_NESTING),
    ("value_core.almd", "__drop_value", "__vdrop_obj", VALUE_NESTING),
    ("value_core.almd", "__vdrop_arr", "__drop_value", VALUE_NESTING),
    ("value_core.almd", "__vdrop_obj", "__drop_value", VALUE_NESTING),
    ("value_core.almd", "__vstr_arr", "value_stringify", VALUE_NESTING),
    ("value_core.almd", "__vstr_obj", "value_stringify", VALUE_NESTING),
    ("value_core.almd", "value_stringify", "__vstr_arr", VALUE_NESTING),
    ("value_core.almd", "value_stringify", "__vstr_obj", VALUE_NESTING),
    ("value_core.almd", "__vsp_arr", "json_stringify_pretty_at", VALUE_NESTING),
    ("value_core.almd", "__vsp_obj", "json_stringify_pretty_at", VALUE_NESTING),
    ("value_core.almd", "json_stringify_pretty_at", "__vsp_arr", VALUE_NESTING),
    ("value_core.almd", "json_stringify_pretty_at", "__vsp_obj", VALUE_NESTING),
];

/// Every expression in TAIL position of `e`, assuming `e` itself is: the
/// branches of an `if`, the arms of a `match`, a block's final expression and
/// its `guard ... else` exits, through parens and ascriptions. A call's
/// arguments, a `match` subject, a `let` value, an operand, a lambda body, a
/// loop body and anything under `!` / `ok(..)` are not tail.
fn tail_positions(e: &Expr, out: &mut HashSet<*const Expr>) {
    out.insert(e as *const Expr);
    match &e.kind {
        ExprKind::If { then, else_, .. } | ExprKind::IfLet { then, else_, .. } => {
            tail_positions(then, out);
            tail_positions(else_, out);
        }
        ExprKind::Match { arms, .. } => {
            for arm in arms {
                tail_positions(&arm.body, out);
            }
        }
        ExprKind::Block { stmts, expr } => {
            for stmt in stmts {
                if let Stmt::Guard { else_, .. } | Stmt::GuardLet { else_, .. } = stmt {
                    tail_positions(else_, out);
                }
            }
            if let Some(last) = expr {
                tail_positions(last, out);
            }
        }
        // `x |> f(y)` is the call `f(x, y)`: the call node is the pipe's right side.
        ExprKind::Pipe { right, .. } => tail_positions(right, out),
        ExprKind::Paren { expr } | ExprKind::TypeAscription { expr, .. } => tail_positions(expr, out),
        _ => {}
    }
}

/// Calls in `body` to a function named in `names`: (callee, call node). A pipe
/// into a bare name (`x |> f`) counts as a call whose node is that name.
fn local_calls(body: &Expr, names: &BTreeSet<&'static str>) -> Vec<(&'static str, *const Expr)> {
    let mut out = Vec::new();
    visit_expr(body, &mut |e| match &e.kind {
        ExprKind::Call { callee, .. } => {
            if let ExprKind::Ident { name } = &callee.kind
                && names.contains(name.as_str())
            {
                out.push((name.as_str(), e as *const Expr));
            }
        }
        ExprKind::Pipe { right, .. } => {
            if let ExprKind::Ident { name } = &right.kind
                && names.contains(name.as_str())
            {
                out.push((name.as_str(), &**right as *const Expr));
            }
        }
        _ => {}
    });
    out
}

/// Strongly connected components of the call graph (Tarjan).
fn components(graph: &BTreeMap<&'static str, BTreeSet<&'static str>>) -> Vec<BTreeSet<&'static str>> {
    struct St<'g> {
        graph: &'g BTreeMap<&'static str, BTreeSet<&'static str>>,
        index: BTreeMap<&'static str, usize>,
        low: BTreeMap<&'static str, usize>,
        stack: Vec<&'static str>,
        on_stack: BTreeSet<&'static str>,
        next: usize,
        out: Vec<BTreeSet<&'static str>>,
    }
    fn strong(st: &mut St, v: &'static str) {
        st.index.insert(v, st.next);
        st.low.insert(v, st.next);
        st.next += 1;
        st.stack.push(v);
        st.on_stack.insert(v);
        let succs: Vec<&'static str> = st.graph.get(v).map(|s| s.iter().copied().collect()).unwrap_or_default();
        for w in succs {
            if !st.index.contains_key(w) {
                strong(st, w);
                let lw = st.low[w];
                let lv = st.low[v];
                st.low.insert(v, lv.min(lw));
            } else if st.on_stack.contains(w) {
                let iw = st.index[w];
                let lv = st.low[v];
                st.low.insert(v, lv.min(iw));
            }
        }
        if st.low[v] == st.index[v] {
            let mut comp = BTreeSet::new();
            while let Some(w) = st.stack.pop() {
                st.on_stack.remove(w);
                comp.insert(w);
                if w == v {
                    break;
                }
            }
            st.out.push(comp);
        }
    }
    let mut st = St {
        graph,
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        next: 0,
        out: Vec::new(),
    };
    for &v in graph.keys() {
        if !st.index.contains_key(v) {
            strong(&mut st, v);
        }
    }
    st.out
}

/// The non-tail calls inside a recursive group of `source`: (function, callee).
fn non_tail_recursive_calls(file: &str, source: &str) -> BTreeSet<(&'static str, &'static str)> {
    let tokens = Lexer::tokenize(source);
    let mut parser = Parser::new(tokens);
    let program = parser.parse().unwrap_or_else(|e| panic!("{file}: parse failed: {e:?}"));
    assert!(parser.errors.is_empty(), "{file}: parsed only via error recovery");

    let fns: Vec<(&'static str, &Expr)> = program
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Fn { name, body: Some(body), .. } => Some((name.as_str(), body)),
            _ => None,
        })
        .collect();
    let names: BTreeSet<&'static str> = fns.iter().map(|(n, _)| *n).collect();

    let mut graph: BTreeMap<&'static str, BTreeSet<&'static str>> = BTreeMap::new();
    let mut sites: BTreeMap<&'static str, Vec<(&'static str, bool)>> = BTreeMap::new();
    for (name, body) in &fns {
        let mut tail = HashSet::new();
        tail_positions(body, &mut tail);
        let calls: Vec<(&'static str, bool)> = local_calls(body, &names)
            .into_iter()
            .map(|(callee, node)| (callee, tail.contains(&node)))
            .collect();
        graph.entry(name).or_default().extend(calls.iter().map(|(c, _)| *c));
        sites.entry(name).or_default().extend(calls);
    }

    let mut found = BTreeSet::new();
    for comp in components(&graph) {
        for f in &comp {
            for (callee, is_tail) in &sites[f] {
                if comp.contains(callee) && !is_tail {
                    found.insert((*f, *callee));
                }
            }
        }
    }
    found
}

#[test]
fn stdlib_recursion_depth_is_bounded_or_declared() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("stdlib/ dir missing")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "almd"))
        .collect();
    files.sort();
    assert!(files.len() > 100, "expected the full stdlib sweep, got {}", files.len());

    let mut found: BTreeSet<(String, &'static str, &'static str)> = BTreeSet::new();
    for path in &files {
        let file = path.file_name().unwrap().to_string_lossy().to_string();
        let source = std::fs::read_to_string(path).expect("read stdlib file");
        for (f, callee) in non_tail_recursive_calls(&file, &source) {
            found.insert((file.clone(), f, callee));
        }
    }

    let declared: BTreeSet<(String, &'static str, &'static str)> =
        BOUNDED.iter().map(|(file, f, callee, _)| (file.to_string(), *f, *callee)).collect();
    assert_eq!(declared.len(), BOUNDED.len(), "BOUNDED lists a call twice");

    let undeclared: Vec<_> = found.difference(&declared).collect();
    assert!(
        undeclared.is_empty(),
        "non-tail recursion in the self-hosted stdlib with no stated depth bound:\n{}\n\n\
         Each of these calls keeps its caller's frame alive, so the depth is the number of \
         times the group recurses. If that number follows the input's length (bytes, \
         elements, repetitions), the function overflows the stack on large inputs — first on \
         wasm, whose stack is far smaller than native's — which is #2291 / #2306. Rewrite it \
         so the recursive call is the last thing the function does (carry the running result \
         in an accumulator parameter), or as a loop that grows its result with `list.push`. \
         If the depth is bounded by something else (digits of an Int, nesting of a document), \
         add the call to BOUNDED in {} with that bound.",
        undeclared.iter().map(|(file, f, c)| format!("  stdlib/{file}: {f} -> {c}")).collect::<Vec<_>>().join("\n"),
        file!(),
    );

    let stale: Vec<_> = declared.difference(&found).collect();
    assert!(
        stale.is_empty(),
        "BOUNDED lists calls that are no longer non-tail recursion — delete these entries \
         (the list only shrinks):\n{}",
        stale.iter().map(|(file, f, c)| format!("  stdlib/{file}: {f} -> {c}")).collect::<Vec<_>>().join("\n"),
    );
}

/// The detector itself: each shape it must see, and each it must not.
#[test]
fn detector_classifies_tail_and_non_tail_calls() {
    let source = r#"
fn tail_if(n: Int) -> Int = if n <= 0 then 0 else tail_if(n - 1)

fn tail_match(n: Int, acc: Int) -> Int = match n {
  0 => acc,
  _ => tail_match(n - 1, acc + 1),
}

fn tail_block(n: Int) -> Int = {
  let m = n - 1
  if m <= 0 then 0 else tail_block(m)
}

fn tail_pipe(n: Int) -> Int = if n <= 0 then 0 else (n - 1) |> tail_pipe

fn operand(n: Int) -> Int = if n <= 0 then 0 else 1 + operand(n - 1)

fn let_bound(n: Int) -> Int = if n <= 0 then 0
else {
  let r = let_bound(n - 1)
  r
}

fn subject(n: Int) -> Int = match subject(n - 1) {
  0 => 0,
  r => r,
}

fn argument(n: Int) -> Int = if n <= 0 then 0 else tail_if(argument(n - 1))

fn in_lambda(xs: List[Int]) -> List[Int] = xs |> list.map((x) => in_lambda([x]) |> list.len)

fn mutual_a(n: Int) -> Int = if n <= 0 then 0 else mutual_b(n - 1)

fn mutual_b(n: Int) -> Int = 1 + mutual_a(n)

fn not_recursive(n: Int) -> Int = 1 + tail_if(n)

// The #2291 shape, verbatim before the fix.
fn __cp_count(base: Int, n: Int) -> Int = if n <= 0 then 0
else {
  let b = prim.load8(base)
  let step = if b / 64 == 2 then 0 else 1
  let rest = __cp_count(base + 1, n - 1)
  step + rest
}

// The #2306 shape, verbatim before the fix.
fn iterate[T](seed: T, f: (T) -> T, n: Int) -> List[T] = if n <= 0 then []
else [seed] + iterate(f(seed), f, n - 1)
"#;
    let found = non_tail_recursive_calls("detector_fixture", source);
    let expected: BTreeSet<(&str, &str)> = [
        ("operand", "operand"),
        ("let_bound", "let_bound"),
        ("subject", "subject"),
        ("argument", "argument"),
        ("in_lambda", "in_lambda"),
        ("mutual_b", "mutual_a"),
        ("__cp_count", "__cp_count"),
        ("iterate", "iterate"),
    ]
    .into_iter()
    .collect();
    assert_eq!(found, expected);
}
