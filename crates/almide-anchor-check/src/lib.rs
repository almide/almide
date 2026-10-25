//! The translation check of #4011 (d): the bounds and init witnesses'
//! EVENTS are the INSTRUCTIONS the shipped wasm bytes contain.
//!
//! The proven checkers (`BoundsCert.check_bounds_cert`,
//! `InitCert.check_init_cert`) judge an event program the structural emitter
//! records as it emits. That the program describes the bytes was trusted:
//! the recording call sat next to the instruction, by the emitter's
//! discipline. This checker decodes the shipped module ITSELF (wasmparser,
//! with its validator for the operand-stack heights) and holds every located
//! event to the instruction it is anchored at:
//!
//! INIT, a fixed site `h H w W n N r R s i ...` anchored at `a s_0 s_1 ...`:
//! - `a` is `i32.const N*W; call $alloc; local.set B`;
//! - each `s i` is a store of W bytes at offset `H + i*W` whose ADDRESS
//!   operand is pushed by `local.get B`, after `a`, with B written by nothing
//!   in between, at `a`'s control depth with nothing in between leaving it
//!   (no `end` below it, no branch to a label at or outside it);
//! - the stores lie in byte order, and every store in the window addressed
//!   through `local.get B` is one of them (none unrecorded).
//!
//! INIT, a loop fill `h H w W v K r R f L t S` anchored at `a f`:
//! `local.get K; i64.const W; i64.mul; i32.wrap_i64; call $alloc;
//! local.set B` at `a`, then the countdown exactly:
//! `local.get B; i32.const H; i32.add; local.set C; block; loop;
//! local.get L; i64.const 0; i64.le_s; br_if 1; <body>; local.get C;
//! i32.const S; i32.add; local.set C; local.get L; i64.const 1; i64.sub;
//! local.set L; br 0; end; end; local.get B` ending at `f`, where the body
//! holds exactly one store, of W bytes at offset 0 addressed by
//! `local.get C`, no branch, and no write of B, C, L or K.
//!
//! BOUNDS, per token: a `c` is `i64.ge_u` (the list compare) or `i32.eqz`
//! (the Bytes room test), then `if` (empty), then `i32.const` — anchored at
//! the const — and that `if` has no `else` and ends in `unreachable` (the
//! abort); an `a` is a load (or a load then the `call` that releases what it
//! loaded); a `w` is a store; a slot's located events lie in one function,
//! in byte order (a `c` before the access it guards).
//!
//! A located event that fails any of these REJECTS. An event anchored `-`
//! (a kill, which no instruction carries; or one in a body that did not ship)
//! is counted, not checked. TRUSTED: that `alloc <index>` names `$alloc`, the
//! compare's operands (bounds), and the representation R (init).

use wasmparser::{BlockType, Operator as Op, Parser, ValidPayload, Validator, WasmFeatures};

/// One defined function's instructions, as the checker needs them.
pub struct Body<'a> {
    pub ops: Vec<Op<'a>>,
    /// Control depth before each instruction (0 = the function's own frame).
    pub depth: Vec<u32>,
    /// For a store: the instruction that pushed its address operand, when
    /// the provenance walk knows it.
    pub addr: Vec<Option<usize>>,
}

/// Every defined function of `bytes`, validated, in position order.
pub fn decode(bytes: &[u8]) -> Result<Vec<Body<'_>>, String> {
    let mut v = Validator::new_with_features(WasmFeatures::all());
    let mut out = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| e.to_string())?;
        if let ValidPayload::Func(func, body) = v.payload(&payload).map_err(|e| e.to_string())? {
            let mut fv = func.into_validator(Default::default());
            let mut br = body.get_binary_reader();
            fv.read_locals(&mut br).map_err(|e| e.to_string())?;
            let mut r = wasmparser::OperatorsReader::new(br);
            let (mut ops, mut depth, mut addr) = (Vec::new(), Vec::new(), Vec::new());
            let mut shadow: Vec<Option<usize>> = Vec::new();
            let mut d = 0u32;
            while !r.eof() {
                let off = r.original_position();
                let op = r.read().map_err(|e| e.to_string())?;
                let i = ops.len();
                let pre = fv.operand_stack_height() as usize;
                let arity = op.operator_arity(&fv);
                addr.push(if store_width(&op).is_some() && pre >= 2 {
                    shadow.get(pre - 2).copied().flatten()
                } else {
                    None
                });
                depth.push(d);
                fv.op(off, &op).map_err(|e| e.to_string())?;
                let post = fv.operand_stack_height() as usize;
                match arity {
                    Some((k, m)) if pre >= k as usize => {
                        let keep = (pre - k as usize).min(post);
                        shadow.truncate(keep);
                        // Pushed by this instruction (or, past a polymorphic
                        // stack, re-derived by it): its own.
                        while shadow.len() < post {
                            shadow.push(Some(i));
                        }
                        let _ = m;
                    }
                    _ => {
                        shadow.iter_mut().for_each(|s| *s = None);
                        shadow.truncate(post);
                        while shadow.len() < post {
                            shadow.push(None);
                        }
                    }
                }
                match op {
                    Op::Block { .. } | Op::Loop { .. } | Op::If { .. } | Op::TryTable { .. } => {
                        d += 1
                    }
                    Op::End => d = d.saturating_sub(1),
                    _ => {}
                }
                ops.push(op);
            }
            out.push(Body { ops, depth, addr });
        }
    }
    Ok(out)
}

/// A store's width in bytes and its static offset.
pub fn store_width(op: &Op<'_>) -> Option<(u32, u64)> {
    Some(match op {
        Op::I32Store { memarg } | Op::F32Store { memarg } | Op::I64Store32 { memarg } => {
            (4, memarg.offset)
        }
        Op::I64Store { memarg } | Op::F64Store { memarg } => (8, memarg.offset),
        Op::I32Store8 { memarg } | Op::I64Store8 { memarg } => (1, memarg.offset),
        Op::I32Store16 { memarg } | Op::I64Store16 { memarg } => (2, memarg.offset),
        Op::V128Store { memarg } => (16, memarg.offset),
        _ => return None,
    })
}

fn is_load(op: &Op<'_>) -> bool {
    matches!(
        op,
        Op::I32Load { .. }
            | Op::I64Load { .. }
            | Op::F32Load { .. }
            | Op::F64Load { .. }
            | Op::I32Load8S { .. }
            | Op::I32Load8U { .. }
            | Op::I32Load16S { .. }
            | Op::I32Load16U { .. }
            | Op::I64Load8S { .. }
            | Op::I64Load8U { .. }
            | Op::I64Load16S { .. }
            | Op::I64Load16U { .. }
            | Op::I64Load32S { .. }
            | Op::I64Load32U { .. }
            | Op::V128Load { .. }
    )
}

/// A branch's target depth (the frame it leaves to), or `Some(0)` for an
/// exit of the whole function; `None` for an instruction that does not
/// branch.
fn branch_target(op: &Op<'_>, d: u32) -> Option<u32> {
    match op {
        Op::Br { relative_depth }
        | Op::BrIf { relative_depth }
        | Op::BrOnNull { relative_depth }
        | Op::BrOnNonNull { relative_depth } => Some(d.saturating_sub(*relative_depth)),
        Op::BrTable { targets } => {
            let max = targets
                .targets()
                .filter_map(Result::ok)
                .chain(std::iter::once(targets.default()))
                .max()
                .unwrap_or(0);
            Some(d.saturating_sub(max))
        }
        Op::Return
        | Op::ReturnCall { .. }
        | Op::ReturnCallIndirect { .. }
        | Op::ReturnCallRef { .. }
        | Op::Throw { .. }
        | Op::ThrowRef
        | Op::Rethrow { .. } => Some(0),
        _ => None,
    }
}

/// A located event: a defined function's position and an instruction index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct At {
    pub func: usize,
    pub op: usize,
}

/// An anchor token: located, unlocated (`-`), or broken (anything else).
fn parse_at(t: &str) -> Result<Option<At>, String> {
    if t == "-" {
        return Ok(None);
    }
    let (f, o) = t
        .split_once(':')
        .ok_or_else(|| format!("anchor `{t}` is not position:ordinal"))?;
    match (f.parse(), o.parse()) {
        (Ok(func), Ok(op)) => Ok(Some(At { func, op })),
        _ => Err(format!("anchor `{t}` is not position:ordinal")),
    }
}

/// What a check saw.
#[derive(Default, Debug)]
pub struct Report {
    pub init_sites: usize,
    pub init_stores: usize,
    pub fill_sites: usize,
    pub init_unlocated: usize,
    pub bounds_checks: usize,
    pub bounds_accesses: usize,
    pub bounds_unlocated: usize,
}

struct Ctx<'b, 'a> {
    bodies: &'b [Body<'a>],
    alloc: u32,
}

impl<'a> Ctx<'_, 'a> {
    fn body(&self, at: At) -> Result<&Body<'a>, String> {
        let b = self
            .bodies
            .get(at.func)
            .ok_or_else(|| format!("no defined function at position {}", at.func))?;
        if at.op >= b.ops.len() {
            return Err(format!("function {} has no instruction {}", at.func, at.op));
        }
        Ok(b)
    }

    /// `ops[at.op - back + k]` for `k` in the pattern.
    fn expect(
        &self,
        b: &Body<'a>,
        start: usize,
        want: &[&dyn Fn(&Op<'_>) -> bool],
        what: &str,
    ) -> Result<(), String> {
        for (k, f) in want.iter().enumerate() {
            match b.ops.get(start + k) {
                Some(op) if f(op) => {}
                other => return Err(format!("{what}: instruction {} is {other:?}", start + k)),
            }
        }
        Ok(())
    }

    /// Nothing in `(from, to)` leaves the control region of depth `d0`:
    /// no instruction at a depth below it, no branch to a label at or
    /// outside it.
    fn stays(
        &self,
        b: &Body<'a>,
        from: usize,
        to: usize,
        d0: u32,
        what: &str,
    ) -> Result<(), String> {
        for k in from + 1..to {
            if b.depth[k] < d0 || (b.depth[k] == d0 && matches!(b.ops[k], Op::End | Op::Else)) {
                return Err(format!("{what}: instruction {k} leaves the site's block"));
            }
            if let Some(t) = branch_target(&b.ops[k], b.depth[k]) {
                if t <= d0 {
                    return Err(format!(
                        "{what}: instruction {k} ({:?}) branches out of the site's window",
                        b.ops[k]
                    ));
                }
            }
        }
        Ok(())
    }

    fn writes(op: &Op<'_>, l: u32) -> bool {
        matches!(op, Op::LocalSet { local_index } | Op::LocalTee { local_index } if *local_index == l)
    }

    fn fixed_site(
        &self,
        hdr: u64,
        w: u64,
        n: u64,
        a: At,
        stores: &[(u64, Option<At>)],
    ) -> Result<usize, String> {
        let b = self.body(a)?;
        if a.op < 2 {
            return Err("site: no room for its allocation".into());
        }
        let bl = match b.ops[a.op] {
            Op::LocalSet { local_index } => local_index,
            ref o => {
                return Err(format!(
                    "site: anchored at {o:?}, not the block's local.set"
                ))
            }
        };
        let size = (n * w) as i64;
        let alloc = self.alloc;
        self.expect(
            b,
            a.op - 2,
            &[
                &|o| matches!(o, Op::I32Const { value } if i64::from(*value) == size),
                &|o| matches!(o, Op::Call { function_index } if *function_index == alloc),
            ],
            "site allocation",
        )?;
        let d0 = b.depth[a.op];
        let mut last = a.op;
        let mut seen = std::collections::BTreeSet::new();
        for &(i, at) in stores {
            let Some(at) = at else {
                return Err(format!("site: store s{i} unlocated in a located site"));
            };
            if at.func != a.func || at.op <= last {
                return Err(format!(
                    "site: store s{i} is not after the site and its earlier stores"
                ));
            }
            let (sw, off) = store_width(&b.ops[at.op])
                .ok_or_else(|| format!("site: s{i} anchored at {:?}, not a store", b.ops[at.op]))?;
            if u64::from(sw) != w || off != hdr + i * w {
                return Err(format!(
                    "site: s{i} stores {sw} bytes at offset {off}, the site says {w} at {}",
                    hdr + i * w
                ));
            }
            if b.depth[at.op] != d0 {
                return Err(format!("site: s{i} is not at the site's control depth"));
            }
            self.address_is(b, a.op, at.op, bl, &format!("site s{i}"))?;
            seen.insert(at.op);
            last = at.op;
        }
        self.stays(b, a.op, last, d0, "site")?;
        // None unrecorded: every store in the window addressed through the
        // block's local is one of the site's.
        for k in a.op + 1..last {
            if store_width(&b.ops[k]).is_some() && !seen.contains(&k) {
                if let Some(p) = b.addr[k] {
                    if matches!(b.ops[p], Op::LocalGet { local_index } if local_index == bl) {
                        return Err(format!(
                            "site: instruction {k} stores into the block unrecorded"
                        ));
                    }
                }
            }
        }
        Ok(stores.len())
    }

    /// The store at `s` takes its address from `local.get l`, pushed after
    /// `from` with `l` unwritten in between.
    fn address_is(
        &self,
        b: &Body<'a>,
        from: usize,
        s: usize,
        l: u32,
        what: &str,
    ) -> Result<(), String> {
        let p = b.addr[s]
            .ok_or_else(|| format!("{what}: the store's address has no known producer"))?;
        if p <= from || !matches!(b.ops[p], Op::LocalGet { local_index } if local_index == l) {
            return Err(format!("{what}: the store's address comes from {:?}, not local.get {l} after the allocation", b.ops[p]));
        }
        if let Some(k) = (from + 1..s).find(|&k| Self::writes(&b.ops[k], l)) {
            return Err(format!(
                "{what}: instruction {k} overwrites the block's local before the store"
            ));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_site(
        &self,
        hdr: u64,
        w: u64,
        k: u32,
        l: u32,
        s: u64,
        a: At,
        f: At,
    ) -> Result<(), String> {
        let b = self.body(a)?;
        self.body(f)?;
        if f.func != a.func || a.op < 5 {
            return Err("fill: the fill is not in the site's function".into());
        }
        let bl = match b.ops[a.op] {
            Op::LocalSet { local_index } => local_index,
            ref o => {
                return Err(format!(
                    "fill: anchored at {o:?}, not the block's local.set"
                ))
            }
        };
        let alloc = self.alloc;
        let (w_i, h_i, s_i) = (w as i64, hdr as i32, s as i32);
        self.expect(
            b,
            a.op - 5,
            &[
                &|o| matches!(o, Op::LocalGet { local_index } if *local_index == k),
                &|o| matches!(o, Op::I64Const { value } if *value == w_i),
                &|o| matches!(o, Op::I64Mul),
                &|o| matches!(o, Op::I32WrapI64),
                &|o| matches!(o, Op::Call { function_index } if *function_index == alloc),
            ],
            "fill allocation",
        )?;
        let c = match b.ops.get(a.op + 4) {
            Some(Op::LocalSet { local_index }) => *local_index,
            other => return Err(format!("fill: the cursor's local.set is {other:?}")),
        };
        if [bl, l, k].contains(&c) || bl == l {
            return Err(
                "fill: the cursor, the block and the counter must be distinct locals".into(),
            );
        }
        self.expect(
            b,
            a.op + 1,
            &[
                &|o| matches!(o, Op::LocalGet { local_index } if *local_index == bl),
                &|o| matches!(o, Op::I32Const { value } if *value == h_i),
                &|o| matches!(o, Op::I32Add),
                &|o| matches!(o, Op::LocalSet { .. }),
                &|o| {
                    matches!(
                        o,
                        Op::Block {
                            blockty: BlockType::Empty
                        }
                    )
                },
                &|o| {
                    matches!(
                        o,
                        Op::Loop {
                            blockty: BlockType::Empty
                        }
                    )
                },
                &|o| matches!(o, Op::LocalGet { local_index } if *local_index == l),
                &|o| matches!(o, Op::I64Const { value: 0 }),
                &|o| matches!(o, Op::I64LeS),
                &|o| matches!(o, Op::BrIf { relative_depth: 1 }),
            ],
            "fill loop head",
        )?;
        let body_from = a.op + 11;
        if f.op < body_from + 11 {
            return Err("fill: no room for the loop's tail".into());
        }
        let tail = f.op - 11;
        self.expect(
            b,
            tail,
            &[
                &|o| matches!(o, Op::LocalGet { local_index } if *local_index == c),
                &|o| matches!(o, Op::I32Const { value } if *value == s_i),
                &|o| matches!(o, Op::I32Add),
                &|o| matches!(o, Op::LocalSet { local_index } if *local_index == c),
                &|o| matches!(o, Op::LocalGet { local_index } if *local_index == l),
                &|o| matches!(o, Op::I64Const { value: 1 }),
                &|o| matches!(o, Op::I64Sub),
                &|o| matches!(o, Op::LocalSet { local_index } if *local_index == l),
                &|o| matches!(o, Op::Br { relative_depth: 0 }),
                &|o| matches!(o, Op::End),
                &|o| matches!(o, Op::End),
                &|o| matches!(o, Op::LocalGet { local_index } if *local_index == bl),
            ],
            "fill loop tail",
        )?;
        let d_body = b.depth[body_from];
        let mut stores = 0;
        for j in body_from..tail {
            let op = &b.ops[j];
            if branch_target(op, b.depth[j]).is_some() {
                return Err(format!("fill: the loop body branches at instruction {j}"));
            }
            if [bl, c, l, k].iter().any(|&x| Self::writes(op, x)) {
                return Err(format!(
                    "fill: the loop body writes the block, cursor or counter at instruction {j}"
                ));
            }
            if let Some((sw, off)) = store_width(op) {
                stores += 1;
                if u64::from(sw) != w || off != 0 || b.depth[j] != d_body {
                    return Err(format!("fill: the loop body's store is {op:?}"));
                }
                match b.addr[j] {
                    Some(p)
                        if p >= body_from
                            && matches!(b.ops[p], Op::LocalGet { local_index } if local_index == c) =>
                        {}
                    _ => {
                        return Err(
                            "fill: the loop body's store is not addressed by the cursor".into()
                        )
                    }
                }
            }
        }
        if stores != 1 {
            return Err(format!(
                "fill: the loop body holds {stores} stores, not one"
            ));
        }
        Ok(())
    }

    fn check_guard(&self, at: At) -> Result<(), String> {
        let b = self.body(at)?;
        if at.op < 2 {
            return Err("c: no room for the compare".into());
        }
        self.expect(
            b,
            at.op - 2,
            &[
                &|o| matches!(o, Op::I64GeU | Op::I32Eqz),
                &|o| {
                    matches!(
                        o,
                        Op::If {
                            blockty: BlockType::Empty
                        }
                    )
                },
                &|o| matches!(o, Op::I32Const { .. }),
            ],
            "c",
        )?;
        // The `if`'s own body: no `else`, and it ends in `unreachable`.
        let d_in = b.depth[at.op];
        let end = (at.op..b.ops.len())
            .find(|&j| b.depth[j] == d_in && matches!(b.ops[j], Op::End | Op::Else))
            .ok_or("c: the guard's `if` has no end")?;
        if !matches!(b.ops[end], Op::End) {
            return Err("c: the guard's `if` has an `else`".into());
        }
        if !matches!(b.ops[end - 1], Op::Unreachable) {
            return Err(format!(
                "c: the guard's `if` ends in {:?}, not an abort",
                b.ops[end - 1]
            ));
        }
        Ok(())
    }

    fn access(&self, at: At, write: bool) -> Result<(), String> {
        let b = self.body(at)?;
        let op = &b.ops[at.op];
        let ok = if write {
            store_width(op).is_some()
        } else {
            is_load(op)
                || (matches!(op, Op::Call { .. }) && at.op > 0 && is_load(&b.ops[at.op - 1]))
        };
        if ok {
            Ok(())
        } else {
            Err(format!(
                "{}: anchored at {op:?}",
                if write { "w" } else { "a" }
            ))
        }
    }
}

fn num(t: &str, tag: char) -> Result<u64, String> {
    t.strip_prefix(tag)
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| format!("witness token `{t}` is not {tag}<n>"))
}

/// Check the init witness `init` against its anchors `anch` over `bodies`.
pub fn check_init(
    bodies: &[Body<'_>],
    alloc: u32,
    init: &str,
    anch: &str,
    rep: &mut Report,
) -> Result<(), String> {
    let ctx = Ctx { bodies, alloc };
    let (sites, anchors): (Vec<&str>, Vec<&str>) =
        (init.split(';').collect(), anch.split(';').collect());
    if init.trim().is_empty() && anch.trim().is_empty() {
        return Ok(());
    }
    if sites.len() != anchors.len() {
        return Err(format!(
            "init: {} sites, {} anchor groups",
            sites.len(),
            anchors.len()
        ));
    }
    for (n, (site, group)) in sites.iter().zip(&anchors).enumerate() {
        let t: Vec<&str> = site.split_whitespace().collect();
        let g: Vec<Option<At>> = group
            .split_whitespace()
            .map(parse_at)
            .collect::<Result<_, _>>()?;
        let at = |k: usize| g.get(k).copied().flatten();
        let what = |e: String| format!("init site {n} `{site}`: {e}");
        if t.len() < 4 {
            return Err(what("too short".into()));
        }
        let (h, w) = (num(t[0], 'h').map_err(what)?, num(t[1], 'w').map_err(what)?);
        if g.iter().all(Option::is_none) {
            rep.init_unlocated += 1;
            continue;
        }
        if t[2].starts_with('v') {
            // `h H w W v K r R f L t S`
            if t.len() != 6 || g.len() != 2 {
                return Err(what(
                    "a fill site is `h w v r f t`, with two anchors".into(),
                ));
            }
            let (k, l, s) = (
                num(t[2], 'v').map_err(what)?,
                num(t[4], 'f').map_err(what)?,
                num(t[5], 't').map_err(what)?,
            );
            let (a, f) = (
                at(0).ok_or_else(|| what("unlocated site".into()))?,
                at(1).ok_or_else(|| what("unlocated fill".into()))?,
            );
            ctx.fill_site(h, w, k as u32, l as u32, s, a, f)
                .map_err(what)?;
            rep.fill_sites += 1;
        } else {
            let nn = num(t[2], 'n').map_err(what)?;
            let stores: Vec<u64> = t[4..]
                .iter()
                .map(|x| num(x, 's'))
                .collect::<Result<_, _>>()
                .map_err(what)?;
            if g.len() != 1 + stores.len() {
                return Err(what(format!(
                    "{} anchors for {} events",
                    g.len(),
                    1 + stores.len()
                )));
            }
            let a = at(0).ok_or_else(|| what("unlocated site".into()))?;
            let paired: Vec<(u64, Option<At>)> = stores
                .iter()
                .enumerate()
                .map(|(k, &i)| (i, at(k + 1)))
                .collect();
            rep.init_stores += ctx.fixed_site(h, w, nn, a, &paired).map_err(what)?;
            rep.init_sites += 1;
        }
    }
    Ok(())
}

/// Check the bounds witness `bounds` against its anchors `anch`.
pub fn check_bounds(
    bodies: &[Body<'_>],
    bounds: &str,
    anch: &str,
    rep: &mut Report,
) -> Result<(), String> {
    let ctx = Ctx {
        bodies,
        alloc: u32::MAX,
    };
    if bounds.trim().is_empty() && anch.trim().is_empty() {
        return Ok(());
    }
    let (st, an): (Vec<&str>, Vec<&str>) = (bounds.split(';').collect(), anch.split(';').collect());
    if st.len() != an.len() {
        return Err(format!(
            "bounds: {} stretches, {} anchor groups",
            st.len(),
            an.len()
        ));
    }
    for (n, (s, g)) in st.iter().zip(&an).enumerate() {
        let t: Vec<&str> = s.split_whitespace().collect();
        let g: Vec<&str> = g.split_whitespace().collect();
        let what = |e: String| format!("bounds stretch {n}: {e}");
        if t.len() != g.len() {
            return Err(what(format!("{} tokens, {} anchors", t.len(), g.len())));
        }
        // Per slot, its located events in byte order (a slot's events are
        // one access's, in one function; slots are independent in the
        // checker, so their mutual order is not a claim).
        let mut last: std::collections::HashMap<String, At> = std::collections::HashMap::new();
        for (tok, a) in t.iter().zip(&g) {
            let kind = tok.chars().next().unwrap_or('?');
            let at = parse_at(a).map_err(what)?;
            match (kind, at) {
                ('s', None) | ('x', None) => continue,
                ('s' | 'x', Some(_)) => return Err(what(format!("`{tok}` carries an anchor"))),
                ('c' | 'a' | 'w', None) => {
                    rep.bounds_unlocated += 1;
                    continue;
                }
                ('c' | 'a' | 'w', Some(at)) => {
                    if let Some(&p) = last.get(&tok[1..]) {
                        if p.func != at.func || at.op <= p.op {
                            return Err(what(format!("`{tok}` is not after its slot's earlier events in the same function")));
                        }
                    }
                    last.insert(tok[1..].to_string(), at);
                    match kind {
                        'c' => {
                            ctx.check_guard(at)
                                .map_err(|e| what(format!("`{tok}`: {e}")))?;
                            rep.bounds_checks += 1;
                        }
                        _ => {
                            ctx.access(at, kind == 'w')
                                .map_err(|e| what(format!("`{tok}`: {e}")))?;
                            rep.bounds_accesses += 1;
                        }
                    }
                }
                _ => return Err(what(format!("unknown token `{tok}`"))),
            }
        }
    }
    Ok(())
}

/// The witness of `property` about `(program)` in a certificate bundle.
pub fn bundle_witness(bundle: &[u8], property: &str) -> Result<String, String> {
    let mut i = bundle
        .iter()
        .position(|&b| b == b'\n')
        .ok_or("bundle: no header")?
        + 1;
    while i < bundle.len() {
        let j = i + bundle[i..]
            .iter()
            .position(|&b| b == b'\n')
            .ok_or("bundle: unterminated line")?;
        let line = std::str::from_utf8(&bundle[i..j]).map_err(|e| e.to_string())?;
        i = j + 1;
        if let Some(rest) = line.strip_prefix("witness ") {
            let mut parts = rest.splitn(3, ' ');
            let (p, n, f) = (
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
                parts.next().unwrap_or(""),
            );
            let n: usize = n
                .parse()
                .map_err(|_| format!("bundle: bad length in `{line}`"))?;
            let w = bundle.get(i..i + n).ok_or("bundle: witness past the end")?;
            if p == property && f == "(program)" {
                return String::from_utf8(w.to_vec()).map_err(|e| e.to_string());
            }
            i += n + 1;
        }
    }
    Ok(String::new())
}

/// The anchor file: `(alloc, init anchors, bounds anchors)`.
pub fn parse_anchors(text: &str) -> Result<(u32, String, String), String> {
    let mut lines = text.lines();
    if lines.next() != Some("almide-anchors 1") {
        return Err("anchors: not an `almide-anchors 1` file".into());
    }
    let mut field = |key: &str| -> Result<String, String> {
        let l = lines
            .next()
            .ok_or_else(|| format!("anchors: no `{key}` line"))?;
        if l == key {
            return Ok(String::new());
        }
        l.strip_prefix(&format!("{key} "))
            .map(str::to_string)
            .ok_or_else(|| format!("anchors: expected `{key}`, read `{l}`"))
    };
    // `-`: the module has no `$alloc` (nothing allocates), and no site can
    // name one.
    let alloc = match field("alloc")?.as_str() {
        "-" => u32::MAX,
        x => x
            .parse()
            .map_err(|_| "anchors: bad alloc index".to_string())?,
    };
    Ok((alloc, field("init")?, field("bounds")?))
}

/// The whole check: the shipped bytes, the bundle's two witnesses and the
/// anchor file (whose `alloc` is a function index, as a `call` names it).
pub fn check(wasm: &[u8], bundle: &[u8], anchors: &str) -> Result<Report, String> {
    let bodies = decode(wasm)?;
    let (alloc, ia, ba) = parse_anchors(anchors)?;
    let mut rep = Report::default();
    check_init(
        &bodies,
        alloc,
        &bundle_witness(bundle, "init")?,
        &ia,
        &mut rep,
    )?;
    check_bounds(&bodies, &bundle_witness(bundle, "bounds")?, &ba, &mut rep)?;
    Ok(rep)
}
