//! cert_project.rs's unit tests (a child module: it reads the private items).
use super::*;

#[test]
fn every_known_op_names_a_capability_and_an_unknown_one_the_sentinel() {
    assert_eq!(op_caps(1), &[cap::FS_READ]);
    assert_eq!(op_caps(2), &[cap::FS_WRITE]);
    assert_eq!(op_caps(32), &[cap::ENTROPY]);
    assert_eq!(op_caps(34), &[cap::CLOCK]);
    assert_eq!(op_caps(35), &[cap::STDIN]);
    assert_eq!(op_caps(31), &[SENTINEL]);
    assert!(op_caps(74).is_empty(), "the fan offer reaches no capability of its own");
    assert_eq!(op_caps(117), &[cap::PROC]);
    assert_eq!(op_caps(118), &[cap::ENV]);
    assert_eq!(op_caps(-1), &[SENTINEL]);
}

#[test]
fn a_cycle_condenses_to_one_node_bounded_by_every_member() {
    // 0 -> 1 -> 2 -> 1 (a cycle), 2 reaches FS_READ; 1 is declared pure.
    let direct = vec![BTreeSet::new(), BTreeSet::new(), [cap::FS_READ].into()];
    let callees: Vec<BTreeSet<usize>> = vec![[1].into(), [2].into(), [1].into()];
    let comp = components(&callees);
    assert_eq!(comp[1], comp[2]);
    assert_ne!(comp[0], comp[1]);
    let bound: Vec<BTreeSet<u32>> = vec![EFFECT_BOUND.iter().copied().collect(), PURE_BOUND.iter().copied().collect(), [cap::FS_READ, cap::STDOUT].into()];
    let g = condensed_graph(&direct, &callees, &bound);
    // The cycle's node: bound = pure ∩ {0,3} = {0}, direct {3} — rejected.
    assert!(g.split(';').any(|n| n == "0|3|"), "{g}");
}

#[test]
fn an_undeclared_function_takes_the_least_bound_its_callees_need() {
    // 0 declared pure, calls 1 (helper) which calls 2 (reaches FS_READ).
    let direct = vec![BTreeSet::new(), BTreeSet::new(), [cap::FS_READ].into()];
    let callees = vec![[1].into(), [2].into(), BTreeSet::new()];
    let declared: BTreeMap<usize, (String, Declared)> = [(0, ("f".to_string(), Declared::Pure))].into();
    let b = least_bounds(&direct, &callees, &declared, None);
    assert_eq!(b[1], [cap::FS_READ].into());
    assert_eq!(reach(&direct, &callees, 0), vec![cap::FS_READ]);
}

#[test]
fn only_the_meters_own_clock_reads_leave_the_frame() {
    // #3041: two wall-clock reads and a file read; one read is the meter's.
    let m = Module::default();
    let b = Body { fs_ops: vec![Some(OP_METER_CLOCK), Some(1), Some(OP_METER_CLOCK)], ..Body::default() };
    assert_eq!(direct_caps(&m, &b, 0, &|_| false), [cap::FS_READ, cap::CLOCK].into());
    assert_eq!(direct_caps(&m, &b, 1, &|_| false), [cap::FS_READ, cap::CLOCK].into(), "the second read is the frame's own");
    assert_eq!(direct_caps(&m, &b, 2, &|_| false), [cap::FS_READ].into());
    // A read the stack does not show as the constant is never discounted.
    let hidden = Body { fs_ops: vec![None], ..Body::default() };
    assert_eq!(direct_caps(&m, &hidden, 1, &|_| false), [SENTINEL].into());
}

/// A module importing `almide.fs_call` whose `main` calls it once, with
/// the op pushed by `push_op`.
fn one_call(push_op: impl Fn(&mut wasm_encoder::InstructionSink<'_>)) -> Vec<u8> {
    use wasm_encoder::{CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection, Module as Enc, TypeSection, ValType};
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32; 5], [ValType::I64]);
    types.ty().function([ValType::I32], []);
    let mut imports = ImportSection::new();
    imports.import("almide", "fs_call", EntityType::Function(0));
    let mut funcs = FunctionSection::new();
    funcs.function(1);
    let mut exports = ExportSection::new();
    exports.export("main", ExportKind::Func, 1);
    let mut f = Function::new([]);
    {
        let mut i = f.instructions();
        push_op(&mut i);
        i.i32_const(0).i32_const(0).i32_const(0).i32_const(0).call(0).drop().end();
    }
    let mut code = CodeSection::new();
    code.function(&f);
    let mut m = Enc::new();
    m.section(&types).section(&imports).section(&funcs).section(&exports).section(&code);
    m.finish()
}

#[test]
fn fs_call_ops_reads_constant_ops_and_refuses_a_computed_one() {
    // #4102: the embedded host's allow-list.
    let constant = one_call(|i| {
        i.i32_const(35);
    });
    assert_eq!(fs_call_ops(&constant).unwrap(), Some([35].into()));
    let computed = one_call(|i| {
        i.local_get(0);
    });
    assert_eq!(fs_call_ops(&computed).unwrap(), None, "an op the stack does not show cannot be bounded");
    // Row 19: the host's list keeps the visible constants and drops the
    // computed site, so it fails closed instead of vanishing.
    assert_eq!(fs_call_ops_visible(&constant).unwrap(), [35].into());
    assert_eq!(fs_call_ops_visible(&computed).unwrap(), BTreeSet::new());
}
