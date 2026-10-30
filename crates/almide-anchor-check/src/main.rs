//! `almide-anchor-check <artifact.wasm> <bundle> <anchors>` — exit 0 when
//! every located bounds and init event is the instruction it is anchored at
//! in the shipped bytes (lib.rs), 1 when one is not or when the check runs
//! past its deadline (`DEADLINE_SECS`; a timeout is a reject), 2 on
//! unreadable input.

use std::io::Write;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(at) = args.iter().position(|a| a == "--explain") {
        args.remove(at);
        almide_anchor_check::rc::EXPLAIN.store(true, std::sync::atomic::Ordering::Relaxed);
    }
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
    let a = String::from_utf8_lossy(&a).into_owned();
    let limit = std::time::Duration::from_secs(almide_anchor_check::DEADLINE_SECS);
    match almide_anchor_check::check_within(w, b, a, limit) {
        Ok(r) => {
            println!(
                "ANCHORS OK: init {} site(s) / {} store(s), {} fill(s), {} unlocated; bounds {} check(s), {} access(es), {} unlocated, {} operand pair(s)",
                r.init_sites, r.init_stores, r.fill_sites, r.init_unlocated, r.bounds_checks, r.bounds_accesses, r.bounds_unlocated, r.bounds_operands
            );
            if let Some(rc) = &r.rc {
                println!(
                    "RC OK: {} frame(s), {} event(s) matched to {} call(s), {} rebind(s) with no instruction, {} judge(s) with no rebind, {} arm-internal call(s), {} routine-internal call(s), {} element call(s), {} consumed release(s)",
                    rc.frames, rc.events, rc.calls, rc.rebind_unmatched, rc.judges_unmatched, rc.arm_internal, rc.routine_internal, rc.element, rc.consumed
                );
            }
        }
        Err(e) => {
            println!("ANCHORS REJECT: {e}");
            std::process::exit(1);
        }
    }
}
