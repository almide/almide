//! Emit a per-build witness for a named MIR scenario to stdout, for one of the
//! flight-grade properties. Stands in for "the compiler emits a certificate per
//! build" — `proofs/gate.sh` pipes this into the KERNEL-PROVEN checker, which
//! re-verifies the property (ownership = no double-free/leak, names = no
//! dangling MIR reference) on the actual emitted bytes.
//!
//!   emit_cert <scenario> [ownership|names]   (property defaults to ownership)

use almide_mir::{
    certificate::{cap_witness_string, name_witness_string, ownership_certificate},
    CallArg, Capability, Init, MirFunction, Op, Repr, RtFn, ValueId, PLACEHOLDER_LAYOUT,
};

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

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "balanced".to_string());
    let property = std::env::args().nth(2).unwrap_or_else(|| "ownership".to_string());
    let f = scenario(&which);
    match property.as_str() {
        "ownership" => print!("{}", ownership_certificate(&f)),
        "names" => print!("{}", name_witness_string(&f)),
        "caps" => print!("{}", cap_witness_string(&f)),
        other => {
            eprintln!("unknown property: {other} (try: ownership | names | caps)");
            std::process::exit(2);
        }
    }
}
