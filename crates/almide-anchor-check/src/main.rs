//! `almide-anchor-check <artifact.wasm> <bundle> <anchors>` — exit 0 when
//! every located bounds, init and fields event is the instruction it is anchored at
//! in the shipped bytes (lib.rs), 1 when one is not or when the check runs
//! past its deadline (`DEADLINE_SECS`; a timeout is a reject), 2 on
//! unreadable input.

use std::io::Write;

/// Probe only: a function body's shape, with each call replaced by its
/// callee's shape and module-specific operands (large constants, global
/// indices) masked, so one routine reads the same in every module.
fn body_shape(bodies: &[almide_anchor_check::Body<'_>], imports: u32, f: u32, memo: &mut std::collections::HashMap<u32, u64>, depth: u32) -> u64 {
    if let Some(h) = memo.get(&f) {
        return *h;
    }
    let Some(b) = f.checked_sub(imports).and_then(|p| bodies.get(p as usize)) else { return 0x1111 };
    memo.insert(f, 0x2222);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for op in &b.ops {
        let s = match op {
            wasmparser::Operator::Call { function_index } => {
                let c = if depth == 0 { 0x3333 } else { body_shape(bodies, imports, *function_index, memo, depth - 1) };
                format!("call#{c:x};")
            }
            wasmparser::Operator::I32Const { value } if *value >= 1024 || *value < -1024 => "i32.const#big;".to_string(),
            wasmparser::Operator::GlobalGet { .. } => "global.get;".to_string(),
            wasmparser::Operator::GlobalSet { .. } => "global.set;".to_string(),
            o => format!("{o:?};"),
        };
        for c in s.bytes() {
            h ^= u64::from(c);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    memo.insert(f, h);
    h
}

/// Probe only: one hash per body after `rounds` rounds of refinement. Round 0
/// hashes a body's own instructions; each later round replaces a call by its
/// callee's hash from the round before, so the value does not depend on the
/// order bodies are visited in or on function indices. `level` 0 abstracts
/// only call targets; level 1 also hides the module-specific operands (large
/// constants, global indices, an indirect call's type index); level 2 keeps
/// only each instruction's name. A call to an import hashes as one fixed token.
fn body_hashes(bodies: &[almide_anchor_check::Body<'_>], imports: u32, level: u8, rounds: u32) -> Vec<u64> {
    // Each instruction once: its text, or the callee's body position.
    let toks: Vec<Vec<Result<String, Option<usize>>>> = bodies
        .iter()
        .map(|b| {
            b.ops
                .iter()
                .map(|op| match op {
                    wasmparser::Operator::Call { function_index } => Err(function_index.checked_sub(imports).map(|p| p as usize)),
                    o if level == 2 => {
                        let text = format!("{o:?}");
                        Ok(format!("{};", text.split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("")))
                    }
                    wasmparser::Operator::I32Const { value } if level == 1 && (*value >= 1024 || *value < -1024) => Ok("i32.const#big;".to_string()),
                    wasmparser::Operator::GlobalGet { .. } if level == 1 => Ok("global.get;".to_string()),
                    wasmparser::Operator::GlobalSet { .. } if level == 1 => Ok("global.set;".to_string()),
                    wasmparser::Operator::CallIndirect { .. } if level == 1 => Ok("call_indirect;".to_string()),
                    o => Ok(format!("{o:?};")),
                })
                .collect()
        })
        .collect();
    let mut prev: Vec<u64> = vec![0; bodies.len()];
    for round in 0..=rounds {
        let next: Vec<u64> = toks
            .iter()
            .map(|body| {
                let mut h: u64 = 0xcbf2_9ce4_8422_2325;
                for t in body {
                    let call;
                    let s: &str = match t {
                        Ok(s) => s.as_str(),
                        Err(callee) => {
                            let c: u64 = match callee {
                                None => 0x1111,
                                Some(_) if round == 0 => 0x3333,
                                Some(p) => prev.get(*p).copied().unwrap_or(0x4444),
                            };
                            call = format!("call#{c:x};");
                            &call
                        }
                    };
                    for c in s.bytes() {
                        h ^= u64::from(c);
                        h = h.wrapping_mul(0x0000_0100_0000_01b3);
                    }
                }
                h
            })
            .collect();
        prev = next;
    }
    prev
}

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
    // Probe only: single-byte mutants of the located instructions, each judged.
    if args.first().map(String::as_str) == Some("--mutate") {
        let (Some(w), Some(b), Some(a)) = (args.get(1), args.get(2), args.get(3)) else { std::process::exit(2) };
        let (w, b) = (std::fs::read(w).unwrap_or_default(), std::fs::read(b).unwrap_or_default());
        let a = String::from_utf8_lossy(&std::fs::read(a).unwrap_or_default()).into_owned();
        let located = almide_anchor_check::located_by(&a).unwrap_or_default();
        let only = std::env::var("MUT_ONLY").ok();
        let ms: Vec<_> = almide_anchor_check::mutate::mutants(&w, &located)
            .unwrap_or_default()
            .into_iter()
            .filter(|m| only.as_deref().is_none_or(|o| m.role == o || m.kind == "branch-depth"))
            .collect();
        // The unmutated module must be accepted, or no verdict below means anything.
        if almide_anchor_check::check(&w, &b, &a).is_err() {
            println!("MUT base/rejected/- skipped {}", ms.len());
            return;
        }
        let mut tally: std::collections::BTreeMap<(String, &str), usize> = std::collections::BTreeMap::new();
        for m in &ms {
            let mut x = w.clone();
            for (p, v) in &m.bytes {
                x[*p] = *v;
            }
            let dec = almide_anchor_check::decode(&x);
            let verdict = if dec.is_err() {
                "invalid"
            } else {
                match almide_anchor_check::check(&x, &b, &a) {
                    Err(_) => "killed",
                    Ok(_) => "survived",
                }
            };
            let class = format!("{}/{}/{}", m.src, m.role, m.kind);
            *tally.entry((class.clone(), verdict)).or_default() += 1;
            if verdict == "survived" && (m.role == "self" || m.role == "addr" || m.role == "cond" || m.role == "control" || m.kind == "branch-depth") {
                println!("MUTSURV {class} {}:{} -> {} {}", m.at.func, m.at.op, m.bytes.first().map_or(0, |b| b.1), m.what.replace(' ', ""));
            }
            // The code around a surviving mutant that could matter to safety:
            // an address or condition operand of a bounds event, or a branch.
            if verdict == "survived" && ((m.src == "bounds" && m.role != "value" && m.role != "self") || m.kind == "branch-depth") {
                if let Some(bd) = dec.as_ref().ok().and_then(|bs| bs.get(m.at.func)) {
                    let lo = m.at.op.saturating_sub(34);
                    let hi = (m.at.op + 10).min(bd.ops.len());
                    let ctx: Vec<String> = (lo..hi).map(|j| format!("{}{:?}", if j == m.at.op { ">>" } else { "" }, bd.ops[j]).replace(' ', "")).collect();
                    println!("MUTCTX {class} {}:{} {}", m.at.func, m.at.op, ctx.join(" ; "));
                }
            }
        }
        for ((k, v), n) in tally {
            println!("MUT {k} {v} {n}");
        }
        return;
    }
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
    let counted = census.then(|| {
        let linear = match &lin_cert {
            None => Ok(std::collections::HashSet::new()),
            Some(p) => almide_anchor_check::lin::verify(&w, &a, String::from_utf8_lossy(&read(p)).trim_end())
                .map(|r| r.located.into_iter().map(|(func, op)| almide_anchor_check::At { func, op }).collect()),
        };
        linear.and_then(|l| almide_anchor_check::decode(&w).and_then(|bodies| almide_anchor_check::census_with(&bodies, &a, &l)))
    });
    // Probe only: the census again, one row per function.
    let by_function = census.then(|| {
        let linear = match &lin_cert {
            None => Ok(std::collections::HashSet::new()),
            Some(p) => almide_anchor_check::lin::verify(&w, &a, String::from_utf8_lossy(&read(p)).trim_end())
                .map(|r| r.located.into_iter().map(|(func, op)| almide_anchor_check::At { func, op }).collect()),
        };
        linear.and_then(|l| almide_anchor_check::decode(&w).and_then(|bodies| almide_anchor_check::census_by_function(&bodies, &a, &l)))
    });
    if census {
        let linear: std::collections::HashSet<almide_anchor_check::At> = match &lin_cert {
            None => std::collections::HashSet::new(),
            Some(p) => almide_anchor_check::lin::verify(&w, &a, String::from_utf8_lossy(&read(p)).trim_end())
                .map(|r| r.located.into_iter().map(|(func, op)| almide_anchor_check::At { func, op }).collect())
                .unwrap_or_default(),
        };
        if let Ok(bodies) = almide_anchor_check::decode(&w) {
            // CENSUSX: the shapes as first measured; CENSUSY: the same rows
            // with the producers named where the classifier said "other".
            for (tag, fine) in [("CENSUSX", false), ("CENSUSY", true)] {
                if let Ok(rows) = almide_anchor_check::census_detail(&bodies, &a, &linear, fine) {
                    for (func, key, n) in rows {
                        println!("{tag} {func} {key} {n}");
                    }
                }
            }
            let imports = almide_anchor_check::function_imports(&w);
            // strict / masked: the body with its call tree (6 rounds); own: the
            // body's own instructions, calls as one token; names: only the
            // instruction names of the body itself.
            let (strict, masked) = (body_hashes(&bodies, imports, 0, 6), body_hashes(&bodies, imports, 1, 6));
            let (own, names) = (body_hashes(&bodies, imports, 0, 0), body_hashes(&bodies, imports, 2, 0));
            for (p, b) in bodies.iter().enumerate() {
                println!("BODYFN {p} {} {:016x} {:016x} {:016x} {:016x}", b.ops.len(), strict[p], masked[p], own[p], names[p]);
            }
        }
    }
    if census {
        if let Ok(rows) = almide_anchor_check::decode(&w).and_then(|bodies| almide_anchor_check::census_writes(&bodies, &a)) {
            for (func, n) in rows {
                println!("STOREFN {func} {}", n.map(|x| x.to_string()).join(" "));
            }
        }
    }
    if census {
        if let (Ok(bodies), Ok((alloc, _, _, judges, holds, _))) = (almide_anchor_check::decode(&w), almide_anchor_check::parse_anchors_held(&a)) {
            let imports = almide_anchor_check::function_imports(&w);
            let mut memo = std::collections::HashMap::new();
            let mut seen = std::collections::BTreeSet::new();
            for (name, sites) in [("judge", &judges), ("hold", &holds)] {
                for at in sites.iter() {
                    if let Some(wasmparser::Operator::Call { function_index }) = bodies.get(at.func).and_then(|b| b.ops.get(at.op)) {
                        seen.insert((name, *function_index));
                    }
                }
            }
            seen.insert(("alloc", alloc));
            for (name, f) in seen {
                let n = f.checked_sub(imports).and_then(|p| bodies.get(p as usize)).map_or(0, |b| b.ops.len());
                let h = body_shape(&bodies, imports, f, &mut memo, 6);
                println!("ROUTINE {name} {f} {h:016x} {n}");
            }
        }
    }
    if let Some(Ok(rows)) = &by_function {
        for r in rows {
            println!(
                "CENSUSFN {} {} {} {} {} {} {} {} {} {}",
                r.func, u8::from(r.frame), r.tally.accesses, r.tally.located, r.tally.linear, r.tally.header, r.tally.bulk, r.other_load, r.other_store, r.other_byte
            );
        }
    }
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
            {
                let mut t: std::collections::BTreeMap<(usize, &str, bool), usize> = std::collections::BTreeMap::new();
                for (at, shape, wr) in &r.bounds_shapes {
                    *t.entry((at.func, *shape, *wr)).or_default() += 1;
                }
                for ((func, shape, wr), n) in t {
                    println!("SHAPEFN {func} {shape} {} {n}", u8::from(wr));
                }
            }
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
