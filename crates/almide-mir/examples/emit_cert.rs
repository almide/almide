//! Emit a per-build witness for a named MIR scenario to stdout, for one of the
//! flight-grade properties. Stands in for "the compiler emits a certificate per
//! build" — `proofs/gate.sh` pipes this into the KERNEL-PROVEN checker, which
//! re-verifies the property (ownership = no double-free/leak, names = no
//! dangling MIR reference) on the actual emitted bytes.
//!
//!   emit_cert <scenario> [ownership|names]   (property defaults to ownership)

use almide_mir::{
    certificate::{call_modes_witness, cap_witness_string, name_witness_string, ownership_certificate},
    CallArg, Capability, Init, MirFunction, MirParam, Op, Repr, RtFn, ValueId, PLACEHOLDER_LAYOUT,
};
use std::collections::BTreeMap;

fn heap() -> Repr {
    Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
}

fn scenario(which: &str) -> MirFunction {
    let (a, b) = (ValueId(0), ValueId(1));
    match which {
        // var a = ...; var b = a (alias); drop a; drop b  → one balanced object,
        // and every used id is defined → ACCEPT under both properties.
        "balanced" => MirFunction {
            name: "f".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::Opaque },
                Op::Dup { dst: b, src: a },
                Op::Drop { v: a },
                Op::Drop { v: b },
            ],
            ..Default::default()
        },
        // allocate and never release → a leak (ownership checker must reject).
        "leak" => MirFunction {
            name: "f".into(),
            ops: vec![Op::Alloc { dst: a, repr: heap(), init: Init::Opaque }],
            ..Default::default()
        },
        // drop a value id that was never defined → a dangling MIR reference
        // (the name-totality checker must reject). Ownership-wise it is also
        // unbalanced, but this scenario exists to exercise the NAME gate.
        "dangling" => MirFunction {
            name: "f".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::Opaque },
                Op::Drop { v: a },
                Op::Drop { v: ValueId(9) }, // 9 is never defined
            ],
            ..Default::default()
        },
        // prints a scalar (reaches Stdout) AND declares Stdout → within bound,
        // the capability checker must accept.
        "sandboxed" => MirFunction {
            name: "f".into(),
            ops: vec![
                Op::Const { dst: a },
                Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(a)] , result: None },
            ],
            declared_caps: vec![Capability::Stdout],
            ..Default::default()
        },
        // prints a scalar (reaches Stdout) but declares NOTHING → an undeclared
        // host effect, the capability checker must reject.
        "undeclared" => MirFunction {
            name: "f".into(),
            ops: vec![
                Op::Const { dst: a },
                Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(a)] , result: None },
            ],
            declared_caps: vec![], // declares no capability
            ..Default::default()
        },
        other => {
            eprintln!(
                "unknown scenario: {other} \
                 (try: balanced | leak | dangling | sandboxed | undeclared)"
            );
            std::process::exit(2);
        }
    }
}

/// Two-function programs for the CALL-MODE witness (brick 2c). `main` passes a
/// heap Handle to `beep`; in the AGREE program `beep` declares one heap param
/// (borrow — the v1 convention), in the MISMATCH program `beep` declares NO heap
/// param, so the site's actual modes cannot equal the signature — the shape a
/// mis-lowered call boundary produces, which the proven checker must REJECT.
fn modes_scenario(which: &str) -> BTreeMap<String, MirFunction> {
    let (a, p) = (ValueId(0), ValueId(1));
    let beep_params =
        if which == "modes-agree" { vec![MirParam { value: p, repr: heap() }] } else { vec![] };
    let beep = MirFunction { name: "beep".into(), params: beep_params, ..Default::default() };
    let main = MirFunction {
        name: "main".into(),
        ops: vec![
            Op::Alloc { dst: a, repr: heap(), init: Init::Opaque },
            Op::CallFn { dst: None, name: "beep".into(), args: vec![CallArg::Handle(a)], result: None },
            Op::Drop { v: a },
        ],
        ..Default::default()
    };
    let mut program = BTreeMap::new();
    program.insert(beep.name.clone(), beep);
    program.insert(main.name.clone(), main);
    program
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "balanced".to_string());
    let property = std::env::args().nth(2).unwrap_or_else(|| "ownership".to_string());
    if property == "modes" {
        print!("{}", call_modes_witness(&modes_scenario(&which), &|_: &str| false));
        return;
    }
    let f = scenario(&which);
    match property.as_str() {
        "ownership" => print!("{}", ownership_certificate(&f)),
        "names" => print!("{}", name_witness_string(&f)),
        "caps" => print!("{}", cap_witness_string(&f)),
        other => {
            eprintln!("unknown property: {other} (try: ownership | names | caps | modes)");
            std::process::exit(2);
        }
    }
}
