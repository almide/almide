//! The bounds witness against the shipped bytes (lib.rs, BOUNDS): each
//! located check is its compare-and-abort, each access its load or store,
//! and each access is held to its check's operands (operands.rs).

use std::collections::HashMap;

use wasmparser::{BlockType, Operator as Op};

use crate::decode::{is_load, store_width};
use crate::{operands, parse_at, At, Body, Ctx, Report};

impl<'a> Ctx<'_, 'a> {
    /// A `c`'s guard; its compare's index. Two shapes, each a compare whose
    /// TRUE side never reaches the access:
    /// - the compare, `if` (empty), `i32.const` — anchored at the const —
    ///   and that `if` has no `else` and ends in `unreachable` (the abort);
    /// - the surface's own test (#4281): the compare, then an `if` — anchored
    ///   at the `if` — that has an `else`; the access lies in that `else`
    ///   (operands.rs `covers` holds it there), so no second test exists.
    fn check_guard(&self, at: At) -> Result<usize, String> {
        let b = self.body(at)?;
        // A fresh block's own size (#4281): `i32.const N; call $alloc`,
        // anchored at the call — the access below reads inside its N bytes.
        if let Op::Call { function_index } = b.ops[at.op] {
            if self.alloc == u32::MAX || function_index != self.alloc {
                return Err(format!("c: anchored at a call of {function_index}, not $alloc"));
            }
            if at.op == 0 || !matches!(b.ops[at.op - 1], Op::I32Const { .. }) {
                return Err("c: the allocation's size is not a constant".into());
            }
            return Ok(at.op);
        }
        if at.op >= 1 && matches!(b.ops[at.op], Op::If { .. }) {
            if !matches!(b.ops[at.op - 1], Op::I32GeU | Op::I32Eqz) {
                return Err(format!("c: the `if`'s test is {:?}, not a compare", b.ops[at.op - 1]));
            }
            let d_in = *b.depth.get(at.op + 1).ok_or("c: the test's `if` has no body")?;
            let has_else = (at.op + 1..b.ops.len())
                .find(|&j| b.depth[j] == d_in && matches!(b.ops[j], Op::End | Op::Else))
                .is_some_and(|j| matches!(b.ops[j], Op::Else));
            if !has_else {
                return Err("c: the test's `if` has no `else` to hold the access".into());
            }
            return Ok(at.op - 1);
        }
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
        Ok(at.op - 2)
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

/// The located call sites a bounds check reads: the copy-on-write judges
/// (the anchor file's `judges` line) and the loop HOLDS (#4280, its `holds`
/// line).
#[derive(Clone, Copy, Default)]
pub struct Sites<'s> {
    pub judges: &'s [At],
    pub holds: &'s [At],
}

/// Check the bounds witness `bounds` against its anchors `anch`.
pub fn check_bounds(
    bodies: &[Body<'_>],
    bounds: &str,
    anch: &str,
    judges: &[At],
    rep: &mut Report,
) -> Result<(), String> {
    check_bounds_held(bodies, bounds, anch, judges, &[], rep)
}

/// [`check_bounds`] with the located loop HOLDS (#4280): each a `call` that
/// takes +1 on a list a `for` loop iterates, before the loop.
pub fn check_bounds_held(
    bodies: &[Body<'_>],
    bounds: &str,
    anch: &str,
    judges: &[At],
    holds: &[At],
    rep: &mut Report,
) -> Result<(), String> {
    check_bounds_alloc(bodies, bounds, anch, Sites { judges, holds }, u32::MAX, rep)
}

/// [`check_bounds_held`] knowing `$alloc` (a function index, as a `call`
/// names it): a `c` may then be a fresh block's own allocation (#4281).
pub fn check_bounds_alloc(
    bodies: &[Body<'_>],
    bounds: &str,
    anch: &str,
    sites: Sites<'_>,
    alloc: u32,
    rep: &mut Report,
) -> Result<(), String> {
    let mut bc = Bounds { ctx: Ctx { bodies, alloc }, sites, flows: HashMap::new() };
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
        bc.stretch(s, g, rep).map_err(|e| format!("bounds stretch {n}: {e}"))?;
    }
    Ok(())
}

/// One bounds check's state: the module, its located sites, and per
/// function the flow its operands are read through.
struct Bounds<'b, 's, 'a> {
    ctx: Ctx<'b, 'a>,
    sites: Sites<'s>,
    flows: HashMap<usize, operands::Flow<'b, 'a>>,
}

impl<'b, 'a> Bounds<'b, '_, 'a> {
    /// Function `func`'s flow, built once, with its judges and holds.
    fn flow(&mut self, func: usize) -> &mut operands::Flow<'b, 'a> {
        let (bodies, Sites { judges, holds }) = (self.ctx.bodies, self.sites);
        self.flows.entry(func).or_insert_with(|| {
            let js = judges.iter().filter(|j| j.func == func).map(|j| j.op).collect();
            operands::Flow::new(&bodies[func], js)
                .with_holds(holds.iter().filter(|h| h.func == func).map(|h| h.op).collect())
        })
    }

    /// One stretch: its tokens `s` against its anchors `g`.
    fn stretch(&mut self, s: &str, g: &str, rep: &mut Report) -> Result<(), String> {
        let t: Vec<&str> = s.split_whitespace().collect();
        let g: Vec<&str> = g.split_whitespace().collect();
        if t.len() != g.len() {
            return Err(format!("{} tokens, {} anchors", t.len(), g.len()));
        }
        // Per slot, its located events in byte order (a slot's events are
        // one access's, in one function; slots are independent in the
        // checker, so their mutual order is not a claim).
        let mut last: HashMap<String, At> = HashMap::new();
        // Per slot, its latest located check.
        // Per slot, its latest located check: its anchor and its compare.
        let mut guard: HashMap<String, (At, usize)> = HashMap::new();
        for (tok, a) in t.iter().zip(&g) {
            let kind = tok.chars().next().unwrap_or('?');
            let at = match (kind, parse_at(a)?) {
                ('s', None) | ('x', None) => continue,
                ('s' | 'x', Some(_)) => return Err(format!("`{tok}` carries an anchor")),
                ('c' | 'e' | 'a' | 'w', None) => {
                    rep.bounds_unlocated += 1;
                    continue;
                }
                ('c' | 'e' | 'a' | 'w', Some(at)) => at,
                _ => return Err(format!("unknown token `{tok}`")),
            };
            if let Some(&p) = last.get(&tok[1..]) {
                if p.func != at.func || at.op <= p.op {
                    return Err(format!("`{tok}` is not after its slot's earlier events in the same function"));
                }
            }
            last.insert(tok[1..].to_string(), at);
            self.event(tok, at, &mut guard, rep)?;
        }
        Ok(())
    }

    /// The located event `tok` at `at`: a check (`c`), a loop's exit test
    /// (`e`), or an access (`a` / `w`) held to its slot's latest check.
    fn event(&mut self, tok: &str, at: At, guard: &mut HashMap<String, (At, usize)>, rep: &mut Report) -> Result<(), String> {
        let kind = tok.chars().next().unwrap_or('?');
        match kind {
            'c' => {
                let cmp = self.ctx.check_guard(at).map_err(|e| format!("`{tok}`: {e}"))?;
                guard.insert(tok[1..].to_string(), (at, cmp));
                rep.bounds_checks += 1;
            }
            'e' => {
                self.ctx.body(at)?;
                let flow = self.flow(at.func);
                operands::loop_exit(flow, at.op).map_err(|e| format!("`{tok}`: {e}"))?;
                // The exit test's compare: the one before the `br_if`.
                guard.insert(tok[1..].to_string(), (at, at.op - 1));
                rep.bounds_checks += 1;
            }
            _ => {
                self.ctx.access(at, kind == 'w').map_err(|e| format!("`{tok}`: {e}"))?;
                rep.bounds_accesses += 1;
                let (c, cmp) = guard
                    .get(&tok[1..])
                    .copied()
                    .ok_or_else(|| format!("`{tok}` has no located check of its slot"))?;
                let flow = self.flow(at.func);
                operands::covers(flow, cmp, at.op)
                    .map_err(|e| format!("`{tok}` against its check at {}:{}: {e}", c.func, c.op))?;
                rep.bounds_operands += 1;
            }
        }
        Ok(())
    }
}
