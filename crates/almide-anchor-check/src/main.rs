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
    // #4005: also print the row-4 census (census.rs) after an accepted check.
    let census = args.iter().position(|a| a == "--census").map(|at| args.remove(at)).is_some();
    // `--lin <cert>` (with `--census`): the module's linear certificate; the
    // prim sites it covers count as located once lin.rs verifies it.
    let lin_cert = args.iter().position(|a| a == "--lin").map(|at| {
        args.remove(at);
        if at < args.len() { args.remove(at) } else { std::process::exit(2) }
    });
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
    // `--proved-bytes <artifact.wasm> <anchors>`: the runtime routines the
    // anchor file names, as they ship (proved.rs), one line of decimal bytes
    // each: the input of the proof script's shipped-form decoder.
    if args.first().map(String::as_str) == Some("--proved-bytes") {
        let (Some(w), Some(a)) = (args.get(1), args.get(2)) else { std::process::exit(2) };
        let (w, a) = (std::fs::read(w).unwrap_or_default(), std::fs::read_to_string(a).unwrap_or_default());
        match almide_anchor_check::shipped_routines(&w, &a) {
            Ok((routines, exit)) => {
                for (name, index, code) in routines {
                    println!("SHIPPED {name} {index}{}", code.iter().map(|b| format!(" {b}")).collect::<String>());
                }
                println!("EXIT {}", exit.map_or_else(|| "-".to_string(), |x| x.to_string()));
            }
            Err(e) => {
                println!("PROVED REJECT: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    // `--fresh <artifact.wasm> <anchors>`: the accesses into a block just
    // allocated with a constant size (fresh.rs), three strings of decimal
    // bytes each: the input of the proof script's `fresh_ok`, whose verdict is
    // the kernel's. The anchor file is read for ONE number, the allocator's
    // function index; no event is. A module that names no allocator has no
    // proposal and says so.
    if args.first().map(String::as_str) == Some("--fresh") {
        let (Some(w), Some(a)) = (args.get(1), args.get(2)) else { std::process::exit(2) };
        let (w, a) = (std::fs::read(w).unwrap_or_default(), std::fs::read_to_string(a).unwrap_or_default());
        let alloc = match almide_anchor_check::parse_anchors_held(&a) {
            Ok((alloc, ..)) => alloc,
            Err(e) => {
                println!("FRESH UNREADABLE: {e}");
                std::process::exit(1);
            }
        };
        let bytes = |v: &[u8]| v.iter().map(|b| format!(" {b}")).collect::<String>();
        match almide_anchor_check::fresh::shipped(&w, alloc) {
            Ok(found) => {
                for x in &found {
                    println!("FRESH {} {} {alloc} |{} |{} |{}", x.func, x.access, bytes(&x.pre), bytes(&x.mid), bytes(&x.tail));
                }
                println!("ALLOCATOR {alloc}");
            }
            Err(e) => {
                println!("FRESH UNREADABLE: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    // `--windows <artifact.wasm>`: the guarded-access windows the cutter
    // proposes over every load and store of the module (window.rs), one line
    // of decimal bytes each: the input of the proof script's `window_ok`,
    // whose verdict is the kernel's. No witness and no anchor file is read.
    if args.first().map(String::as_str) == Some("--windows") {
        let Some(w) = args.get(1) else { std::process::exit(2) };
        let w = std::fs::read(w).unwrap_or_default();
        match almide_anchor_check::window::shipped(&w) {
            Ok((accesses, windows)) => {
                for x in &windows {
                    println!("WINDOW {} {}{}", x.func, x.access, x.bytes.iter().map(|b| format!(" {b}")).collect::<String>());
                }
                println!("ACCESSES {accesses}");
            }
            Err(e) => {
                println!("WINDOWS REJECT: {e}");
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
    let counted = census.then(|| {
        let linear = match &lin_cert {
            None => Ok(std::collections::HashSet::new()),
            Some(p) => almide_anchor_check::lin::verify(&w, &a, String::from_utf8_lossy(&read(p)).trim_end())
                .map(|r| r.located.into_iter().map(|(func, op)| almide_anchor_check::At { func, op }).collect()),
        };
        linear.and_then(|l| almide_anchor_check::decode(&w).and_then(|bodies| almide_anchor_check::census_with(&bodies, &a, &l)))
    });
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
            // Per held routine: its call sites and global sites that carry
            // another index than the list's, its out-of-memory addresses
            // other than the probe's, and the exit calls in front of its traps.
            println!(
                "PROVED OK: {} runtime routine(s) held to the proof script's bytes{}",
                r.proved.len(),
                r.proved.iter().map(|(n, d)| format!(" {n}:{}/{}/{}/{}", d.calls, d.globals, d.addresses, d.exits)).collect::<String>()
            );
            match &counted {
                Some(Ok(c)) => println!(
                    "CENSUS: frames {} access(es), {} located ({} by the linear certificate), {} header word(s), {} bulk; routines {} access(es), {} located ({} by the linear certificate), {} header word(s), {} bulk",
                    c.frames.accesses, c.frames.located, c.frames.linear, c.frames.header, c.frames.bulk, c.routines.accesses, c.routines.located, c.routines.linear, c.routines.header, c.routines.bulk
                ),
                Some(Err(e)) => println!("CENSUS: unreadable: {e}"),
                None => {}
            }
            if let Some(rc) = &r.rc {
                println!(
                    "RC OK: {} frame(s), {} event(s) matched to {} call(s), {} rebind(s) with no instruction, {} judge(s) with no rebind, {} signed consumption(s), {} release(s) the last read of their local",
                    rc.frames, rc.events, rc.calls, rc.rebind_unmatched, rc.judges_unmatched, rc.signed, rc.last_reads
                );
            }
        }
        Err(e) => {
            println!("ANCHORS REJECT: {e}");
            std::process::exit(1);
        }
    }
}
