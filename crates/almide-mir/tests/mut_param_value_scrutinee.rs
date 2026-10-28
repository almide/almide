//! #4080: `match f(q) { … }` and `if g(q) then …` where `f` / `g` take a `mut`
//! param and return a value. The C-132 move-mode write-back turns each call
//! into a block (`{ let (r, buf) = f(q); q = buf; r }`), and the lowering
//! walled a block scrutinee ("call argument Block not in this brick"). Found
//! through `import deque`: an explicitly imported bundled module's fns are in
//! the program, so their call sites take the write-back that an auto-imported
//! module's in-place stdlib calls never do. The block now absorbs the branch,
//! so both shapes lower and the ownership verifier accepts them.

#[test]
fn a_mut_param_call_as_scrutinee_or_condition_lowers() {
    let src = include_str!("fixtures/mut_param_value_scrutinee.almd");
    let fns = almide_mir::pipeline::lowered_functions(src).expect("the fixture must reach the lowering");
    for name in ["drain", "grown"] {
        let f = fns.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("`{name}` walls (#4080)"));
        assert_eq!(almide_mir::verify_ownership(f), Ok(()), "`{name}`");
    }
}
