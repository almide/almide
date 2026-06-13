//! Emit an ownership certificate (format v0) for a named MIR scenario to stdout.
//! Stands in for "the compiler emits a certificate per build" — `proofs/gate.sh`
//! pipes this into the KERNEL-PROVEN checker, which re-verifies memory safety.

use almide_mir::{
    certificate::ownership_certificate, Init, MirFunction, Op, Repr, ValueId, PLACEHOLDER_LAYOUT,
};

fn heap() -> Repr {
    Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "balanced".to_string());
    let (a, b) = (ValueId(0), ValueId(1));
    let f = match which.as_str() {
        // var a = ...; var b = a (alias); drop a; drop b  → one balanced object.
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
        // allocate and never release → a leak (the checker must reject).
        "leak" => MirFunction {
            name: "f".into(),
            ops: vec![Op::Alloc { dst: a, repr: heap(), init: Init::Opaque }],
            ..Default::default()
        },
        other => {
            eprintln!("unknown scenario: {other} (try: balanced | leak)");
            std::process::exit(2);
        }
    };
    print!("{}", ownership_certificate(&f));
}
