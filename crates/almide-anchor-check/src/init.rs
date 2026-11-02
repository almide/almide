//! The init witness's sites against the shipped bytes (lib.rs, INIT): a
//! fixed site's allocation and stores, a loop fill's exact countdown.

use wasmparser::{BlockType, Operator as Op};

use crate::decode::{branch_target, store_width, Body};
use crate::{parse_at, At, Ctx, Report};

/// A fill site's anchor row: header bytes `hdr`, element width `w`, the
/// count local `k`, the counter local `l`, the cursor stride `s`, and the
/// site (`a`) and fill (`f`) anchors.
struct Fill {
    hdr: u64,
    w: u64,
    k: u32,
    l: u32,
    s: u64,
    a: At,
    f: At,
}

impl<'a> Ctx<'_, 'a> {
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

    fn fill_site(&self, site: Fill) -> Result<(), String> {
        let Fill { hdr, w, k, l, s, a, f } = site;
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
                &|o| matches!(o, Op::Block { blockty: BlockType::Empty }),
                &|o| matches!(o, Op::Loop { blockty: BlockType::Empty }),
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
        let stores = Self::fill_body_stores(b, body_from..tail, [bl, c, l, k], w)?;
        if stores != 1 {
            return Err(format!(
                "fill: the loop body holds {stores} stores, not one"
            ));
        }
        Ok(())
    }

    /// The fill loop's body (`body`): no branch, no write to the block,
    /// cursor, counter or count local (`locals`, the cursor second), and
    /// every store `w` bytes wide at offset 0, addressed by the cursor.
    /// Answers how many stores it holds.
    fn fill_body_stores(
        b: &Body,
        body: std::ops::Range<usize>,
        locals: [u32; 4],
        w: u64,
    ) -> Result<usize, String> {
        let (body_from, c) = (body.start, locals[1]);
        let d_body = b.depth[body_from];
        let mut stores = 0;
        for j in body {
            let op = &b.ops[j];
            if branch_target(op, b.depth[j]).is_some() {
                return Err(format!("fill: the loop body branches at instruction {j}"));
            }
            if locals.iter().any(|&x| Self::writes(op, x)) {
                return Err(format!(
                    "fill: the loop body writes the block, cursor or counter at instruction {j}"
                ));
            }
            let Some((sw, off)) = store_width(op) else { continue };
            stores += 1;
            if u64::from(sw) != w || off != 0 || b.depth[j] != d_body {
                return Err(format!("fill: the loop body's store is {op:?}"));
            }
            let by_cursor = b.addr[j].is_some_and(|p| {
                p >= body_from && matches!(b.ops[p], Op::LocalGet { local_index } if local_index == c)
            });
            if !by_cursor {
                return Err("fill: the loop body's store is not addressed by the cursor".into());
            }
        }
        Ok(stores)
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
            ctx.fill_site(Fill { hdr: h, w, k: k as u32, l: l as u32, s, a, f })
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
