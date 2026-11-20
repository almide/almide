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

/// Task-6 (3): every control operator the checker's domain admits, classified
/// by `branch_target` — a branch names the frame it leaves to, an exit of the
/// function answers `Some(0)`, everything else does not branch — and every
/// operator outside the domain (GC, legacy and current exception handling,
/// stack switching) or no operator at all (an unassigned opcode byte) is
/// refused at decode. Each module is a fresh function, so no table of names
/// stands in for the operators.
#[test]
fn novel_every_control_operator_is_classified_or_refused() {
    use almide_anchor_check::branch_target;
    let f = novel::ident("f");
    // (operator's text name as it opens the instruction, its function body, the
    // expected answer of branch_target at that instruction)
    let admitted: &[(&str, String, Option<u32>)] = &[
        ("Unreachable", "unreachable".into(), None),
        ("Nop", "nop".into(), None),
        ("Block", "(block (nop))".into(), None),
        ("Loop", "(loop (nop))".into(), None),
        ("If", "(if (i32.const 0) (then (nop)) (else (nop)))".into(), None),
        ("Else", "(if (i32.const 0) (then (nop)) (else (nop)))".into(), None),
        ("Br", "(block (block (br 1)))".into(), Some(1)),
        ("BrIf", "(block (block (br_if 0 (i32.const 1))))".into(), Some(2)),
        ("BrTable", "(block (block (block (br_table 0 2 1 (i32.const 0)))))".into(), Some(1)),
        ("BrOnNull", "(block (br_on_null 0 (ref.null func)) (drop))".into(), Some(1)),
        ("BrOnNonNull", "(block (result funcref) (br_on_non_null 0 (ref.null func)) (ref.null func)) drop".into(), Some(1)),
        ("Return", "(block (return))".into(), Some(0)),
        ("Call", format!("(call ${f}_g)"), None),
        ("CallIndirect", "(call_indirect (type $t) (i32.const 0))".into(), None),
        ("CallRef", format!("(call_ref $t (ref.func ${f}_g))"), None),
        ("ReturnCall", format!("(block (return_call ${f}_g))"), Some(0)),
        ("ReturnCallIndirect", "(block (return_call_indirect (type $t) (i32.const 0)))".into(), Some(0)),
        ("ReturnCallRef", format!("(block (return_call_ref $t (ref.func ${f}_g)))"), Some(0)),
    ];
    let module = |body: &str| {
        format!(
            "(module (type $t (func)) (table 1 funcref) (elem declare func ${f}_g) (func ${f}_g) (func ${f} {body}))"
        )
    };
    for (name, body, want) in admitted {
        let bytes = wat::parse_str(module(body)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let bodies = decode(&bytes).unwrap_or_else(|e| panic!("{name} was refused: {e}"));
        let b = bodies.last().expect("the probe function");
        let at = b
            .ops
            .iter()
            .position(|op| format!("{op:?}").split([' ', '{', '(']).next() == Some(*name))
            .unwrap_or_else(|| panic!("{name} not found in {body}"));
        assert_eq!(branch_target(&b.ops[at], b.depth[at]), *want, "{name} in {body} (depth {})", b.depth[at]);
    }
    let tag = novel::ident("t");
    let refused: &[(&str, String)] = &[
        ("try_table (exceptions)", format!("(tag ${tag}) (func ${f} (block (try_table (catch ${tag} 0) (call ${f}))))")),
        ("throw (exceptions)", format!("(tag ${tag}) (func ${f} (throw ${tag}))")),
        ("try (legacy exceptions)", format!("(tag ${tag}) (func ${f} try throw ${tag} catch_all end)")),
        ("br_on_cast (gc)", format!("(func ${f} (block (result anyref) (br_on_cast 0 anyref i31ref (ref.null any)) drop (ref.null any)) drop)")),
    ];
    for (name, items) in refused {
        let bytes = wat::parse_str(format!("(module {items})")).unwrap_or_else(|e| panic!("{name} does not assemble: {e}"));
        assert!(decode(&bytes).is_err(), "{name} was decoded as if the checker classified it");
    }
    // An unassigned opcode byte: a one-function module whose body is that
    // byte then `end` — no operator, so decode refuses it.
    for byte in [0x27u8, 0xc5] {
        let bytes = [
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic, version
            0x01, 0x04, 0x01, 0x60, 0x00, 0x00, // type: () -> ()
            0x03, 0x02, 0x01, 0x00, // func 0: type 0
            0x0a, 0x05, 0x01, 0x03, 0x00, byte, 0x0b, // code: no locals, byte, end
        ];
        assert!(decode(&bytes).is_err(), "unassigned opcode {byte:#04x} was decoded");
    }
    let plain = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00, 0x03, 0x02, 0x01, 0x00, 0x0a, 0x05, 0x01, 0x03, 0x00, 0x01, 0x0b];
    assert!(decode(&plain).is_ok(), "control: the same module with `nop` (0x01) decodes");
}
