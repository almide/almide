// Link-shape gates run before the renderer writes a byte (#2807).
//
// Both walls here refuse a program the renderer would otherwise turn into a
// module that fails validation. A wall is an honest refusal the router can
// report; a module that fails validation reads as "this is an Almide bug" and
// never names the function whose lowering went wrong.

/// Per unlinked callee, the functions that reference it. [`unlinked_call_names`]
/// is its key set; the wall reads the callers so it can name the function
/// that did not lower: a bare `println` refused with no caller cost the
/// reporter of #2807 a long hunt, and `ALMIDE_DBG_UNLINKED=1` was the only way
/// to see who called it.
pub fn unlinked_call_sites(prog: &MirProgram) -> BTreeMap<String, BTreeSet<String>> {
    let resolvable = resolvable_call_names(prog);
    let mut sites: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for f in &prog.functions {
        for op in &f.ops {
            let name = match op {
                Op::CallFn { name, .. } => name.clone(),
                Op::DropVariant { ty, .. } => drop_target_name(ty),
                Op::DropWrapperRec { drop_fn, .. } => drop_target_name(drop_fn),
                _ => continue,
            };
            if !resolvable.contains(&name) {
                sites.entry(name).or_default().insert(f.name.clone());
            }
        }
    }
    sites
}

/// The unlinked-call wall. The callee list stays the text between the prefix
/// and the first ` — ` (`pipeline_c::attribute_unlinked_calls` parses it);
/// the callers ride at the END of the sentence.
fn unlinked_call_wall(sites: &BTreeMap<String, BTreeSet<String>>) -> crate::lower::LowerError {
    let names = sites.keys().cloned().collect::<Vec<_>>().join(", ");
    let callers = sites
        .iter()
        .map(|(callee, from)| {
            let from = from.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ");
            format!("`{callee}` in {from}")
        })
        .collect::<Vec<_>>()
        .join("; ");
    crate::lower::LowerError::Unsupported(format!(
        "unlinked stdlib/runtime call(s) with no wasm definition: {names} — \
         rendering them would emit a dangling `(call $…)` (invalid wasm). \
         Add the callee to the self-host registry or wall the using function. \
         Referenced from: {callers} — the function whose lowering left the call unresolved."
    ))
}

/// A call that binds the result of a function whose wasm signature has NONE.
///
/// The lowering classifies each effect fn's ABI once (a never-err `effect fn
/// … -> Unit` renders with no result) while a call site can still read the
/// callee as a Result carrier: `if c then run()! else fail()!` in a can-err
/// dispatcher bound `run()` as a handle and tail-called it from a function
/// that returns one (#2807). The module then fails validation ("callee
/// returns []"). Refuse it here, naming both functions, instead.
fn void_result_call_wall(prog: &MirProgram) -> Result<(), crate::lower::LowerError> {
    let void: BTreeSet<&str> =
        prog.functions.iter().filter(|f| f.ret.is_none()).map(|f| f.name.as_str()).collect();
    for f in prog.functions.iter().filter(|f| f.ret.is_some()) {
        for op in &f.ops {
            let Op::CallFn { dst: Some(_), name, .. } = op else { continue };
            if void.contains(name.as_str()) {
                return Err(crate::lower::LowerError::Unsupported(format!(
                    "fn `{}` binds the result of `{name}`, which the renderer lowered with no \
                     result (a never-err effect fn); the call site and the callee disagree on \
                     its ABI, so the module would fail validation (#2807)",
                    f.name
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod link_gate_tests {
    use super::*;

    fn func(name: &str, ops: Vec<Op>, ret: Option<ValueId>) -> MirFunction {
        MirFunction { name: name.into(), ops, ret, ..Default::default() }
    }

    fn call(dst: Option<ValueId>, name: &str) -> Op {
        Op::CallFn { dst, name: name.into(), args: vec![], result: None }
    }

    /// The #2807 shape reduced to its ABI: a function that returns a handle
    /// binds the result of a void callee. Rendered, the module fails
    /// validation; the gate refuses it and names both functions.
    #[test]
    fn a_bound_result_from_a_void_callee_walls_naming_both_functions() {
        let prog = MirProgram {
            functions: vec![
                func("run_a", vec![], None),
                func("dispatch_ok", vec![call(Some(ValueId(0)), "run_a")], Some(ValueId(0))),
                func("main", vec![call(None, "dispatch_ok")], None),
            ],
            ..Default::default()
        };
        let msg = try_render_wasm_program(&prog).expect_err("must wall").to_string();
        assert!(
            msg.contains("fn `dispatch_ok` binds the result of `run_a`"),
            "the wall must name the caller and the void callee: {msg}"
        );
    }

    /// A void callee called as a statement is the ordinary shape: no wall.
    #[test]
    fn a_statement_call_of_a_void_callee_is_not_refused() {
        let prog = MirProgram {
            functions: vec![
                func("run_a", vec![], None),
                func("main", vec![call(None, "run_a")], None),
            ],
            ..Default::default()
        };
        assert!(void_result_call_wall(&prog).is_ok());
    }

    /// The unlinked wall names the function holding the dangling call, and
    /// keeps the callee list where `attribute_unlinked_calls` parses it.
    #[test]
    fn the_unlinked_wall_names_the_referencing_function() {
        let prog = MirProgram {
            functions: vec![
                func("dispatch_ok", vec![call(None, "println")], None),
                func("main", vec![call(None, "dispatch_ok")], None),
            ],
            ..Default::default()
        };
        let msg = try_render_wasm_program(&prog).expect_err("must wall").to_string();
        let prefix = "unlinked stdlib/runtime call(s) with no wasm definition: ";
        let names = msg.split(prefix).nth(1).and_then(|r| r.split(" — ").next());
        assert_eq!(names, Some("println"), "the callee list must stay parseable: {msg}");
        assert!(msg.contains("Referenced from: `println` in `dispatch_ok`"), "{msg}");
    }
}
