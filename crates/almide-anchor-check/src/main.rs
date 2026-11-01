//! `almide-anchor-check <artifact.wasm> <bundle> <anchors>` — exit 0 when
//! every located bounds, init and fields event is the instruction it is anchored at
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
    // `--lin-skel <artifact.wasm> <anchors>`: the linear certificate's
    // translation of every self-host body (lin.rs), holes as `_`.
    if args.first().map(String::as_str) == Some("--lin-skel") {
        let (Some(w), Some(a)) = (args.get(1), args.get(2)) else { std::process::exit(2) };
        let (w, a) = (std::fs::read(w).unwrap_or_default(), std::fs::read_to_string(a).unwrap_or_default());
        match almide_anchor_check::lin::skeletons(&w, &a, None) {
            Ok(v) => {
                for (pos, r) in v {
                    match r {
                        Ok((_, t)) => println!("{}", almide_anchor_check::lin::render_holes(&t)),
                        Err(e) => println!("REFUSED {pos}: {e}"),
                    }
                }
            }
            Err(e) => {
                println!("LIN REJECT: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    // `--lin-verify <artifact.wasm> <anchors> <cert>`: the linear
    // certificate is the translation of the functions it names (lin.rs).
    if args.first().map(String::as_str) == Some("--lin-verify") {
        let (Some(w), Some(a), Some(c)) = (args.get(1), args.get(2), args.get(3)) else { std::process::exit(2) };
        let (w, a, c) = (
            std::fs::read(w).unwrap_or_default(),
            std::fs::read_to_string(a).unwrap_or_default(),
            std::fs::read_to_string(c).unwrap_or_default(),
        );
        match almide_anchor_check::lin::verify(&w, &a, c.trim_end()) {
            Ok(r) => println!("LIN OK: {} procedure(s), {} prim site(s) certified, {} counted", r.procs, r.sites, r.counted),
            Err(e) => {
                println!("LIN REJECT: {e}");
                std::process::exit(1);
            }
        }
        return;
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
            println!(
                "FIELDS OK: {} site(s) / {} store(s), {} access(es) ({} case field(s) under their tag test), {} tag read(s), {} unlocated",
                r.field_sites, r.field_stores, r.field_accesses, r.field_guarded, r.field_tags, r.field_unlocated
            );
            if let Some(rc) = &r.rc {
                println!(
                    "RC OK: {} frame(s), {} event(s) matched to {} call(s), {} release(s) before an abort, {} rebind(s) with no instruction, {} judge(s) with no rebind, {} signed consumption(s)",
                    rc.frames, rc.events, rc.calls, rc.pre_abort, rc.rebind_unmatched, rc.judges_unmatched, rc.signed
                );
            }
        }
        Err(e) => {
            println!("ANCHORS REJECT: {e}");
            std::process::exit(1);
        }
    }
}
