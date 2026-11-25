//! The row-4 census (#4005): every heap access the shipped bytes contain,
//! and which of them a witness event locates.
//!
//! A located access is one the bounds, init or fields witness names (its
//! anchor is the instruction; an `a` anchored at the release call after
//! its load counts that load) — the accesses the proven checkers and this
//! crate's byte checks hold. Every other load, store, `memory.copy` and
//! `memory.fill` is outside every certificate: the inventory row 4's
//! trusted list owes. A census, not a verdict: it rejects nothing.
//!
//! Each access is classed by where it is — a certified FRAME (a function
//! the anchor file's `rc` line names: user code and linked self-host
//! bodies) or a RUNTIME routine (every other defined function: the fixed
//! helpers, the per-program helpers, drop / copy / equality / display
//! bodies) — and by its shape: a HEADER word (memarg offset below the
//! payload and the address a bare `local.get`: a block's rc, len or cap
//! word), BULK (`memory.copy` / `memory.fill`), or a payload or raw
//! access.
//!
//! [`census_with`] also counts as located the self-host prim sites a
//! verified linear certificate covers (lin.rs, #4005 A4), tallied apart as
//! `linear`.

use std::collections::HashSet;

use wasmparser::Operator as Op;

use crate::decode::{is_load, store_width, Body};
use crate::{parse_anchors_held, parse_at, At};

/// One class's counts.
#[derive(Default, Debug, Clone, Copy)]
pub struct Tally {
    pub accesses: usize,
    pub located: usize,
    pub header: usize,
    pub bulk: usize,
    /// Of `located`, the accesses only a linear certificate locates.
    pub linear: usize,
}

/// The census of one artifact: certified frames, runtime routines.
#[derive(Default, Debug, Clone, Copy)]
pub struct Census {
    pub frames: Tally,
    pub routines: Tally,
}

fn offset(op: &Op<'_>) -> Option<u64> {
    if let Some((_, off)) = store_width(op) {
        return Some(off);
    }
    if !is_load(op) {
        return None;
    }
    Some(match op {
        Op::I32Load { memarg }
        | Op::I64Load { memarg }
        | Op::F32Load { memarg }
        | Op::F64Load { memarg }
        | Op::I32Load8S { memarg }
        | Op::I32Load8U { memarg }
        | Op::I32Load16S { memarg }
        | Op::I32Load16U { memarg }
        | Op::I64Load8S { memarg }
        | Op::I64Load8U { memarg }
        | Op::I64Load16S { memarg }
        | Op::I64Load16U { memarg }
        | Op::I64Load32S { memarg }
        | Op::I64Load32U { memarg }
        | Op::V128Load { memarg } => memarg.offset,
        _ => return None,
    })
}

/// Every `func:op` token of the anchor file's event lines.
pub fn located(anchors: &str) -> Result<HashSet<At>, String> {
    let (_, init, bounds, _, _, _) = parse_anchors_held(anchors)?;
    let fields = crate::fields_anchors(anchors);
    let mut out = HashSet::new();
    for t in [init.as_str(), bounds.as_str(), fields.as_str()]
        .into_iter()
        .flat_map(|l| l.split(|c: char| c.is_whitespace() || c == ';' || c == '|'))
        .filter(|t| !t.is_empty())
    {
        if let Ok(Some(at)) = parse_at(t) {
            out.insert(at);
        }
    }
    Ok(out)
}

/// Probe only: every located instruction with the witness that names it.
pub fn located_by(anchors: &str) -> Result<std::collections::HashMap<At, &'static str>, String> {
    let (_, init, bounds, _, _, _) = parse_anchors_held(anchors)?;
    let fields = crate::fields_anchors(anchors);
    let mut out = std::collections::HashMap::new();
    for (name, line) in [("init", init.as_str()), ("bounds", bounds.as_str()), ("fields", fields.as_str())] {
        for t in line.split(|c: char| c.is_whitespace() || c == ';' || c == '|').filter(|t| !t.is_empty()) {
            if let Ok(Some(at)) = parse_at(t) {
                out.insert(at, name);
            }
        }
    }
    Ok(out)
}

/// The census of `bodies` against `anchors`.
pub fn census(bodies: &[Body<'_>], anchors: &str) -> Result<Census, String> {
    census_with(bodies, anchors, &HashSet::new())
}

/// [`census`] with the accesses a verified linear certificate covers.
pub fn census_with(bodies: &[Body<'_>], anchors: &str, linear: &HashSet<At>) -> Result<Census, String> {
    let at = located(anchors)?;
    let (_, _, _, _, _, rc) = parse_anchors_held(anchors)?;
    let frames: HashSet<usize> = rc.map(|(_, _, f, _, _, _)| f.into_iter().map(|(p, _)| p).collect()).unwrap_or_default();
    let mut c = Census::default();
    for (func, b) in bodies.iter().enumerate() {
        let t = if frames.contains(&func) { &mut c.frames } else { &mut c.routines };
        for (k, op) in b.ops.iter().enumerate() {
            let bulk = matches!(op, Op::MemoryCopy { .. } | Op::MemoryFill { .. });
            let Some(off) = offset(op).or(bulk.then_some(0)) else { continue };
            t.accesses += 1;
            let here = at.contains(&At { func, op: k });
            let before_call = b.ops.get(k + 1).is_some_and(|o| matches!(o, Op::Call { .. })) && at.contains(&At { func, op: k + 1 });
            if here || before_call {
                t.located += 1;
            } else if linear.contains(&At { func, op: k }) {
                t.located += 1;
                t.linear += 1;
            } else if bulk {
                t.bulk += 1;
            } else if off < 12 && b.args[k].first().copied().flatten().is_some_and(|p| matches!(b.ops[p], Op::LocalGet { .. })) {
                t.header += 1;
            }
        }
    }
    Ok(c)
}

/// One function's census (probe only): its position, whether it is a
/// certified frame, its tally, and the accesses no class above takes —
/// loads, stores, and byte-wide ones.
#[derive(Default, Debug, Clone, Copy)]
pub struct FnRow {
    pub func: usize,
    pub frame: bool,
    pub tally: Tally,
    pub other_load: usize,
    pub other_store: usize,
    pub other_byte: usize,
}

/// [`census_with`], one row per defined function that accesses memory.
pub fn census_by_function(bodies: &[Body<'_>], anchors: &str, linear: &HashSet<At>) -> Result<Vec<FnRow>, String> {
    let at = located(anchors)?;
    let (_, _, _, _, _, rc) = parse_anchors_held(anchors)?;
    let frames: HashSet<usize> = rc.map(|(_, _, f, _, _, _)| f.into_iter().map(|(p, _)| p).collect()).unwrap_or_default();
    let mut out = Vec::new();
    for (func, b) in bodies.iter().enumerate() {
        let mut r = FnRow { func, frame: frames.contains(&func), ..FnRow::default() };
        for (k, op) in b.ops.iter().enumerate() {
            let bulk = matches!(op, Op::MemoryCopy { .. } | Op::MemoryFill { .. });
            let Some(off) = offset(op).or(bulk.then_some(0)) else { continue };
            r.tally.accesses += 1;
            let here = at.contains(&At { func, op: k });
            let before_call = b.ops.get(k + 1).is_some_and(|o| matches!(o, Op::Call { .. })) && at.contains(&At { func, op: k + 1 });
            if here || before_call {
                r.tally.located += 1;
            } else if linear.contains(&At { func, op: k }) {
                r.tally.located += 1;
                r.tally.linear += 1;
            } else if bulk {
                r.tally.bulk += 1;
            } else if off < 12 && b.args[k].first().copied().flatten().is_some_and(|p| matches!(b.ops[p], Op::LocalGet { .. })) {
                r.tally.header += 1;
            } else {
                let byte_load = matches!(op, Op::I32Load8S { .. } | Op::I32Load8U { .. } | Op::I64Load8S { .. } | Op::I64Load8U { .. });
                match store_width(op) {
                    Some((1, _)) => r.other_byte += 1,
                    Some(_) => r.other_store += 1,
                    None if byte_load => r.other_byte += 1,
                    None => r.other_load += 1,
                }
            }
        }
        if r.tally.accesses > 0 {
            out.push(r);
        }
    }
    Ok(out)
}

/// `pos:lo-hi` tokens of one anchor-file line (`rcarms`, `rcroutines`).
fn windows(anchors: &str, line: &str) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    for l in anchors.lines() {
        let Some(rest) = l.strip_prefix(line) else { continue };
        for tok in rest.split_whitespace() {
            let Some((p, r)) = tok.split_once(':') else { continue };
            let Some((lo, hi)) = r.split_once('-') else { continue };
            if let (Ok(p), Ok(lo), Ok(hi)) = (p.parse(), lo.parse(), hi.parse()) {
                out.push((p, lo, hi));
            }
        }
    }
    out
}

fn load_width(op: &Op<'_>) -> u32 {
    match op {
        Op::I32Load8S { .. } | Op::I32Load8U { .. } | Op::I64Load8S { .. } | Op::I64Load8U { .. } => 1,
        Op::I32Load16S { .. } | Op::I32Load16U { .. } | Op::I64Load16S { .. } | Op::I64Load16U { .. } => 2,
        Op::I32Load { .. } | Op::F32Load { .. } | Op::I64Load32S { .. } | Op::I64Load32U { .. } => 4,
        Op::I64Load { .. } | Op::F64Load { .. } => 8,
        _ => 16,
    }
}

/// How an access's address was built, read off its producer.
fn addr_shape(b: &Body<'_>, k: usize) -> &'static str {
    let prod = |i: usize, n: usize| b.args.get(i).and_then(|a| a.get(n)).copied().flatten();
    let Some(p) = prod(k, 0) else { return "unknown" };
    match &b.ops[p] {
        Op::LocalGet { .. } | Op::LocalTee { .. } => "local",
        Op::GlobalGet { .. } => "global",
        Op::Call { .. } => "call",
        Op::I32Const { .. } => "const",
        o if is_load(o) => "loaded",
        Op::I32Add => {
            let kind = |n: usize| match prod(p, n).map(|q| &b.ops[q]) {
                Some(Op::LocalGet { .. } | Op::LocalTee { .. }) => 'l',
                Some(Op::I32Const { .. }) => 'c',
                Some(Op::I32Mul | Op::I32Shl) => 'm',
                Some(Op::I32Add) => 'a',
                Some(Op::I32WrapI64) => 'w',
                _ => 'o',
            };
            match (kind(0), kind(1)) {
                ('l', 'c') | ('c', 'l') => "local+const",
                ('l', 'm') | ('m', 'l') | ('a', 'm') | ('m', 'a') => "base+scaled",
                ('l', 'l') => "local+local",
                ('l', 'w') | ('w', 'l') | ('a', 'w') | ('w', 'a') => "base+index",
                ('a', _) | (_, 'a') => "sum",
                _ => "add-other",
            }
        }
        _ => "other",
    }
}

/// Probe only: the producers behind an address [`addr_shape`] does not name —
/// the instruction's name, or for an addition the names of its two operands.
fn addr_fine(b: &Body<'_>, k: usize) -> String {
    let prod = |i: usize, n: usize| b.args.get(i).and_then(|a| a.get(n)).copied().flatten();
    let name = |q: Option<usize>| -> String {
        match q {
            None => "?".to_string(),
            Some(q) => {
                let text = format!("{:?}", b.ops[q]);
                text.split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("").to_string()
            }
        }
    };
    match prod(k, 0) {
        Some(p) if matches!(b.ops[p], Op::I32Add) => format!("add({},{})", name(prod(p, 0)), name(prod(p, 1))),
        p => name(p),
    }
}

/// The accesses [`census_by_function`] leaves in no class, by where they sit
/// (inside an inlined routine's or arm's window, or not), what they are and
/// how their address was built: `(function, key, count)`. With `fine`, an
/// address the classifier calls `add-other` or `other` also names its
/// producers.
pub fn census_detail(bodies: &[Body<'_>], anchors: &str, linear: &HashSet<At>, fine: bool) -> Result<Vec<(usize, String, usize)>, String> {
    let at = located(anchors)?;
    let (routines, arms) = (windows(anchors, "rcroutines "), windows(anchors, "rcarms "));
    let inside = |w: &[(usize, usize, usize)], f: usize, k: usize| w.iter().any(|&(p, lo, hi)| p == f && lo <= k && k <= hi);
    let mut out: std::collections::BTreeMap<(usize, String), usize> = std::collections::BTreeMap::new();
    for (func, b) in bodies.iter().enumerate() {
        for (k, op) in b.ops.iter().enumerate() {
            let Some(off) = offset(op) else { continue };
            let here = at.contains(&At { func, op: k });
            let before_call = b.ops.get(k + 1).is_some_and(|o| matches!(o, Op::Call { .. })) && at.contains(&At { func, op: k + 1 });
            let header = off < 12 && b.args[k].first().copied().flatten().is_some_and(|p| matches!(b.ops[p], Op::LocalGet { .. }));
            if here || before_call || linear.contains(&At { func, op: k }) || header {
                continue;
            }
            let region = if inside(&routines, func, k) {
                "routine"
            } else if inside(&arms, func, k) {
                "arm"
            } else {
                "plain"
            };
            let (kind, width) = match store_width(op) {
                Some((w, _)) => ("store", w),
                None => ("load", load_width(op)),
            };
            let offc = match off {
                0 => "0",
                1..=11 => "hdr",
                12 => "12",
                _ => ">12",
            };
            let shape = addr_shape(b, k);
            let addr = if fine && matches!(shape, "add-other" | "other") { format!("{shape}:{}", addr_fine(b, k)) } else { shape.to_string() };
            *out.entry((func, format!("{region}|{kind}{width}|off{offc}|{addr}"))).or_insert(0) += 1;
        }
    }
    Ok(out.into_iter().map(|((f, key), n)| (f, key, n)).collect())
}

/// Probe only: every instruction of each function that WRITES memory (a
/// store, `memory.copy`, `memory.fill`), as `[writes, located by bounds,
/// located by init, located by fields, header word, bulk, other]`.
pub fn census_writes(bodies: &[Body<'_>], anchors: &str) -> Result<Vec<(usize, [usize; 7])>, String> {
    let by = located_by(anchors)?;
    let mut out = Vec::new();
    for (func, b) in bodies.iter().enumerate() {
        let mut n = [0usize; 7];
        for (k, op) in b.ops.iter().enumerate() {
            let bulk = matches!(op, Op::MemoryCopy { .. } | Op::MemoryFill { .. });
            let off = match store_width(op) {
                Some((_, off)) => off,
                None if bulk => 0,
                None => continue,
            };
            n[0] += 1;
            match by.get(&At { func, op: k }) {
                Some(&"bounds") => n[1] += 1,
                Some(&"init") => n[2] += 1,
                Some(_) => n[3] += 1,
                None if bulk => n[5] += 1,
                None if off < 12 && b.args[k].first().copied().flatten().is_some_and(|p| matches!(b.ops[p], Op::LocalGet { .. })) => n[4] += 1,
                None => n[6] += 1,
            }
        }
        if n[0] > 0 {
            out.push((func, n));
        }
    }
    Ok(out)
}

/// Probe only: per function, its memory accesses by which witnesses locate
/// them. A row is `(func, write, mask, n)`: `mask` has bit 1 for init, 2
/// for bounds, 4 for fields, and 8 when it is the located CALL right after
/// the access that carries those bits (the census's before-call rule).
pub fn census_kinds(bodies: &[Body<'_>], anchors: &str) -> Result<Vec<(usize, bool, u8, usize)>, String> {
    let (_, init, bounds, _, _, _) = parse_anchors_held(anchors)?;
    let fields = crate::fields_anchors(anchors);
    let mut by: std::collections::HashMap<At, u8> = std::collections::HashMap::new();
    for (bit, line) in [(1u8, init.as_str()), (2, bounds.as_str()), (4, fields.as_str())] {
        for t in line.split(|c: char| c.is_whitespace() || c == ';' || c == '|').filter(|t| !t.is_empty()) {
            if let Ok(Some(at)) = parse_at(t) {
                *by.entry(at).or_insert(0) |= bit;
            }
        }
    }
    let mut out: std::collections::BTreeMap<(usize, bool, u8), usize> = std::collections::BTreeMap::new();
    for (func, b) in bodies.iter().enumerate() {
        for (k, op) in b.ops.iter().enumerate() {
            let bulk = matches!(op, Op::MemoryCopy { .. } | Op::MemoryFill { .. });
            if offset(op).is_none() && !bulk {
                continue;
            }
            let write = bulk || store_width(op).is_some();
            let mask = match by.get(&At { func, op: k }) {
                Some(&m) => m,
                None if b.ops.get(k + 1).is_some_and(|o| matches!(o, Op::Call { .. })) => by.get(&At { func, op: k + 1 }).map_or(0, |&m| m | 8),
                None => 0,
            };
            *out.entry((func, write, mask)).or_insert(0) += 1;
        }
    }
    Ok(out.into_iter().map(|((f, w, m), n)| (f, w, m, n)).collect())
}

fn load_width(op: &Op<'_>) -> Option<u64> {
    Some(match op {
        Op::I32Load8S { .. } | Op::I32Load8U { .. } | Op::I64Load8S { .. } | Op::I64Load8U { .. } => 1,
        Op::I32Load16S { .. } | Op::I32Load16U { .. } | Op::I64Load16S { .. } | Op::I64Load16U { .. } => 2,
        Op::I32Load { .. } | Op::F32Load { .. } | Op::I64Load32S { .. } | Op::I64Load32U { .. } => 4,
        Op::I64Load { .. } | Op::F64Load { .. } => 8,
        _ => return None,
    })
}

/// Probe only: per function, the loads and stores whose address is a constant
/// past the result of a call of `alloc` given a constant size `n`, the result
/// held in one local not assigned since, and the access inside `[0, 12 + n)`
/// of it. A row is `(func, [stores, straight header, straight payload, any
/// header, any payload, loads, straight header, straight payload, any
/// header, any payload])`: `straight` has nothing nested between the call
/// and the access, `any` allows whole nested blocks that do not assign the
/// local.
pub fn census_fresh(bodies: &[Body<'_>], alloc: u32) -> Vec<(usize, [usize; 10])> {
    let mut out = Vec::new();
    for (func, b) in bodies.iter().enumerate() {
        let mut n = [0usize; 10];
        for (k, op) in b.ops.iter().enumerate() {
            let (base, w, off) = match (store_width(op), load_width(op)) {
                (Some((w, off)), _) => (0, u64::from(w), off),
                (None, Some(w)) => (5, w, offset(op).unwrap_or(0)),
                _ => continue,
            };
            n[base] += 1;
            let Some(p) = b.args[k].first().copied().flatten() else { continue };
            let (x, c) = match &b.ops[p] {
                Op::LocalGet { local_index } => (*local_index, 0i64),
                Op::I32Add => {
                    let a: Vec<usize> = b.args[p].iter().copied().flatten().collect();
                    if a.len() != 2 {
                        continue;
                    }
                    match (&b.ops[a[0]], &b.ops[a[1]]) {
                        (Op::LocalGet { local_index }, Op::I32Const { value }) | (Op::I32Const { value }, Op::LocalGet { local_index }) => (*local_index, i64::from(*value)),
                        _ => continue,
                    }
                }
                _ => continue,
            };
            // The latest assignment of the local before the access.
            let Some(j) = (0..p).rev().find(|&t| matches!(&b.ops[t], Op::LocalSet { local_index } | Op::LocalTee { local_index } if *local_index == x)) else { continue };
            if b.depth[j] != b.depth[k] || (j..=k).any(|t| b.depth[t] < b.depth[j]) {
                continue;
            }
            let Some(q) = b.args[j].first().copied().flatten() else { continue };
            if !matches!(&b.ops[q], Op::Call { function_index } if *function_index == alloc) {
                continue;
            }
            let Some(a) = b.args[q].first().copied().flatten() else { continue };
            let Op::I32Const { value: size } = &b.ops[a] else { continue };
            let (size, lo) = (i64::from(*size), c + off as i64);
            if size < 0 || c < 0 || lo + w as i64 > 12 + size {
                continue;
            }
            let straight = (q..=k).all(|t| b.depth[t] == b.depth[k]);
            let slot = if lo < 12 { 0 } else { 1 };
            n[base + 3 + slot] += 1;
            if straight {
                n[base + 1 + slot] += 1;
            }
        }
        if n[0] + n[5] > 0 {
            out.push((func, n));
        }
    }
    out
}
