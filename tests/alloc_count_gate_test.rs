//! #1531 (attack-list A1-6, the roc alloc-count model): EXACT loop-body
//! allocation counts, asserted per shape — here as a STATIC guarantee over
//! the shipped v1 wasm text: for every `(loop …)` region of `$main`, the
//! number of allocation-reaching calls is ZERO. `$alloc` is the module's
//! single allocator, so "allocation-reaching" is decidable from the module
//! alone: a fn allocates iff it calls `$alloc` directly or calls a fn that
//! does (transitive closure over the call graph).
//!
//! Static-zero is STRONGER than a runtime counter reading zero for the
//! exercised N: the property holds for every iteration count, including the
//! ones no harness runs. The controls keep the analysis honest the same way
//! a live counter would — two shapes that allocate per iteration ON PURPOSE
//! (`ys = ys + [i]`, `s = s + int.to_string(i)`) must be NAMED by the same
//! analysis, so a broken extractor cannot go green by seeing nothing.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Ten alloc-free loop shapes (the assertion set) — scalar accumulations,
/// while/for, nested, float, and list scans whose bodies must never touch
/// the allocator.
const ZERO_ALLOC_SHAPES: &[&str] = &[
    "sum_range",
    "product_mod",
    "max_scan",
    "count_evens",
    "fib_iter",
    "gcd_iter",
    "bit_walk",
    "list_scan_sum",
    "nested_scalar",
    "while_accum_float",
];

/// The controls: allocate per iteration on purpose. The analysis must report
/// a NONZERO count for each, proving the extractor and the transitive
/// closure are alive (the #1487 kill-evidence discipline, built in).
const ALLOCATING_CONTROLS: &[&str] = &["control_list_build", "control_string_concat"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn render(name: &str) -> String {
    let src_path = repo_root().join(format!("tests/alloc_shapes/{name}.almd"));
    let source = std::fs::read_to_string(&src_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", src_path.display()));
    let modules = almide_mir::pipeline::bundled_self_modules(&source);
    almide_mir::pipeline::try_render_wasm_source(&source, &modules, false)
        .unwrap_or_else(|e| panic!("{name} must render on the v1 leg (it is the gate's subject): {e}"))
}

/// fn name → the fn names it calls, from the WAT text.
fn call_graph(wat: &str) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in wat.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("(func $") {
            let name = rest.split([' ', '(', ')']).next().unwrap_or("?").to_string();
            current = Some(name.clone());
            out.entry(name).or_default();
            continue;
        }
        if t.starts_with(";;") {
            continue;
        }
        if let Some(f) = &current {
            let mut rest = t;
            while let Some(i) = rest.find("call $") {
                let tail = &rest[i + "call $".len()..];
                let callee: String = tail
                    .chars()
                    .take_while(|c| !c.is_whitespace() && *c != '(' && *c != ')')
                    .collect();
                out.get_mut(f).unwrap().push(callee);
                rest = tail;
            }
        }
    }
    out
}

/// The fns that reach `$alloc` (directly or transitively).
fn allocating_fns(graph: &HashMap<String, Vec<String>>) -> HashSet<String> {
    let mut hot: HashSet<String> = HashSet::new();
    hot.insert("alloc".to_string());
    loop {
        let mut grew = false;
        for (f, callees) in graph {
            if !hot.contains(f) && callees.iter().any(|c| hot.contains(c)) {
                hot.insert(f.clone());
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    hot
}

/// The number of allocation-reaching calls inside `$main`'s loop regions.
fn main_loop_alloc_calls(wat: &str, hot: &HashSet<String>) -> usize {
    let mut count = 0;
    let mut in_main = false;
    let mut loop_depth = 0i32; // paren depth relative to open loop regions
    let mut open_loops = 0;
    for line in wat.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("(func $") {
            in_main = rest.starts_with("main ") || rest.starts_with("main(") || rest == "main";
            open_loops = 0;
            loop_depth = 0;
            continue;
        }
        if !in_main || t.starts_with(";;") {
            continue;
        }
        let opens = t.matches('(').count() as i32;
        let closes = t.matches(')').count() as i32;
        if t.starts_with("(loop ") {
            open_loops += 1;
            loop_depth += opens - closes;
        } else if open_loops > 0 {
            if count_line_alloc_calls(t, hot) > 0 {
                count += count_line_alloc_calls(t, hot);
            }
            loop_depth += opens - closes;
            while open_loops > 0 && loop_depth <= 0 {
                open_loops -= 1;
            }
        }
    }
    count
}

fn count_line_alloc_calls(t: &str, hot: &HashSet<String>) -> usize {
    let mut n = 0;
    let mut rest = t;
    while let Some(i) = rest.find("call $") {
        let tail = &rest[i + "call $".len()..];
        let callee: String = tail
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '(' && *c != ')')
            .collect();
        if hot.contains(&callee) {
            n += 1;
        }
        rest = tail;
    }
    n
}

#[test]
fn loop_bodies_allocate_exactly_zero() {
    let mut failures: Vec<String> = Vec::new();
    for name in ZERO_ALLOC_SHAPES {
        let wat = render(name);
        let hot = allocating_fns(&call_graph(&wat));
        let n = main_loop_alloc_calls(&wat, &hot);
        if n != 0 {
            failures.push(format!("{name}: {n} allocation-reaching call(s) inside a loop body"));
        }
    }
    assert!(
        failures.is_empty(),
        "alloc-count gate (expected zero per iteration):\n{}",
        failures.join("\n")
    );
}

#[test]
fn allocating_controls_are_named_by_the_same_analysis() {
    for name in ALLOCATING_CONTROLS {
        let wat = render(name);
        let hot = allocating_fns(&call_graph(&wat));
        let n = main_loop_alloc_calls(&wat, &hot);
        assert!(
            n > 0,
            "{name} allocates per iteration ON PURPOSE and the analysis saw zero — \
             the extractor or the transitive closure is dead"
        );
    }
}
