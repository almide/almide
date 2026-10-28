//! `almide-anchor-check <artifact.wasm> <bundle> <anchors>` — exit 0 when
//! every located bounds and init event is the instruction it is anchored at
//! in the shipped bytes (lib.rs), 1 when one is not, 2 on unreadable input.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [wasm, bundle, anchors] = args.as_slice() else {
        let _ = writeln!(
            std::io::stderr(),
            "usage: almide-anchor-check <artifact.wasm> <bundle> <anchors>"
        );
        std::process::exit(2);
    };
    let read = |p: &str| {
        std::fs::read(p).unwrap_or_else(|e| {
            let _ = writeln!(std::io::stderr(), "{p}: {e}");
            std::process::exit(2);
        })
    };
    let (w, b, a) = (read(wasm), read(bundle), read(anchors));
    let a = String::from_utf8_lossy(&a);
    match almide_anchor_check::check(&w, &b, &a) {
        Ok(r) => {
            println!(
                "ANCHORS OK: init {} site(s) / {} store(s), {} fill(s), {} unlocated; bounds {} check(s), {} access(es), {} unlocated, {} operand pair(s)",
                r.init_sites, r.init_stores, r.fill_sites, r.init_unlocated, r.bounds_checks, r.bounds_accesses, r.bounds_unlocated, r.bounds_operands
            );
            if let Some(rc) = &r.rc {
                println!(
                    "RC OK: {} frame(s), {} event(s) matched to {} call(s), {} release(s) before an abort, {} rebind(s) with no instruction, {} judge(s) with no rebind, {} arm-internal call(s), {} routine-internal call(s), {} element call(s)",
                    rc.frames, rc.events, rc.calls, rc.pre_abort, rc.rebind_unmatched, rc.judges_unmatched, rc.arm_internal, rc.routine_internal, rc.element
                );
            }
        }
        Err(e) => {
            println!("ANCHORS REJECT: {e}");
            std::process::exit(1);
        }
    }
}
