//! #4854: the checker's control-flow domain is declared, not defaulted. A
//! module using an operator from a proposal whose branches the checker does
//! not classify (legacy exception handling's `try` / `delegate`) is REFUSED at
//! decode, where it used to be read as if the operator did not branch; and the
//! depth of every instruction is the validator's own control stack, so a
//! block-opening operator the old list did not name cannot mis-depth the rest.
//! Row `anchor.branch-target-depth`.

#[path = "../../../tests/support/novel.rs"]
mod novel;

use almide_anchor_check::decode;

#[test]
fn novel_a_legacy_try_in_a_fresh_function_is_refused() {
    let f = novel::ident("f");
    let tag = novel::ident("t");
    let plain = wat::parse_str(format!("(module (func ${f} (block (nop))))")).unwrap();
    assert!(decode(&plain).is_ok(), "control: a plain module decodes");
    let legacy = wat::parse_str(format!(
        "(module (tag ${tag}) (func ${f} try throw ${tag} catch_all end))"
    ))
    .expect("the legacy-exceptions module assembles");
    let r = decode(&legacy);
    assert!(r.is_err(), "a legacy `try` was decoded as if it did not branch");
}

#[test]
fn novel_depth_is_the_validators_control_stack() {
    let f = novel::ident("g");
    let m = wat::parse_str(format!("(module (func ${f} (block (loop (block (nop))))))")).unwrap();
    let bodies = decode(&m).unwrap();
    // block, loop, block, nop, end, end, end, end(fn)
    assert_eq!(bodies[0].depth, vec![0, 1, 2, 3, 3, 2, 1, 0]);
}
