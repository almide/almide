//! The FIELDS witness against the shipped bytes (#4013, #4279):
//! `FieldCert.check_fields_cert` proves an accepted witness's sites fill
//! every declared field of their layout and its accesses name declared
//! fields. Here each located event is held to the instruction it names,
//! against the SAME layout table the witness carries:
//!
//! - `L R c C n N z S (o O w W)*` carries no instruction (its anchor is `-`).
//!   R is `3 * index + 0` for a record, `+ 1` for a variant, `+ 2` for a
//!   tuple (witness_fields.rs).
//! - a site `k R c C s I ...` anchored at `a s_0 s_1 ...`: `a` is the
//!   block's `local.set B` / `local.tee B` right after the allocation of S
//!   payload bytes — `i32.const S; call $alloc`, or the inline bump of
//!   alloc_inline.rs (an `if (result i32)` whose then-arm writes S as the
//!   block's `len` and whose `else` is `i32.const S; call $alloc`); each
//!   `s I` a store of field I's width at offset `PAYLOAD + O_I`, addressed by
//!   `local.get B` (or the tee itself) with B unwritten since, at the site's
//!   control depth, nothing leaving the window between, and no unrecorded
//!   store through B in it. A variant's field 0 is its tag word, and its
//!   store must store the constant C.
//! - an access `a R c C f I` is a load or a store of field I's width at
//!   offset `PAYLOAD + O_I`. For a variant of more than one case and a field
//!   past the tag (I >= 1), the access must run only where the block's tag
//!   is C: it sits in the then-arm of an `if` whose condition implies
//!   `i32.load offset=12 (X) == C` (directly, or through the AND that a
//!   pattern test builds as `if (result i32) .. else i32.const 0 end`), and
//!   X is the access's own address — the same variable unwritten between,
//!   or the same field re-loaded from the same block with nothing between
//!   the two loads that may write memory — a store, a bulk or atomic op, or
//!   a call of anything but a function whose own body writes no memory
//!   (operands.rs `same_since`, `readonly_calls`).
//! - a tag read `t R` is `i32.load offset=12`.
//!
//! TRUSTED (TRUSTED_BASE.md): that a block's static type is its layout (the
//! representation), and that a variant block's tag word keeps the case its
//! construction site stored there.

use std::collections::BTreeMap;

use wasmparser::{BlockType, Operator as Op};

use crate::decode::{branch_target, store_width, Body};
use crate::operands::{self, Flow};
use crate::{parse_at, At, Report};

const PAYLOAD: u64 = 12;

/// A layout: the case count, the payload size, the fields `(offset, width)`.
struct Layout {
    n: u64,
    z: u64,
    fields: Vec<(u64, u64)>,
}

fn num(t: &str, tag: char) -> Result<u64, String> {
    t.strip_prefix(tag)
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| format!("witness token `{t}` is not {tag}<n>"))
}

fn is_variant(r: u64) -> bool {
    r % 3 == 1
}

/// A load's width and static offset.
fn load_width(op: &Op<'_>) -> Option<(u64, u64)> {
    Some(match op {
        Op::I32Load { memarg } | Op::F32Load { memarg } => (4, memarg.offset),
        Op::I64Load { memarg } | Op::F64Load { memarg } => (8, memarg.offset),
        _ => return None,
    })
}

struct Fx<'b, 'a> {
    bodies: &'b [Body<'a>],
    alloc: u32,
    ro: std::collections::HashSet<u32>,
    layouts: BTreeMap<(u64, u64), Layout>,
}

impl<'b, 'a> Fx<'b, 'a> {
    fn body(&self, at: At) -> Result<&'b Body<'a>, String> {
        let bodies: &'b [Body<'a>] = self.bodies;
        let b = bodies.get(at.func).ok_or_else(|| format!("no defined function at position {}", at.func))?;
        if at.op >= b.ops.len() {
            return Err(format!("function {} has no instruction {}", at.func, at.op));
        }
        Ok(b)
    }

    fn layout(&self, r: u64, c: u64) -> Result<&Layout, String> {
        self.layouts.get(&(r, c)).ok_or_else(|| format!("no layout for kind ({r}, {c})"))
    }

    fn field(&self, r: u64, c: u64, i: u64) -> Result<(u64, u64), String> {
        let l = self.layout(r, c)?;
        usize::try_from(i)
            .ok()
            .and_then(|i| l.fields.get(i).copied())
            .ok_or_else(|| format!("kind ({r}, {c}) declares no field {i}"))
    }

    /// The allocation of `z` payload bytes ends right before `a`.
    fn allocation(&self, b: &Body<'a>, a: usize, z: u64) -> Result<(), String> {
        let alloc = self.alloc;
        let is_size = |o: &Op<'_>| matches!(o, Op::I32Const { value } if u64::try_from(*value) == Ok(z));
        let is_alloc = |o: &Op<'_>| matches!(o, Op::Call { function_index } if *function_index == alloc);
        if a >= 2 && is_size(&b.ops[a - 2]) && is_alloc(&b.ops[a - 1]) {
            return Ok(());
        }
        // The inline bump: `if (result i32) .. len := S .. else i32.const S
        // call $alloc end`, the `end` right before `a`.
        if a >= 4 && matches!(b.ops[a - 1], Op::End) && is_alloc(&b.ops[a - 2]) && is_size(&b.ops[a - 3]) && matches!(b.ops[a - 4], Op::Else) {
            let d = b.depth[a - 4];
            let s = (0..a - 4)
                .rev()
                .find(|&k| b.depth[k] + 1 == d && matches!(b.ops[k], Op::If { .. }))
                .ok_or("site: the inline allocation has no `if`")?;
            if !matches!(b.ops[s], Op::If { blockty: BlockType::Type(wasmparser::ValType::I32) }) {
                return Err("site: the inline allocation's `if` does not yield the block".into());
            }
            let writes_len = (s + 1..a - 5).any(|k| {
                is_size(&b.ops[k]) && matches!(b.ops[k + 1], Op::I32Store { memarg } if memarg.offset == 4)
            });
            if writes_len {
                return Ok(());
            }
            return Err("site: the inline allocation does not write the block's len".into());
        }
        Err(format!("site allocation: instructions {}..{} are not an allocation of {z} payload bytes", a.saturating_sub(2), a))
    }

    fn site(&self, r: u64, c: u64, a: At, stores: &[(u64, Option<At>)]) -> Result<usize, String> {
        let l = self.layout(r, c)?;
        let b = self.body(a)?;
        let (bl, tee) = match b.ops[a.op] {
            Op::LocalSet { local_index } => (local_index, false),
            Op::LocalTee { local_index } => (local_index, true),
            ref o => return Err(format!("site: anchored at {o:?}, not the block's local.set / local.tee")),
        };
        self.allocation(b, a.op, l.z)?;
        let d0 = b.depth[a.op];
        let mut last = a.op;
        let mut seen = std::collections::BTreeSet::new();
        for &(i, at) in stores {
            let at = at.ok_or_else(|| format!("site: store s{i} unlocated in a located site"))?;
            if at.func != a.func || at.op <= last {
                return Err(format!("site: store s{i} is not after the site and its earlier stores"));
            }
            let (o, w) = self.field(r, c, i)?;
            let (sw, off) = store_width(&b.ops[at.op]).ok_or_else(|| format!("site: s{i} anchored at {:?}, not a store", b.ops[at.op]))?;
            if u64::from(sw) != w || off != PAYLOAD + o {
                return Err(format!("site: s{i} stores {sw} bytes at offset {off}, the layout says {w} at {}", PAYLOAD + o));
            }
            if b.depth[at.op] != d0 {
                return Err(format!("site: s{i} is not at the site's control depth"));
            }
            let p = b.addr[at.op].ok_or_else(|| format!("site s{i}: the store's address has no known producer"))?;
            let by_block = (p > a.op && matches!(b.ops[p], Op::LocalGet { local_index } if local_index == bl)) || (tee && p == a.op);
            if !by_block {
                return Err(format!("site s{i}: the store's address comes from {:?}, not the block's local after the allocation", b.ops[p]));
            }
            if let Some(k) = (a.op + 1..at.op).find(|&k| matches!(b.ops[k], Op::LocalSet { local_index } | Op::LocalTee { local_index } if local_index == bl)) {
                return Err(format!("site s{i}: instruction {k} overwrites the block's local before the store"));
            }
            if is_variant(r) && i == 0 {
                let v = b.args[at.op].get(1).copied().flatten();
                if !v.is_some_and(|v| matches!(b.ops[v], Op::I32Const { value } if u64::try_from(value) == Ok(c))) {
                    return Err(format!("site: the tag store does not store the case {c}"));
                }
            }
            seen.insert(at.op);
            last = at.op;
        }
        for k in a.op + 1..last {
            if b.depth[k] < d0 || (b.depth[k] == d0 && matches!(b.ops[k], Op::End | Op::Else)) {
                return Err(format!("site: instruction {k} leaves the site's block"));
            }
            if branch_target(&b.ops[k], b.depth[k]).is_some_and(|t| t <= d0) {
                return Err(format!("site: instruction {k} ({:?}) branches out of the site's window", b.ops[k]));
            }
            if store_width(&b.ops[k]).is_some() && !seen.contains(&k) {
                if let Some(p) = b.addr[k] {
                    if matches!(b.ops[p], Op::LocalGet { local_index } if local_index == bl) || (tee && p == a.op) {
                        return Err(format!("site: instruction {k} stores into the block unrecorded"));
                    }
                }
            }
        }
        Ok(stores.len())
    }

    fn access(&self, r: u64, c: u64, i: u64, at: At, flows: &mut Flows<'b, 'a>) -> Result<bool, String> {
        let (o, w) = self.field(r, c, i)?;
        let b = self.body(at)?;
        let op = &b.ops[at.op];
        let (aw, off) = load_width(op)
            .or_else(|| store_width(op).map(|(w, o)| (u64::from(w), o)))
            .ok_or_else(|| format!("a: anchored at {op:?}, not a load or store"))?;
        if aw != w || off != PAYLOAD + o {
            return Err(format!("a: {aw} bytes at offset {off}, the layout of ({r}, {c}) says field {i} is {w} at {}", PAYLOAD + o));
        }
        let n = self.layout(r, c)?.n;
        if !(is_variant(r) && n > 1 && i >= 1) {
            return Ok(false);
        }
        let flow = flows.entry(at.func).or_insert_with(|| Flow::new(b, Default::default()).with_readonly(self.ro.clone()));
        operands::case_guarded(flow, at.op, c).map_err(|e| format!("a: the case-{c} field read is not under its tag test: {e}"))?;
        Ok(true)
    }

    fn tag(&self, at: At) -> Result<(), String> {
        let b = self.body(at)?;
        match b.ops[at.op] {
            Op::I32Load { memarg } if memarg.offset == PAYLOAD => Ok(()),
            ref o => Err(format!("t: anchored at {o:?}, not the tag word's load")),
        }
    }
}

type Flows<'b, 'a> = std::collections::HashMap<usize, Flow<'b, 'a>>;

/// Check the fields witness `fields` against its anchors `anch`.
pub fn check_fields(bodies: &[Body<'_>], imports: u32, alloc: u32, fields: &str, anch: &str, rep: &mut Report) -> Result<(), String> {
    if fields.trim().is_empty() && anch.trim().is_empty() {
        return Ok(());
    }
    let (items, groups): (Vec<&str>, Vec<&str>) = (fields.split(';').collect(), anch.split(';').collect());
    if items.len() != groups.len() {
        return Err(format!("fields: {} items, {} anchor groups", items.len(), groups.len()));
    }
    let mut fx = Fx { bodies, alloc, ro: operands::readonly_calls(bodies, imports), layouts: BTreeMap::new() };
    for item in &items {
        let t: Vec<&str> = item.split_whitespace().collect();
        if t.first().is_some_and(|x| x.starts_with('L')) {
            let what = |e: String| format!("fields layout `{item}`: {e}");
            if t.len() < 4 || !(t.len() - 4).is_multiple_of(2) {
                return Err(what("not `L R c C n N z S (o O w W)*`".into()));
            }
            let (r, c, n, z) = (num(t[0], 'L').map_err(what)?, num(t[1], 'c').map_err(what)?, num(t[2], 'n').map_err(what)?, num(t[3], 'z').map_err(what)?);
            let mut fs = Vec::new();
            for p in t[4..].chunks(2) {
                fs.push((num(p[0], 'o').map_err(what)?, num(p[1], 'w').map_err(what)?));
            }
            if fx.layouts.insert((r, c), Layout { n, z, fields: fs }).is_some() {
                return Err(what("a second layout for the kind".into()));
            }
        }
    }
    let mut flows: Flows<'_, '_> = std::collections::HashMap::new();
    for (k, (item, group)) in items.iter().zip(&groups).enumerate() {
        let t: Vec<&str> = item.split_whitespace().collect();
        let g: Vec<Option<At>> = group.split_whitespace().map(parse_at).collect::<Result<_, _>>()?;
        let what = |e: String| format!("fields item {k} `{item}`: {e}");
        let Some(head) = t.first() else {
            return Err(what("empty item".into()));
        };
        if head.starts_with('L') {
            if g.iter().any(Option::is_some) || g.len() != 1 {
                return Err(what("a layout carries one `-` anchor".into()));
            }
            continue;
        }
        if g.iter().all(Option::is_none) {
            rep.field_unlocated += 1;
            continue;
        }
        match head.chars().next() {
            Some('k') => {
                let (r, c) = (num(t[0], 'k').map_err(what)?, num(t.get(1).copied().unwrap_or(""), 'c').map_err(what)?);
                let stores: Vec<u64> = t[2..].iter().map(|x| num(x, 's')).collect::<Result<_, _>>().map_err(what)?;
                if g.len() != 1 + stores.len() {
                    return Err(what(format!("{} anchors for {} events", g.len(), 1 + stores.len())));
                }
                let a = g[0].ok_or_else(|| what("unlocated site".into()))?;
                let paired: Vec<(u64, Option<At>)> = stores.iter().enumerate().map(|(j, &i)| (i, g[j + 1])).collect();
                rep.field_stores += fx.site(r, c, a, &paired).map_err(what)?;
                rep.field_sites += 1;
            }
            Some('a') => {
                if t.len() != 3 || g.len() != 1 {
                    return Err(what("an access is `a R c C f I`, with one anchor".into()));
                }
                let (r, c, i) = (num(t[0], 'a').map_err(what)?, num(t[1], 'c').map_err(what)?, num(t[2], 'f').map_err(what)?);
                let at = g[0].ok_or_else(|| what("unlocated access".into()))?;
                if fx.access(r, c, i, at, &mut flows).map_err(what)? {
                    rep.field_guarded += 1;
                }
                rep.field_accesses += 1;
            }
            Some('t') => {
                if t.len() != 1 || g.len() != 1 {
                    return Err(what("a tag read is `t R`, with one anchor".into()));
                }
                let r = num(t[0], 't').map_err(what)?;
                if !fx.layouts.keys().any(|&(x, _)| x == r) {
                    return Err(what("no layout of the representation".into()));
                }
                fx.tag(g[0].ok_or_else(|| what("unlocated tag read".into()))?).map_err(what)?;
                rep.field_tags += 1;
            }
            _ => return Err(what("unknown item".into())),
        }
    }
    Ok(())
}
