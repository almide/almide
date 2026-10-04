//! #3327: an in-place mutator (`list.push`, `list.pop`, `map.insert`,
//! `map.delete`, `list.clear`, `string.push`, a field assign, and a push two
//! fields deep) on a field of a mutable module-level record lost its write in
//! the MIR: the call-argument projection hoist bound `g.xs` to a temp and the
//! mutator wrote the temp. Every shape — straight-line, in a loop, in a fold
//! callback — must now write the global back: take the slot's old block
//! (`__mg_take`) and store the new one into the slot, and the ownership
//! verifier must accept the function.

use almide_mir::{Op, PrimKind};

const SLOT0: i64 = 8192; // `mg_slot_addr(0)`: `g` is the module's only `var`

fn writes_back(f: &almide_mir::MirFunction) -> bool {
    let takes = f.ops.iter().any(|op| matches!(op, Op::CallFn { name, .. } if name == "__mg_take"));
    let slot_consts: Vec<_> = f
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::ConstInt { dst, value } if *value == SLOT0 => Some(*dst),
            _ => None,
        })
        .collect();
    let stores = f.ops.iter().any(|op| {
        matches!(op, Op::Prim { kind: PrimKind::Store { .. }, args, .. } if args.first().is_some_and(|a| slot_consts.contains(a)))
    });
    takes && stores
}

#[test]
fn every_mutator_on_a_global_field_writes_the_global_back() {
    let src = include_str!("fixtures/global_field_mutators.almd");
    let fns = almide_mir::pipeline::lowered_functions(src).expect("the fixture must lower");
    let shapes = [
        "push_line", "push_loop", "push_fold", "pop_line", "pop_loop", "insert_line", "insert_loop",
        "delete_line", "clear_line", "clear_loop", "spush_line", "spush_loop", "field_line", "field_loop",
        "deep_line", "deep_loop",
    ];
    for name in shapes {
        let f = fns.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("`{name}` must lower"));
        assert!(writes_back(f), "`{name}` does not write the global back:\n{:#?}", f.ops);
        assert_eq!(almide_mir::verify_ownership(f), Ok(()), "`{name}`");
    }
}
