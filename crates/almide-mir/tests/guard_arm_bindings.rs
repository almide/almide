//! #3267: a `guard … else err(…)` in a can-err effect fn with a `mut` param
//! lowers to a heap-result `if`. The then arm's field write rebinds the param
//! to a copy-on-write clone, and the else arm (the err path, which carries the
//! buffer back) read that clone, a value only the then arm defines. Each arm
//! now starts from the bindings that dominate the `if`, so the err path writes
//! back the unmodified param, and `verify_ownership` accepts every function.

use almide_mir::{Op, ValueId};
use std::collections::BTreeSet;

/// Values defined by an op (the destinations a later op may name).
fn defined(op: &Op) -> Option<ValueId> {
    match op {
        Op::Prim { dst, .. }
        | Op::Call { dst, .. }
        | Op::CallFn { dst, .. }
        | Op::CallImport { dst, .. }
        | Op::CallIndirect { dst, .. }
        | Op::IfThen { dst, .. } => *dst,
        Op::Alloc { dst, .. } | Op::ListLit { dst, .. } | Op::Dup { dst, .. } => Some(*dst),
        _ => None,
    }
}

/// The `Else` that closes the then arm of the `IfThen` at `open`.
fn matching_else(ops: &[Op], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, op) in ops.iter().enumerate().skip(open + 1) {
        match op {
            Op::IfThen { .. } => depth += 1,
            Op::EndIf { .. } if depth > 0 => depth -= 1,
            Op::Else { .. } if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

#[test]
fn a_guard_err_arm_names_only_values_that_dominate_it() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("spec/wasm_cross/mut_param_effect_can_err.almd"))
        .expect("read the fixture");
    let fns = almide_mir::pipeline::lowered_functions(&source).expect("the fixture must lower");
    let names = ["p_int", "p_str", "p_list", "p_opt", "p_rec", "p_res", "p_tup"];
    for name in names {
        let f = fns.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("`{name}` must lower"));
        assert_eq!(almide_mir::verify_ownership(f), Ok(()), "`{name}`");
        // No `Dup` in the guard's else arm names a value its then arm defined.
        let open = f.ops.iter().position(|o| matches!(o, Op::IfThen { .. }));
        let open = open.unwrap_or_else(|| panic!("`{name}` must lower its guard to an if"));
        let els = matching_else(&f.ops, open).unwrap_or_else(|| panic!("`{name}`: the guard has no else"));
        let then_defs: BTreeSet<ValueId> = f.ops[open + 1..els].iter().filter_map(defined).collect();
        for op in &f.ops[els + 1..] {
            if let Op::Dup { src, .. } = op {
                assert!(!then_defs.contains(src), "`{name}`: the else arm reads {src:?}, defined in the then arm");
            }
        }
    }
}
