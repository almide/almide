//! Rows `anchor.memory-shape` and `anchor.count-word-writes`: the two
//! refusals the bounds rules stand on, each judged over an ENUMERATED matrix
//! rather than a list of cases.
//!
//! memory-shape: every module of one or two memories, each declared or
//! imported, shared or not, 32- or 64-bit, is decoded; exactly the module of
//! one unshared 32-bit memory is judged, every other is refused for the first
//! memory that breaks the shape.
//!
//! count-word-writes: every plain store operator of the instruction set, at
//! every static start 0..=16 bytes from a list block (as the store's offset
//! and as a constant added to the address), and the bulk writes (fill, copy)
//! from the same starts, placed between a bounds check and its access: a
//! write whose bytes meet the count word (bytes 4..8) is refused, every
//! other write is accepted. The function and import names are fresh.

#[path = "../../../tests/support/novel.rs"]
mod novel;
mod common;

use almide_anchor_check::{check_bounds, decode, Report};

#[derive(Clone, Copy)]
struct Mem {
    imported: bool,
    shared: bool,
    wide: bool,
}

fn mem_text(m: Mem, import: &str) -> String {
    let limits = match (m.wide, m.shared) {
        (false, false) => "1",
        (false, true) => "1 1 shared",
        (true, false) => "i64 1",
        (true, true) => "i64 1 1 shared",
    };
    if m.imported {
        format!("(import \"env\" \"{import}\" (memory {limits}))")
    } else {
        format!("(memory {limits})")
    }
}

#[test]
fn novel_every_memory_shape_is_judged_only_when_it_is_one_plain_memory() {
    let f = novel::ident("f");
    let all: Vec<Mem> = (0..8).map(|b| Mem { imported: b & 1 != 0, shared: b & 2 != 0, wide: b & 4 != 0 }).collect();
    let mut shapes: Vec<Vec<Mem>> = all.iter().map(|m| vec![*m]).collect();
    for a in &all {
        for b in &all {
            // A text module lists its imports before its definitions.
            if !(b.imported && !a.imported) {
                shapes.push(vec![*a, *b]);
            }
        }
    }
    let (mut judged, mut refused) = (0, 0);
    for shape in &shapes {
        let mems: Vec<String> = shape.iter().map(|m| mem_text(*m, &novel::ident("m"))).collect();
        let text = format!("(module {} (func ${f} (nop)))", mems.join(" "));
        let bytes = wat::parse_str(&text).unwrap_or_else(|e| panic!("{text}: {e}"));
        let first = shape[0];
        let want = if first.shared {
            Some("shared")
        } else if first.wide {
            Some("64-bit")
        } else if shape.len() > 1 {
            Some("more than one memory")
        } else {
            None
        };
        match (decode(&bytes), want) {
            (Ok(_), None) => judged += 1,
            (Err(e), Some(why)) if e.contains(why) => refused += 1,
            (r, w) => panic!("{text}: expected {w:?}, got {:?}", r.map(|b| b.len())),
        }
    }
    // One plain memory, declared or imported.
    assert_eq!((judged, refused), (2, shapes.len() - 2));
}

/// The `xs[i]` read of check.rs with `write` (of `n` instructions) between
/// the guard and the access. Local 0 is the list block, local 3 an i32.
fn judged(write: &str, n: usize) -> Result<Report, String> {
    let body = format!(
        "local.get 1
         local.get 0 i32.load offset=4 i32.const 8 i32.div_u i64.extend_i32_u
         i64.ge_u
         (if (then i32.const 0 drop unreachable))
         {write}
         local.get 0 local.get 1 i32.wrap_i64 i32.const 8 i32.mul i32.add i64.load offset=12 drop"
    );
    let bytes = common::module("(local i32 i64 i64 i32 i64)", &body);
    let bodies = decode(&bytes)?;
    let mut r = Report::default();
    check_bounds(&bodies, "c0 a0", &format!("1:8 1:{}", 18 + n), &[], &mut r).map(|()| r)
}

#[test]
fn novel_every_write_meeting_the_count_word_is_refused_and_no_other() {
    // The count word: bytes 4..8 of the block.
    const WORD: (i64, i64) = (4, 8);
    // (operator, value pushed, bytes written)
    let stores: [(&str, &str, i64); 9] = [
        ("i32.store", "i32.const 0", 4),
        ("i64.store", "i64.const 0", 8),
        ("f32.store", "f32.const 0", 4),
        ("f64.store", "f64.const 0", 8),
        ("i32.store8", "i32.const 0", 1),
        ("i32.store16", "i32.const 0", 2),
        ("i64.store8", "i64.const 0", 1),
        ("i64.store16", "i64.const 0", 2),
        ("i64.store32", "i64.const 0", 4),
    ];
    let mut cases: Vec<(String, usize, bool)> = Vec::new();
    for start in 0..=16i64 {
        for (op, val, w) in stores {
            let meets = start < WORD.1 && start + w > WORD.0;
            cases.push((format!("local.get 0 {val} {op} offset={start}"), 3, meets));
            cases.push((format!("local.get 0 i32.const {start} i32.add {val} {op}"), 5, meets));
        }
        // A bulk write has no static extent: it runs on from its start.
        let meets = start < WORD.1;
        cases.push((format!("local.get 0 i32.const {start} i32.add i32.const 0 local.get 3 memory.fill"), 6, meets));
        cases.push((format!("local.get 0 i32.const {start} i32.add local.get 3 local.get 3 memory.copy"), 6, meets));
    }
    let mut wrong = Vec::new();
    for (write, n, meets) in &cases {
        let accepted = judged(write, *n).is_ok();
        if accepted == *meets {
            wrong.push(format!("{} `{write}`", if *meets { "ACCEPTED" } else { "REFUSED" }));
        }
    }
    assert!(wrong.is_empty(), "{} of {} writes judged wrongly:\n{}", wrong.len(), cases.len(), wrong.join("\n"));
    assert!(cases.iter().filter(|c| c.2).count() > 50 && cases.iter().filter(|c| !c.2).count() > 50);
}
