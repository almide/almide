//! The linear certificate's TRANSLATION (#4005 A4): the shipped bytes of a
//! self-host body, read as a RoutineLin program (proofs/RoutineLin.v).
//!
//! The proven checker (`RoutineLinCert.check_lbounds_cert`) judges a table
//! of procedures written in RoutineLin's language. That such a procedure is
//! the function the module contains is this file's job, and it is TRUSTED:
//! it reads each function's instructions itself (wasmparser, no compiler
//! code) and writes the program those instructions are, with a HOLE
//! wherever the certificate supplies something the bytes do not say — a
//! Farkas proof, a loop invariant, a contract, the region an access is
//! proven inside. [`template`] is that program as text with holes; a
//! certificate's procedure is accepted by [`verify`] only when it is the
//! template with each hole filled by text of the hole's kind, so a
//! certificate cannot describe instructions the bytes do not contain. The
//! untrusted prover fills the holes (crates/almide-lin-prove).
//!
//! What the translation states, and so what is trusted:
//! - a slot is the wasm local of that index (params first); a value the
//!   translation cannot name linearly gets a fresh slot past the locals,
//!   bound with `s` (any value) or `r` (a value the instruction bounds: a
//!   zero-extending load, an `and` with a non-negative constant, an
//!   unsigned shift right, a compare);
//! - an i64 add, subtract or multiply by a constant is the exact linear
//!   value wrapped to 64 bits; an i32 address is that value mod 2^32, plus
//!   the memarg offset;
//! - each prim load or store the anchor file's `prims` line locates is an
//!   `A` / `W` event of its width at its address; no other instruction is
//!   an event (non-prim accesses are other witnesses' business);
//! - a heap-typed parameter (the `selfhost` line) holds a block's base:
//!   `h p r` at entry; a load of its len word at +4 is a count of that
//!   block at the parameter's stride (`n`, the header invariant) when the
//!   line names a stride;
//! - `call $alloc` returns a fresh block of at least the size passed;
//! - an `i32.const` that is the base of a block in the static block chain
//!   of an active data segment (headers `[1][len][cap]`, `len == cap`,
//!   back to back from the segment's start) is that block (`g`);
//! - a call of a function in the certified table is that procedure's call;
//!   any other call returns an unknown value and changes no local;
//! - control: `if` on a compare is `T` (else `B`); a `br` to the innermost
//!   loop is `c` (its back edge), to the `block` wrapping it `k`, to the
//!   function `R`; `unreachable` is `x`. Any other shape is refused (the
//!   body is not translated, and its prim sites are counted).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use wasmparser::{BlockType, Operator as Op, Parser, Payload, ValType};

/// wasmparser's operator, for the prover's reading of call sites.
pub use wasmparser::Operator as WasmOp;

pub type Slot = u32;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum V {
    S(Slot),
    B(Slot),
    C(Slot),
}

/// `Σ a·v + c`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lin {
    pub terms: Vec<(V, i128)>,
    pub cst: i128,
}

impl Lin {
    pub fn konst(c: i128) -> Lin {
        Lin { terms: Vec::new(), cst: c }
    }
    pub fn var(v: V) -> Lin {
        Lin { terms: vec![(v, 1)], cst: 0 }
    }
    pub fn slot(x: Slot) -> Lin {
        Lin::var(V::S(x))
    }
    fn norm(mut self) -> Lin {
        let mut m: BTreeMap<V, i128> = BTreeMap::new();
        for (v, a) in self.terms.drain(..) {
            *m.entry(v).or_insert(0) += a;
        }
        self.terms = m.into_iter().filter(|(_, a)| *a != 0).collect();
        self
    }
    fn add(&self, o: &Lin) -> Option<Lin> {
        let mut t = self.terms.clone();
        t.extend(o.terms.iter().cloned());
        Some(Lin { terms: t, cst: self.cst.checked_add(o.cst)? }.norm())
    }
    fn scale(&self, k: i128) -> Option<Lin> {
        let mut terms = Vec::with_capacity(self.terms.len());
        for (v, a) in &self.terms {
            terms.push((v.clone(), a.checked_mul(k)?));
        }
        Some(Lin { terms, cst: self.cst.checked_mul(k)? }.norm())
    }
    fn sub(&self, o: &Lin) -> Option<Lin> {
        self.add(&o.scale(-1)?)
    }
    fn as_const(&self) -> Option<i128> {
        self.terms.is_empty().then_some(self.cst)
    }
    pub fn mentions(&self, x: Slot) -> bool {
        self.terms.iter().any(|(v, _)| *v == V::S(x))
    }
    /// The certificate text: `[s3:1 b7:-1]12`.
    pub fn text(&self) -> String {
        let t: Vec<String> = self
            .terms
            .iter()
            .map(|(v, a)| match v {
                V::S(x) => format!("s{x}:{a}"),
                V::B(x) => format!("b{x}:{a}"),
                V::C(x) => format!("c{x}:{a}"),
            })
            .collect();
        format!("[{}]{}", t.join(" "), self.cst)
    }
}

/// One translated item. `Acc.site` is the instruction's index in its body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum It {
    Acc { store: bool, w: u32, off: u64, addr: Lin, site: usize },
    Handle { h: Slot, r: Slot },
    Len { n: Slot, r: Slot, s: i128 },
    Alloc { h: Slot, r: Slot, sz: Lin },
    Wf(Slot),
    Static { r: Slot, b0: i128, c0: i128 },
    Set(Slot),
    SetR { x: Slot, lo: i128, hi: i128 },
    Asn { x: Slot, l: Lin },
    Test { a: Lin, b: Lin, sgn: bool, thn: Vec<It>, els: Vec<It> },
    Branch { thn: Vec<It>, els: Vec<It> },
    Loop { body: Vec<It> },
    Cont,
    Brk,
    Trap,
    Call { f: usize, args: Vec<(Slot, Lin)>, res: Slot },
    Ret,
}

/// A translated function: its result slot and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skel {
    pub rslot: Slot,
    pub body: Vec<It>,
    /// The prim sites the body's events cover, in order.
    pub sites: Vec<usize>,
}

// ── The module, as the translation reads it ───────────────────────────────

#[derive(Default)]
pub struct Module<'a> {
    pub imports: u32,
    types: Vec<(Vec<ValType>, Vec<ValType>)>,
    func_types: Vec<u32>,
    import_types: Vec<u32>,
    locals: Vec<Vec<ValType>>,
    pub bodies: Vec<crate::Body<'a>>,
    /// Static block bases and capacities (the data segments' chains).
    statics: BTreeMap<u32, u32>,
}

impl<'a> Module<'a> {
    pub fn read(bytes: &'a [u8]) -> Result<Module<'a>, String> {
        let mut m = Module { bodies: crate::decode(bytes)?, ..Module::default() };
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.map_err(|e| e.to_string())? {
                Payload::TypeSection(r) => {
                    for ty in r.into_iter_err_on_gc_types() {
                        let f = ty.map_err(|e| e.to_string())?;
                        m.types.push((f.params().to_vec(), f.results().to_vec()));
                    }
                }
                Payload::ImportSection(r) => {
                    for imp in r.into_imports() {
                        if let wasmparser::TypeRef::Func(t) = imp.map_err(|e| e.to_string())?.ty {
                            m.import_types.push(t);
                        }
                    }
                }
                Payload::FunctionSection(r) => {
                    m.func_types = r.into_iter().collect::<Result<_, _>>().map_err(|e| e.to_string())?;
                }
                Payload::CodeSectionEntry(body) => {
                    let mut ls = Vec::new();
                    for l in body.get_locals_reader().map_err(|e| e.to_string())? {
                        let (n, ty) = l.map_err(|e| e.to_string())?;
                        ls.extend(std::iter::repeat_n(ty, n as usize));
                    }
                    m.locals.push(ls);
                }
                Payload::DataSection(r) => {
                    for d in r {
                        let d = d.map_err(|e| e.to_string())?;
                        if let wasmparser::DataKind::Active { offset_expr, .. } = d.kind {
                            let mut ops = offset_expr.get_operators_reader();
                            if let Ok(Op::I32Const { value }) = ops.read() {
                                m.chain(value as u32, d.data);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        m.imports = m.import_types.len() as u32;
        Ok(m)
    }

    /// The static block chain of a segment at `base`: headers `[1][n][n]`
    /// back to back from its first byte, each block wholly inside it.
    fn chain(&mut self, base: u32, data: &[u8]) {
        let word = |at: usize| -> Option<u32> { Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?)) };
        let mut at = 0usize;
        while let (Some(rc), Some(len), Some(cap)) = (word(at), word(at + 4), word(at + 8)) {
            let end = at + 12 + cap as usize;
            if rc != 1 || len != cap || end > data.len() {
                break;
            }
            self.statics.insert(base + at as u32, cap);
            at = end;
        }
    }

    /// The type of function index `f` (imports first).
    fn ftype(&self, f: u32) -> Option<&(Vec<ValType>, Vec<ValType>)> {
        let t = if f < self.imports {
            *self.import_types.get(f as usize)?
        } else {
            *self.func_types.get((f - self.imports) as usize)?
        };
        self.types.get(t as usize)
    }
}

/// The anchor file's `selfhost` line: position -> heap params (param ->
/// optional stride).
pub fn selfhost_params(anchors: &str) -> BTreeMap<usize, BTreeMap<u32, Option<i128>>> {
    let line = anchors
        .lines()
        .find_map(|l| if l == "selfhost" { Some("") } else { l.strip_prefix("selfhost ") })
        .unwrap_or("");
    let mut out = BTreeMap::new();
    for t in line.split_whitespace() {
        let Some((p, rest)) = t.split_once(':') else { continue };
        let Ok(pos) = p.parse::<usize>() else { continue };
        // `params[:name]`: the name is for reports only.
        let rest = rest.split_once(':').map_or(rest, |(ps, _)| ps);
        let mut ps = BTreeMap::new();
        if rest != "-" {
            for q in rest.split(',') {
                match q.split_once('=') {
                    Some((i, s)) => {
                        if let (Ok(i), Ok(s)) = (i.parse(), s.parse()) {
                            ps.insert(i, Some(s));
                        }
                    }
                    None => {
                        if let Ok(i) = q.parse() {
                            ps.insert(i, None);
                        }
                    }
                }
            }
        }
        out.insert(pos, ps);
    }
    out
}

/// The anchor file's `selfhost` names, per position (reports only).
pub fn selfhost_names(anchors: &str) -> BTreeMap<usize, String> {
    let line = anchors.lines().find_map(|l| l.strip_prefix("selfhost ")).unwrap_or("");
    line.split_whitespace()
        .filter_map(|t| {
            let mut it = t.splitn(3, ':');
            let pos = it.next()?.parse().ok()?;
            let _ = it.next()?;
            Some((pos, it.next().unwrap_or("?").to_string()))
        })
        .collect()
}

/// The anchor file's located prim sites, per function position.
pub fn prim_sites(anchors: &str) -> Result<BTreeMap<usize, BTreeSet<usize>>, String> {
    let mut out: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for t in crate::prims_anchors(anchors).split_whitespace() {
        if let Some(at) = crate::parse_at(t)? {
            out.entry(at.func).or_default().insert(at.op);
        }
    }
    Ok(out)
}

// ── Control structure ─────────────────────────────────────────────────────

#[derive(Debug)]
enum Node {
    Op(usize),
    Block { bt: BlockType, body: Vec<Node> },
    Loop { bt: BlockType, body: Vec<Node> },
    If { bt: BlockType, thn: Vec<Node>, els: Vec<Node> },
}

/// The body's instructions as a tree, the final `end` dropped.
fn tree(ops: &[Op<'_>]) -> Result<Vec<Node>, String> {
    fn seq(ops: &[Op<'_>], i: &mut usize, stop_else: bool) -> Result<(Vec<Node>, bool), String> {
        let mut out = Vec::new();
        while *i < ops.len() {
            let k = *i;
            *i += 1;
            match &ops[k] {
                Op::End => return Ok((out, false)),
                Op::Else if stop_else => return Ok((out, true)),
                Op::Block { blockty } => {
                    let (body, _) = seq(ops, i, false)?;
                    out.push(Node::Block { bt: *blockty, body });
                }
                Op::Loop { blockty } => {
                    let (body, _) = seq(ops, i, false)?;
                    out.push(Node::Loop { bt: *blockty, body });
                }
                Op::If { blockty } => {
                    let (thn, had_else) = seq(ops, i, true)?;
                    let els = if had_else { seq(ops, i, false)?.0 } else { Vec::new() };
                    out.push(Node::If { bt: *blockty, thn, els });
                }
                Op::Else | Op::Try { .. } | Op::TryTable { .. } | Op::Catch { .. } | Op::Delegate { .. } => {
                    return Err(format!("unsupported control at {k}"));
                }
                _ => out.push(Node::Op(k)),
            }
        }
        Ok((out, false))
    }
    let mut i = 0;
    let (body, _) = seq(ops, &mut i, false)?;
    Ok(body)
}

// ── The translation ───────────────────────────────────────────────────────

/// A value on the operand stack.
#[derive(Clone, Debug)]
enum Av {
    /// An i64: the linear value wrapped to 64 bits.
    I64(Lin),
    /// An i32: the linear value mod 2^32; `exact` when it is a single i32
    /// slot or a constant in [0, 2^32), so also its value as an integer.
    I32(Lin, bool),
    /// An i32 0/1 compare of two values (`a op b`), not yet consumed.
    Cmp(Cmp),
    /// A float or reference: no slot, no fact.
    Other,
}

#[derive(Clone, Debug)]
struct Cmp {
    /// a >= b (`ge`), or a == b (`eq`), negated by `neg`.
    a: Lin,
    b: Lin,
    sgn: bool,
    eq: bool,
    neg: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Label {
    Func,
    /// A `block` wrapping exactly one loop: `br` to it leaves the loop
    /// (with the block's result slot, when it has one).
    LoopExit(Option<Slot>),
    Loop,
    /// An `if` arm or a plain `block`: no `br` may target it.
    Other,
}

struct Tr<'m, 'a> {
    m: &'m Module<'a>,
    ops: &'m [Op<'a>],
    sites: &'m BTreeSet<usize>,
    table: &'m BTreeSet<usize>,
    alloc: u32,
    /// The wasm type of every slot (locals, then fresh).
    tys: Vec<ValType>,
    /// Slot -> the region whose base it holds (cleared on write).
    base_of: HashMap<Slot, Slot>,
    /// Region -> the len stride of its block (heap params only).
    stride: HashMap<Slot, i128>,
    regions: Vec<Slot>,
    next_region: Slot,
    labels: Vec<Label>,
    rslot: Slot,
    result: Option<ValType>,
    covered: Vec<usize>,
}

const REGION0: Slot = 1000;

impl<'m, 'a> Tr<'m, 'a> {
    fn fresh(&mut self, ty: ValType) -> Slot {
        self.tys.push(ty);
        (self.tys.len() - 1) as Slot
    }

    fn region(&mut self) -> Slot {
        let r = self.next_region;
        self.next_region += 1;
        self.regions.push(r);
        r
    }

    fn written(&mut self, x: Slot) {
        self.base_of.remove(&x);
    }

    /// The item binding slot x to value v (x is written).
    fn assign(&mut self, out: &mut Vec<It>, x: Slot, v: Av) {
        let ty = self.tys[x as usize];
        let base = match &v {
            Av::I64(l) | Av::I32(l, true) => l.terms.first().filter(|_| l.terms.len() == 1 && l.cst == 0).and_then(|(var, a)| match var {
                V::S(y) if *a == 1 => self.base_of.get(y).copied(),
                _ => None,
            }),
            _ => None,
        };
        match (ty, v) {
            (ValType::I64, Av::I64(l)) | (ValType::I32, Av::I32(l, true)) => {
                if l != Lin::slot(x) {
                    out.push(It::Asn { x, l });
                }
            }
            (_, Av::Cmp(_)) => out.push(It::SetR { x, lo: 0, hi: 1 }),
            (ValType::I32, Av::I32(_, false)) => out.push(It::SetR { x, lo: 0, hi: (1 << 32) - 1 }),
            _ => out.push(It::Set(x)),
        }
        self.written(x);
        if let Some(r) = base {
            self.base_of.insert(x, r);
        }
    }

    /// A fresh slot holding `v`, as a stack value.
    fn bind(&mut self, out: &mut Vec<It>, v: Av, ty: ValType) -> Av {
        let t = self.fresh(ty);
        self.assign(out, t, v);
        match ty {
            ValType::I64 => Av::I64(Lin::slot(t)),
            ValType::I32 => Av::I32(Lin::slot(t), true),
            _ => Av::Other,
        }
    }

    fn bind_range(&mut self, out: &mut Vec<It>, ty: ValType, lo: i128, hi: i128) -> Av {
        let t = self.fresh(ty);
        out.push(It::SetR { x: t, lo, hi });
        match ty {
            ValType::I64 => Av::I64(Lin::slot(t)),
            _ => Av::I32(Lin::slot(t), true),
        }
    }

    fn unknown(&mut self, out: &mut Vec<It>, ty: ValType) -> Av {
        match ty {
            ValType::I64 | ValType::I32 => {
                let t = self.fresh(ty);
                out.push(It::Set(t));
                if ty == ValType::I64 { Av::I64(Lin::slot(t)) } else { Av::I32(Lin::slot(t), true) }
            }
            _ => Av::Other,
        }
    }

    /// A value as a linear i64 (an exact i32 is also its integer value).
    fn as_i64(&mut self, out: &mut Vec<It>, v: Av) -> Lin {
        match v {
            Av::I64(l) | Av::I32(l, true) => l,
            other => match self.bind(out, other, ValType::I64) {
                Av::I64(l) => l,
                _ => unreachable!(),
            },
        }
    }

    /// The stack's values that name a slot `ws` holds: moved to fresh slots
    /// (the construct about to run writes them).
    fn spill(&mut self, out: &mut Vec<It>, stack: &mut [Av], ws: &BTreeSet<Slot>) {
        for v in stack.iter_mut() {
            let hit = |l: &Lin| l.terms.iter().any(|(x, _)| matches!(x, V::S(s) if ws.contains(s)));
            let touched = match v {
                Av::I64(l) | Av::I32(l, _) => hit(l),
                Av::Cmp(c) => hit(&c.a) || hit(&c.b),
                Av::Other => false,
            };
            if touched {
                *v = match v.clone() {
                    Av::I64(l) => self.bind(out, Av::I64(l), ValType::I64),
                    Av::I32(l, e) => self.bind(out, Av::I32(l, e), ValType::I32),
                    c @ Av::Cmp(_) => self.bind(out, c, ValType::I32),
                    Av::Other => Av::Other,
                };
            }
        }
    }

    fn block_result(&self, bt: BlockType) -> Result<Option<ValType>, String> {
        match bt {
            BlockType::Empty => Ok(None),
            BlockType::Type(t) => Ok(Some(t)),
            BlockType::FuncType(_) => Err("multi-value block".into()),
        }
    }

    /// Translate a sequence. `Ok(true)` when it ended in a jump (the rest
    /// is unreachable and dropped).
    fn seq(&mut self, nodes: &[Node], out: &mut Vec<It>, stack: &mut Vec<Av>, res: Option<(Slot, ValType)>) -> Result<bool, String> {
        for n in nodes {
            if self.node(n, out, stack)? {
                return Ok(true);
            }
        }
        // Fall through the construct: its result into its slot.
        if let Some((t, _)) = res {
            let v = stack.pop().ok_or("empty stack at a result")?;
            self.assign(out, t, v);
        }
        Ok(false)
    }

    fn label_at(&self, depth: u32) -> Result<(Label, usize), String> {
        let i = self.labels.len().checked_sub(1 + depth as usize).ok_or("branch past the function")?;
        Ok((self.labels[i], i))
    }

    /// The items a branch to `depth` is. A branch carrying a value (the
    /// function's result) assigns it first.
    fn jump(&mut self, depth: u32, out: &mut Vec<It>, stack: &mut Vec<Av>) -> Result<(), String> {
        let (l, i) = self.label_at(depth)?;
        // No loop may sit between the branch and its target loop/exit.
        let inner_loops = self.labels[i + 1..].iter().filter(|x| **x == Label::Loop).count();
        match l {
            Label::Func => {
                self.ret(out, stack)?;
            }
            Label::Loop if inner_loops == 0 => out.push(It::Cont),
            Label::LoopExit(res) if inner_loops == 1 && self.labels.get(i + 1) == Some(&Label::Loop) => {
                if let Some(t) = res {
                    let v = stack.pop().ok_or("a branch with no value for its block")?;
                    self.assign(out, t, v);
                }
                out.push(It::Brk);
            }
            _ => return Err(format!("branch to an unsupported label ({l:?})")),
        }
        Ok(())
    }

    fn ret(&mut self, out: &mut Vec<It>, stack: &mut Vec<Av>) -> Result<(), String> {
        if self.result.is_some() {
            let v = stack.pop().ok_or("return with an empty stack")?;
            let r = self.rslot;
            self.assign(out, r, v);
        }
        out.push(It::Ret);
        Ok(())
    }

    /// The test `c` as items around two arms.
    fn test(c: Cmp, thn: Vec<It>, els: Vec<It>) -> It {
        let (thn, els) = if c.neg { (els, thn) } else { (thn, els) };
        if c.eq {
            // a == b: a >= b and b >= a.
            let inner = It::Test { a: c.b.clone(), b: c.a.clone(), sgn: c.sgn, thn, els: els.clone() };
            It::Test { a: c.a, b: c.b, sgn: c.sgn, thn: vec![inner], els }
        } else {
            It::Test { a: c.a, b: c.b, sgn: c.sgn, thn, els }
        }
    }

    fn cond(&mut self, v: Av) -> Option<Cmp> {
        match v {
            Av::Cmp(c) => Some(c),
            // A value as a condition: v != 0.
            Av::I32(l, true) => Some(Cmp { a: l, b: Lin::konst(0), sgn: true, eq: true, neg: true }),
            _ => None,
        }
    }

    fn node(&mut self, n: &Node, out: &mut Vec<It>, stack: &mut Vec<Av>) -> Result<bool, String> {
        match n {
            Node::Op(k) => self.op(*k, out, stack),
            Node::Block { bt, body } => {
                let res = self.block_result(*bt)?;
                let ws = self.writes(body);
                self.spill(out, stack, &ws);
                let rs = res.map(|t| (self.fresh(t), t));
                let is_exit = matches!(body.as_slice(), [Node::Loop { .. }]);
                self.labels.push(if is_exit { Label::LoopExit(rs.map(|r| r.0)) } else { Label::Other });
                let mut inner = Vec::new();
                let jumped = self.seq(body, out, &mut inner, rs)?;
                self.labels.pop();
                if let Some((t, ty)) = rs {
                    stack.push(if ty == ValType::I64 { Av::I64(Lin::slot(t)) } else { Av::I32(Lin::slot(t), true) });
                }
                let _ = jumped;
                Ok(jumped && !is_exit)
            }
            Node::Loop { bt, body } => {
                let res = self.block_result(*bt)?;
                let ws = self.writes(body);
                self.spill(out, stack, &ws);
                for x in &ws {
                    self.written(*x);
                }
                let rs = res.map(|t| (self.fresh(t), t));
                self.labels.push(Label::Loop);
                let mut b = Vec::new();
                for r in self.regions.clone() {
                    b.push(It::Wf(r));
                }
                let mut inner = Vec::new();
                self.seq(body, &mut b, &mut inner, rs)?;
                self.labels.pop();
                // After the loop, a slot it writes holds any iteration's value.
                for x in &ws {
                    self.written(*x);
                }
                out.push(It::Loop { body: b });
                if let Some((t, ty)) = rs {
                    stack.push(if ty == ValType::I64 { Av::I64(Lin::slot(t)) } else { Av::I32(Lin::slot(t), true) });
                }
                Ok(false)
            }
            Node::If { bt, thn, els } => {
                let res = self.block_result(*bt)?;
                let c = stack.pop().ok_or("if with an empty stack")?;
                let c = self.cond(c);
                let mut ws = self.writes(thn);
                ws.extend(self.writes(els));
                self.spill(out, stack, &ws);
                let rs = res.map(|t| (self.fresh(t), t));
                let saved = self.base_of.clone();
                self.labels.push(Label::Other);
                let (mut ti, mut ei) = (Vec::new(), Vec::new());
                let tj = self.seq(thn, &mut ti, &mut Vec::new(), rs)?;
                let after_t = std::mem::replace(&mut self.base_of, saved.clone());
                let ej = self.seq(els, &mut ei, &mut Vec::new(), rs)?;
                let after_e = std::mem::take(&mut self.base_of);
                self.labels.pop();
                // A base survives the join when both arms agree on it.
                self.base_of = after_t.into_iter().filter(|(k, v)| after_e.get(k) == Some(v)).collect();
                for x in &ws {
                    if !saved.contains_key(x) {
                        self.base_of.remove(x);
                    }
                }
                out.push(match c {
                    Some(c) => Self::test(c, ti, ei),
                    None => It::Branch { thn: ti, els: ei },
                });
                if let Some((t, ty)) = rs {
                    stack.push(if ty == ValType::I64 { Av::I64(Lin::slot(t)) } else { Av::I32(Lin::slot(t), true) });
                }
                Ok(tj && ej)
            }
        }
    }

    /// Every local a subtree writes.
    fn writes(&self, nodes: &[Node]) -> BTreeSet<Slot> {
        let mut out = BTreeSet::new();
        fn walk(ops: &[Op<'_>], nodes: &[Node], out: &mut BTreeSet<Slot>) {
            for n in nodes {
                match n {
                    Node::Op(k) => {
                        if let Op::LocalSet { local_index } | Op::LocalTee { local_index } = ops[*k] {
                            out.insert(local_index);
                        }
                    }
                    Node::Block { body, .. } | Node::Loop { body, .. } => walk(ops, body, out),
                    Node::If { thn, els, .. } => {
                        walk(ops, thn, out);
                        walk(ops, els, out);
                    }
                }
            }
        }
        walk(self.ops, nodes, &mut out);
        out
    }

    fn pop(&mut self, stack: &mut Vec<Av>) -> Result<Av, String> {
        stack.pop().ok_or_else(|| "operand stack underflow".to_string())
    }

    fn binop64(&mut self, out: &mut Vec<It>, stack: &mut Vec<Av>, f: impl Fn(&Lin, &Lin) -> Option<Lin>) -> Result<(), String> {
        let b = self.pop(stack)?;
        let a = self.pop(stack)?;
        let (la, lb) = (self.as_i64(out, a), self.as_i64(out, b));
        let v = match f(&la, &lb) {
            Some(l) => Av::I64(l),
            None => self.unknown(out, ValType::I64),
        };
        stack.push(v);
        Ok(())
    }

    fn cmp(&mut self, out: &mut Vec<It>, stack: &mut Vec<Av>, i32s: bool, sgn: bool, kind: &str) -> Result<(), String> {
        let b = self.pop(stack)?;
        let a = self.pop(stack)?;
        let lin = |me: &mut Self, out: &mut Vec<It>, v: Av| -> Option<Lin> {
            match v {
                Av::I64(l) if !i32s => Some(l),
                Av::I32(l, true) if i32s => Some(l),
                other => {
                    let _ = me.bind(out, other, if i32s { ValType::I32 } else { ValType::I64 });
                    None
                }
            }
        };
        let (Some(la), Some(lb)) = (lin(self, out, a), lin(self, out, b)) else {
            let v = self.bind_range(out, ValType::I32, 0, 1);
            stack.push(v);
            return Ok(());
        };
        let c = match kind {
            "ge" => Cmp { a: la, b: lb, sgn, eq: false, neg: false },
            "lt" => Cmp { a: la, b: lb, sgn, eq: false, neg: true },
            "le" => Cmp { a: lb, b: la, sgn, eq: false, neg: false },
            "gt" => Cmp { a: lb, b: la, sgn, eq: false, neg: true },
            "eq" => Cmp { a: la, b: lb, sgn: true, eq: true, neg: false },
            _ => Cmp { a: la, b: lb, sgn: true, eq: true, neg: true },
        };
        stack.push(Av::Cmp(c));
        Ok(())
    }

    fn access(&mut self, k: usize, out: &mut Vec<It>, stack: &mut Vec<Av>, store: bool, w: u32, off: u64) -> Result<Av, String> {
        let value = if store { Some(self.pop(stack)?) } else { None };
        let a = self.pop(stack)?;
        let addr = match a {
            Av::I32(l, _) => l,
            other => match self.bind(out, other, ValType::I32) {
                Av::I32(l, _) => l,
                _ => return Err("an address that is not an i32".into()),
            },
        };
        let _ = value;
        if self.sites.contains(&k) {
            out.push(It::Acc { store, w, off, addr: addr.clone(), site: k });
            self.covered.push(k);
        }
        // A len word read off a counted heap parameter's base.
        if !store && w == 4 {
            let at = addr.add(&Lin::konst(off as i128));
            let h = at.as_ref().and_then(|at| match at.terms.as_slice() {
                [(V::S(h), 1)] if at.cst == 4 => Some(*h),
                _ => None,
            });
            let rs = h.and_then(|h| self.base_of.get(&h).copied()).and_then(|r| self.stride.get(&r).map(|s| (r, *s)));
            if let Some((r, s)) = rs {
                let n = self.fresh(ValType::I32);
                out.push(It::Len { n, r, s });
                return Ok(Av::I32(Lin::slot(n), true));
            }
        }
        Ok(Av::Other)
    }


    fn call_op(&mut self, op: &Op<'a>, out: &mut Vec<It>, stack: &mut Vec<Av>) -> Result<(), String> {
        let Op::Call { function_index } = op else { return Err("not a call".into()) };
        let f = *function_index;
        let (params, results) = self.m.ftype(f).cloned().ok_or("call of an unknown function")?;
        let mut args = Vec::new();
        for _ in &params {
            args.push(self.pop(stack)?);
        }
        args.reverse();
        let rty = results.first().copied();
        if results.len() > 1 {
            return Err("multi-value call".into());
        }
        let pos = f.checked_sub(self.m.imports).map(|p| p as usize);
        if f == self.alloc {
            let sz = match args.pop() {
                Some(Av::I32(l, _)) => l,
                Some(other) => match self.bind(out, other, ValType::I32) {
                    Av::I32(l, _) => l,
                    _ => return Err("alloc size".into()),
                },
                None => return Err("alloc arity".into()),
            };
            let h = self.fresh(ValType::I32);
            let r = self.region();
            out.push(It::Alloc { h, r, sz });
            out.push(It::Wf(r));
            self.base_of.insert(h, r);
            stack.push(Av::I32(Lin::slot(h), true));
        } else if let Some(p) = pos.filter(|p| self.table.contains(p)) {
            let mut lins = Vec::new();
            for (i, a) in args.into_iter().enumerate() {
                let l = match a {
                    Av::I64(l) => Some(l),
                    Av::I32(l, true) => Some(l),
                    Av::Other => None,
                    other => {
                        let ty = params[i];
                        match self.bind(out, other, ty) {
                            Av::I64(l) | Av::I32(l, _) => Some(l),
                            _ => None,
                        }
                    }
                };
                if let Some(l) = l {
                    lins.push((i as Slot, l));
                }
            }
            let res = self.fresh(rty.unwrap_or(ValType::I64));
            out.push(It::Call { f: p, args: lins, res });
            if let Some(t) = rty {
                stack.push(match t {
                    ValType::I64 => Av::I64(Lin::slot(res)),
                    ValType::I32 => Av::I32(Lin::slot(res), true),
                    _ => Av::Other,
                });
            }
        } else if let Some(t) = rty {
            let v = self.unknown(out, t);
            stack.push(v);
        }
            Ok(())
    }

    fn op(&mut self, k: usize, out: &mut Vec<It>, stack: &mut Vec<Av>) -> Result<bool, String> {
        let ops = self.ops;
        let op = &ops[k];
        match op {
            Op::Nop => {}
            Op::Unreachable => {
                out.push(It::Trap);
                return Ok(true);
            }
            Op::Return => {
                self.ret(out, stack)?;
                return Ok(true);
            }
            Op::Br { relative_depth } => {
                self.jump(*relative_depth, out, stack)?;
                return Ok(true);
            }
            Op::BrIf { relative_depth } => {
                let c = self.pop(stack)?;
                let c = self.cond(c);
                let mut taken = Vec::new();
                let mut st = stack.clone();
                self.jump(*relative_depth, &mut taken, &mut st)?;
                out.push(match c {
                    Some(c) => Self::test(c, taken, Vec::new()),
                    None => It::Branch { thn: taken, els: Vec::new() },
                });
            }
            Op::Drop => {
                self.pop(stack)?;
            }
            Op::Select | Op::TypedSelect { .. } => {
                self.pop(stack)?;
                let b = self.pop(stack)?;
                let _ = self.pop(stack)?;
                let ty = match b {
                    Av::I64(_) => ValType::I64,
                    Av::I32(..) | Av::Cmp(_) => ValType::I32,
                    Av::Other => ValType::F64,
                };
                let v = self.unknown(out, ty);
                stack.push(v);
            }
            Op::LocalGet { local_index } => {
                let x = *local_index;
                stack.push(match self.tys.get(x as usize) {
                    Some(ValType::I64) => Av::I64(Lin::slot(x)),
                    Some(ValType::I32) => Av::I32(Lin::slot(x), true),
                    _ => Av::Other,
                });
            }
            Op::LocalSet { local_index } | Op::LocalTee { local_index } => {
                let x = *local_index;
                let v = self.pop(stack)?;
                // A value still on the stack that names x is x's OLD value:
                // it moves to a fresh slot before x changes.
                self.spill(out, stack, &BTreeSet::from([x]));
                self.assign(out, x, v);
                if matches!(op, Op::LocalTee { .. }) {
                    stack.push(match self.tys[x as usize] {
                        ValType::I64 => Av::I64(Lin::slot(x)),
                        ValType::I32 => Av::I32(Lin::slot(x), true),
                        _ => Av::Other,
                    });
                }
            }
            Op::GlobalGet { .. } | Op::MemorySize { .. } => {
                let v = self.unknown(out, ValType::I32);
                stack.push(v);
            }
            Op::GlobalSet { .. } => {
                self.pop(stack)?;
            }
            Op::I64Const { value } => stack.push(Av::I64(Lin::konst(i128::from(*value)))),
            Op::I32Const { value } => {
                let u = *value as u32;
                if let Some(&cap) = self.m.statics.get(&u) {
                    let r = self.region();
                    out.push(It::Static { r, b0: i128::from(u), c0: i128::from(cap) });
                    out.push(It::Wf(r));
                }
                stack.push(Av::I32(Lin::konst(i128::from(u)), true));
            }
            Op::F64Const { .. } | Op::F32Const { .. } => stack.push(Av::Other),
            Op::I64Add => self.binop64(out, stack, |a, b| a.add(b))?,
            Op::I64Sub => self.binop64(out, stack, |a, b| a.sub(b))?,
            Op::I64Mul => self.binop64(out, stack, |a, b| match (a.as_const(), b.as_const()) {
                (_, Some(k)) => a.scale(k),
                (Some(k), _) => b.scale(k),
                _ => None,
            })?,
            Op::I64And | Op::I64ShrU | Op::I64ShrS => {
                let b = self.pop(stack)?;
                let a = self.pop(stack)?;
                let kb = match &b { Av::I64(l) => l.as_const(), _ => None };
                let ka = match &a { Av::I64(l) => l.as_const(), _ => None };
                let range = match (op, kb, ka) {
                    (Op::I64And, Some(c), _) | (Op::I64And, _, Some(c)) if c >= 0 => Some((0, c)),
                    (Op::I64ShrU, Some(s), _) if (1..64).contains(&(s & 63)) => Some((0, (1i128 << (64 - (s & 63))) - 1)),
                    (Op::I64ShrS, Some(s), _) if (1..64).contains(&(s & 63)) => {
                        let e = 63 - (s & 63);
                        Some((-(1i128 << e), (1i128 << e) - 1))
                    }
                    _ => None,
                };
                let v = match range {
                    Some((lo, hi)) => self.bind_range(out, ValType::I64, lo, hi),
                    None => self.unknown(out, ValType::I64),
                };
                stack.push(v);
            }
            Op::I32Add | Op::I32Sub | Op::I32Mul => {
                let b = self.pop(stack)?;
                let a = self.pop(stack)?;
                let (la, lb) = match (a, b) {
                    (Av::I32(a, _), Av::I32(b, _)) => (a, b),
                    _ => {
                        let v = self.unknown(out, ValType::I32);
                        stack.push(v);
                        return Ok(false);
                    }
                };
                let l = match op {
                    Op::I32Add => la.add(&lb),
                    Op::I32Sub => la.sub(&lb),
                    _ => match (la.as_const(), lb.as_const()) {
                        (_, Some(c)) => la.scale(c),
                        (Some(c), _) => lb.scale(c),
                        _ => None,
                    },
                };
                stack.push(match l {
                    Some(l) => match l.as_const() {
                        Some(c) => Av::I32(Lin::konst(c.rem_euclid(1 << 32)), true),
                        None => Av::I32(l, false),
                    },
                    None => self.unknown(out, ValType::I32),
                });
            }
            Op::I32WrapI64 => {
                let a = self.pop(stack)?;
                stack.push(match a {
                    Av::I64(l) => match l.as_const() {
                        Some(c) => Av::I32(Lin::konst(c.rem_euclid(1 << 32)), true),
                        None => Av::I32(l, false),
                    },
                    other => self.bind(out, other, ValType::I32),
                });
            }
            Op::I64ExtendI32U => {
                let a = self.pop(stack)?;
                stack.push(match a {
                    Av::I32(l, true) => Av::I64(l),
                    Av::I32(_, false) | Av::Cmp(_) => self.bind_range(out, ValType::I64, 0, (1 << 32) - 1),
                    _ => self.unknown(out, ValType::I64),
                });
            }
            Op::I64ExtendI32S => {
                self.pop(stack)?;
                let v = self.bind_range(out, ValType::I64, -(1 << 31), (1 << 31) - 1);
                stack.push(v);
            }
            Op::I64GeS => self.cmp(out, stack, false, true, "ge")?,
            Op::I64GeU => self.cmp(out, stack, false, false, "ge")?,
            Op::I64LtS => self.cmp(out, stack, false, true, "lt")?,
            Op::I64LtU => self.cmp(out, stack, false, false, "lt")?,
            Op::I64LeS => self.cmp(out, stack, false, true, "le")?,
            Op::I64LeU => self.cmp(out, stack, false, false, "le")?,
            Op::I64GtS => self.cmp(out, stack, false, true, "gt")?,
            Op::I64GtU => self.cmp(out, stack, false, false, "gt")?,
            Op::I64Eq => self.cmp(out, stack, false, true, "eq")?,
            Op::I64Ne => self.cmp(out, stack, false, true, "ne")?,
            Op::I32GeU => self.cmp(out, stack, true, false, "ge")?,
            Op::I32LtU => self.cmp(out, stack, true, false, "lt")?,
            Op::I32LeU => self.cmp(out, stack, true, false, "le")?,
            Op::I32GtU => self.cmp(out, stack, true, false, "gt")?,
            Op::I32Eq => self.cmp(out, stack, true, true, "eq")?,
            Op::I32Ne => self.cmp(out, stack, true, true, "ne")?,
            Op::I64Eqz => {
                stack.push(Av::I64(Lin::konst(0)));
                self.cmp(out, stack, false, true, "eq")?;
            }
            Op::I32Eqz => {
                let v = self.pop(stack)?;
                match v {
                    Av::Cmp(mut c) => {
                        c.neg = !c.neg;
                        stack.push(Av::Cmp(c));
                    }
                    other => {
                        stack.push(other);
                        stack.push(Av::I32(Lin::konst(0), true));
                        self.cmp(out, stack, true, true, "eq")?;
                    }
                }
            }
            Op::Call { .. } => self.call_op(op, out, stack)?,
            Op::CallIndirect { type_index, .. } => {
                self.pop(stack)?;
                let (params, results) = self.m.types.get(*type_index as usize).cloned().ok_or("call_indirect type")?;
                for _ in &params {
                    self.pop(stack)?;
                }
                if let Some(t) = results.first().copied() {
                    let v = self.unknown(out, t);
                    stack.push(v);
                }
            }
            Op::ReturnCall { function_index } => {
                // A tail call: the call, then its result returned.
                let call = Op::Call { function_index: *function_index };
                self.call_op(&call, out, stack)?;
                self.ret(out, stack)?;
                return Ok(true);
            }
            Op::BrTable { .. } => return Err("br_table".into()),
            Op::I32Load8U { memarg } | Op::I64Load8U { memarg } => {
                let _ = self.access(k, out, stack, false, 1, memarg.offset)?;
                let ty = if matches!(op, Op::I32Load8U { .. }) { ValType::I32 } else { ValType::I64 };
                let v = self.bind_range(out, ty, 0, 255);
                stack.push(v);
            }
            Op::I32Load16U { memarg } | Op::I64Load16U { memarg } => {
                let _ = self.access(k, out, stack, false, 2, memarg.offset)?;
                let ty = if matches!(op, Op::I32Load16U { .. }) { ValType::I32 } else { ValType::I64 };
                let v = self.bind_range(out, ty, 0, 65535);
                stack.push(v);
            }
            Op::I64Load32U { memarg } => {
                let _ = self.access(k, out, stack, false, 4, memarg.offset)?;
                let v = self.bind_range(out, ValType::I64, 0, (1 << 32) - 1);
                stack.push(v);
            }
            Op::I32Load { memarg } => {
                match self.access(k, out, stack, false, 4, memarg.offset)? {
                    Av::Other => {
                        let v = self.bind_range(out, ValType::I32, 0, (1 << 32) - 1);
                        stack.push(v);
                    }
                    counted => stack.push(counted),
                }
            }
            Op::I32Load8S { memarg } | Op::I64Load8S { memarg } => {
                let _ = self.access(k, out, stack, false, 1, memarg.offset)?;
                let v = self.unknown(out, if matches!(op, Op::I32Load8S { .. }) { ValType::I32 } else { ValType::I64 });
                stack.push(v);
            }
            Op::I32Load16S { memarg } | Op::I64Load16S { memarg } | Op::I64Load32S { memarg } => {
                let w = if matches!(op, Op::I64Load32S { .. }) { 4 } else { 2 };
                let _ = self.access(k, out, stack, false, w, memarg.offset)?;
                let v = self.unknown(out, if matches!(op, Op::I32Load16S { .. }) { ValType::I32 } else { ValType::I64 });
                stack.push(v);
            }
            Op::I64Load { memarg } => {
                let _ = self.access(k, out, stack, false, 8, memarg.offset)?;
                let v = self.unknown(out, ValType::I64);
                stack.push(v);
            }
            Op::F64Load { memarg } => {
                let _ = self.access(k, out, stack, false, 8, memarg.offset)?;
                stack.push(Av::Other);
            }
            Op::F32Load { memarg } => {
                let _ = self.access(k, out, stack, false, 4, memarg.offset)?;
                stack.push(Av::Other);
            }
            op if crate::store_width(op).is_some() => {
                let (w, off) = crate::store_width(op).expect("a store");
                let _ = self.access(k, out, stack, true, w, off)?;
            }
            _ => {
                // Any other instruction: its operands consumed, its results
                // unknown — by its signature, which only the plain numeric
                // instructions below have here.
                let (pops, push) = numeric_arity(op).ok_or_else(|| format!("unsupported instruction {op:?}"))?;
                for _ in 0..pops {
                    self.pop(stack)?;
                }
                if let Some(t) = push {
                    let v = self.unknown(out, t);
                    stack.push(v);
                }
            }
        }
        Ok(false)
    }
}

/// Operand and result shape of the remaining numeric instructions.
fn numeric_arity(op: &Op<'_>) -> Option<(usize, Option<ValType>)> {
    use ValType::*;
    Some(match op {
        Op::I32And | Op::I32Or | Op::I32Xor | Op::I32Shl | Op::I32ShrU | Op::I32ShrS | Op::I32Rotl | Op::I32Rotr
        | Op::I32DivS | Op::I32DivU | Op::I32RemS | Op::I32RemU | Op::I32LtS | Op::I32GtS | Op::I32LeS | Op::I32GeS => (2, Some(I32)),
        Op::I64Or | Op::I64Xor | Op::I64Shl | Op::I64Rotl | Op::I64Rotr | Op::I64DivS | Op::I64DivU | Op::I64RemS | Op::I64RemU => {
            (2, Some(I64))
        }
        Op::I32Clz | Op::I32Ctz | Op::I32Popcnt | Op::I32Extend8S | Op::I32Extend16S => (1, Some(I32)),
        Op::I64Clz | Op::I64Ctz | Op::I64Popcnt | Op::I64Extend8S | Op::I64Extend16S | Op::I64Extend32S => (1, Some(I64)),
        Op::F64Add | Op::F64Sub | Op::F64Mul | Op::F64Div | Op::F64Min | Op::F64Max | Op::F64Copysign => (2, Some(F64)),
        Op::F64Neg | Op::F64Abs | Op::F64Sqrt | Op::F64Ceil | Op::F64Floor | Op::F64Trunc | Op::F64Nearest => (1, Some(F64)),
        Op::F64Eq | Op::F64Ne | Op::F64Lt | Op::F64Gt | Op::F64Le | Op::F64Ge => (2, Some(I32)),
        Op::F64ConvertI64S | Op::F64ConvertI64U | Op::F64ConvertI32S | Op::F64ConvertI32U | Op::F64ReinterpretI64 | Op::F64PromoteF32 => {
            (1, Some(F64))
        }
        Op::I64TruncF64S | Op::I64TruncF64U | Op::I64ReinterpretF64 | Op::I64TruncSatF64S | Op::I64TruncSatF64U => (1, Some(I64)),
        Op::I32TruncF64S | Op::I32TruncF64U | Op::I32TruncSatF64S | Op::I32TruncSatF64U => (1, Some(I32)),
        Op::F32DemoteF64 => (1, Some(F32)),
        _ => return None,
    })
}

/// Translate function position `pos`.
pub fn translate(
    m: &Module<'_>,
    pos: usize,
    sites: &BTreeSet<usize>,
    heap: &BTreeMap<u32, Option<i128>>,
    table: &BTreeSet<usize>,
    alloc: u32,
) -> Result<Skel, String> {
    let body = m.bodies.get(pos).ok_or("no such function")?;
    let (params, results) = m.ftype(m.imports + pos as u32).cloned().ok_or("no type")?;
    let mut tys = params.clone();
    tys.extend(m.locals.get(pos).ok_or("no locals")?.iter().copied());
    let nodes = tree(&body.ops)?;
    let mut t = Tr {
        m,
        ops: &body.ops,
        sites,
        table,
        alloc,
        tys,
        base_of: HashMap::new(),
        stride: HashMap::new(),
        regions: Vec::new(),
        next_region: REGION0,
        labels: vec![Label::Func],
        rslot: 0,
        result: results.first().copied(),
        covered: Vec::new(),
    };
    if results.len() > 1 {
        return Err("multi-value function".into());
    }
    t.rslot = t.fresh(t.result.unwrap_or(ValType::I64));
    let mut out = Vec::new();
    for (&p, &s) in heap {
        if params.get(p as usize) != Some(&ValType::I32) {
            return Err(format!("heap parameter {p} is not an i32"));
        }
        let r = t.region();
        out.push(It::Handle { h: p, r });
        out.push(It::Wf(r));
        t.base_of.insert(p, r);
        if let Some(s) = s {
            t.stride.insert(r, s);
        }
    }
    let mut stack = Vec::new();
    let jumped = t.seq(&nodes, &mut out, &mut stack, None)?;
    if !jumped {
        t.ret(&mut out, &mut stack)?;
    }
    let mut sites_seen = t.covered.clone();
    sites_seen.sort_unstable();
    let want: Vec<usize> = sites.iter().copied().collect();
    if sites_seen != want {
        return Err("a prim site in an unreachable stretch".into());
    }
    Ok(Skel { rslot: t.rslot, body: out, sites: t.covered })
}

// ── Templates: the certificate text with holes ────────────────────────────

/// What a hole holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hole {
    /// One Farkas proof `( … )Z`.
    Pf,
    /// A sequence of proofs.
    Pfs,
    /// A sequence of facts `[ … ]Z`.
    Facts,
    /// A region (a slot number).
    Reg,
    /// A sequence of `N:N` maps.
    Maps,
    /// An optional `! pf pf` after a call argument.
    ArgPf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Lit(String),
    Hole(Hole),
}

fn lit(out: &mut Vec<Tok>, s: &str) {
    if let Some(Tok::Lit(l)) = out.last_mut() {
        l.push_str(s);
    } else {
        out.push(Tok::Lit(s.to_string()));
    }
}

fn hole(out: &mut Vec<Tok>, h: Hole) {
    out.push(Tok::Hole(h));
}

fn items_template(items: &[It], out: &mut Vec<Tok>) {
    for it in items {
        match it {
            It::Acc { store, w, off, addr, .. } => {
                lit(out, if *store { " W" } else { " A" });
                hole(out, Hole::Reg);
                lit(out, &format!(",{w},{off} {} ", addr.text()));
                hole(out, Hole::Pf);
                lit(out, " ");
                hole(out, Hole::Pf);
                lit(out, " ");
                hole(out, Hole::Pf);
            }
            It::Handle { h, r } => lit(out, &format!(" h{h},{r}")),
            It::Len { n, r, s } => lit(out, &format!(" n{n},{r},{s}")),
            It::Alloc { h, r, sz } => {
                lit(out, &format!(" a{h},{r} {} ", sz.text()));
                hole(out, Hole::Pf);
                lit(out, " ");
                hole(out, Hole::Pf);
            }
            It::Wf(r) => lit(out, &format!(" f{r}")),
            It::Static { r, b0, c0 } => lit(out, &format!(" g{r},{b0},{c0}")),
            It::Set(x) => lit(out, &format!(" s{x}")),
            It::SetR { x, lo, hi } => lit(out, &format!(" r{x},{lo},{hi}")),
            It::Asn { x, l } => {
                lit(out, &format!(" ={x} {} ", l.text()));
                hole(out, Hole::Pf);
                lit(out, " ");
                hole(out, Hole::Pf);
            }
            It::Test { a, b, sgn, thn, els } => {
                lit(out, &format!(" T{} {} {} ", if *sgn { "+" } else { "u" }, a.text(), b.text()));
                for i in 0..6 {
                    if i > 0 {
                        lit(out, " ");
                    }
                    hole(out, Hole::Pf);
                }
                lit(out, " {");
                items_template(thn, out);
                lit(out, " |");
                items_template(els, out);
                lit(out, " }");
            }
            It::Branch { thn, els } => {
                lit(out, " B{");
                items_template(thn, out);
                lit(out, " |");
                items_template(els, out);
                lit(out, " }");
            }
            It::Loop { body } => {
                lit(out, " L{ ");
                hole(out, Hole::Facts);
                lit(out, " | ");
                hole(out, Hole::Pfs);
                lit(out, " |");
                items_template(body, out);
                lit(out, " }");
            }
            It::Cont => {
                lit(out, " c{ ");
                hole(out, Hole::Pfs);
                lit(out, " }");
            }
            It::Brk => lit(out, " k"),
            It::Trap => lit(out, " x"),
            It::Ret => {
                lit(out, " R{ ");
                hole(out, Hole::Pfs);
                lit(out, " }");
            }
            It::Call { f, args, res } => {
                lit(out, &format!(" C{f}{{"));
                for (p, l) in args {
                    lit(out, &format!(" {p}:{}", l.text()));
                    hole(out, Hole::ArgPf);
                }
                lit(out, " | ");
                hole(out, Hole::Maps);
                lit(out, " | ");
                hole(out, Hole::Maps);
                lit(out, &format!(" | {res} | "));
                hole(out, Hole::Pfs);
                lit(out, " }");
            }
        }
    }
}

/// The procedure `Q<pos>{ pre | post | rslot | body }` as a template.
pub fn template(pos: usize, s: &Skel) -> Vec<Tok> {
    let mut out = Vec::new();
    lit(&mut out, &format!("Q{pos}{{ "));
    hole(&mut out, Hole::Facts);
    lit(&mut out, " | ");
    hole(&mut out, Hole::Facts);
    lit(&mut out, &format!(" | {} |", s.rslot));
    items_template(&s.body, &mut out);
    lit(&mut out, " }");
    out
}

// ── Matching a certificate against its template ───────────────────────────

/// The length of a `Z` at the head of `s` (optional `-`, digits).
fn z_len(s: &[u8]) -> Option<usize> {
    let neg = usize::from(s.first() == Some(&b'-'));
    let d = s[neg..].iter().take_while(|b| b.is_ascii_digit()).count();
    (d > 0).then_some(neg + d)
}

fn nat_len(s: &[u8]) -> Option<usize> {
    let d = s.iter().take_while(|b| b.is_ascii_digit()).count();
    (d > 0).then_some(d)
}

/// `( N:Z … )Z` with single spaces between entries.
fn pf_len(s: &[u8]) -> Option<usize> {
    if s.first() != Some(&b'(') {
        return None;
    }
    let mut i = 1;
    while s.get(i) != Some(&b')') {
        if i > 1 {
            if s.get(i) != Some(&b' ') {
                return None;
            }
            i += 1;
        }
        i += nat_len(&s[i..])?;
        if s.get(i) != Some(&b':') {
            return None;
        }
        i += 1;
        i += z_len(&s[i..])?;
    }
    i += 1;
    Some(i + z_len(&s[i..])?)
}

/// `[ V N:Z … ]Z`.
fn lin_len(s: &[u8]) -> Option<usize> {
    if s.first() != Some(&b'[') {
        return None;
    }
    let mut i = 1;
    while s.get(i) != Some(&b']') {
        if i > 1 {
            if s.get(i) != Some(&b' ') {
                return None;
            }
            i += 1;
        }
        if !matches!(s.get(i), Some(b's' | b'b' | b'c')) {
            return None;
        }
        i += 1;
        i += nat_len(&s[i..])?;
        if s.get(i) != Some(&b':') {
            return None;
        }
        i += 1;
        i += z_len(&s[i..])?;
    }
    i += 1;
    Some(i + z_len(&s[i..])?)
}

/// `unit ( ' ' unit )*` or nothing.
fn many_len(s: &[u8], unit: impl Fn(&[u8]) -> Option<usize>) -> usize {
    let mut i = match unit(s) {
        Some(n) => n,
        None => return 0,
    };
    while s.get(i) == Some(&b' ') {
        match unit(&s[i + 1..]) {
            Some(n) => i += 1 + n,
            None => break,
        }
    }
    i
}

fn map_len(s: &[u8]) -> Option<usize> {
    let a = nat_len(s)?;
    if s.get(a) != Some(&b':') {
        return None;
    }
    Some(a + 1 + nat_len(&s[a + 1..])?)
}

/// Does `cert` (from `at`) spell `tpl`, each hole filled by text of its
/// kind? The index after it.
pub fn matches(tpl: &[Tok], cert: &[u8], mut at: usize) -> Option<usize> {
    for t in tpl {
        let s = &cert[at.min(cert.len())..];
        at += match t {
            Tok::Lit(l) => s.starts_with(l.as_bytes()).then_some(l.len())?,
            Tok::Hole(Hole::Pf) => pf_len(s)?,
            Tok::Hole(Hole::Pfs) => many_len(s, pf_len),
            Tok::Hole(Hole::Facts) => many_len(s, lin_len),
            Tok::Hole(Hole::Reg) => nat_len(s)?,
            Tok::Hole(Hole::Maps) => many_len(s, map_len),
            Tok::Hole(Hole::ArgPf) => {
                if s.starts_with(b" ! ") {
                    let a = pf_len(&s[3..])?;
                    if s.get(3 + a) != Some(&b' ') {
                        return None;
                    }
                    3 + a + 1 + pf_len(&s[3 + a + 1..])?
                } else {
                    0
                }
            }
        };
    }
    Some(at)
}

/// A certificate's procedures, as the bounds witness's `Q` stretches
/// (`;`-separated, nothing else in it).
pub struct LinReport {
    pub procs: usize,
    pub sites: usize,
    pub counted: usize,
}

/// The whole translation check: `cert` is `;`-separated `Q` stretches, each
/// the template of the function it names with every hole filled; every
/// prim site of a function in the table is one of its events; every call of
/// a procedure with a non-empty precondition is in a certified body (the
/// table's own calls), and no such procedure is exported or in a table.
pub fn verify(wasm: &[u8], anchors: &str, cert: &str) -> Result<LinReport, String> {
    let m = Module::read(wasm)?;
    let (alloc, ..) = crate::parse_anchors(anchors)?;
    let sites = prim_sites(anchors)?;
    let heap = selfhost_params(anchors);
    let stretches: Vec<&str> = if cert.trim().is_empty() { Vec::new() } else { cert.split(" ; ").collect() };
    let mut table = BTreeSet::new();
    for st in &stretches {
        let b = st.as_bytes();
        if b.first() != Some(&b'Q') {
            return Err(format!("linear certificate: a stretch that is not a procedure: `{}`", &st[..st.len().min(40)]));
        }
        let n = nat_len(&b[1..]).ok_or("linear certificate: a procedure with no id")?;
        let id: usize = st[1..1 + n].parse().map_err(|_| "linear certificate: bad id")?;
        if !table.insert(id) {
            return Err(format!("linear certificate: procedure {id} twice"));
        }
    }
    let none = BTreeSet::new();
    let mut prims = 0;
    let mut nonempty_pre = BTreeSet::new();
    for st in &stretches {
        let id: usize = st[1..1 + nat_len(&st.as_bytes()[1..]).unwrap_or(0)].parse().unwrap_or(usize::MAX);
        let s = sites.get(&id).unwrap_or(&none);
        let hp = heap.get(&id).cloned().unwrap_or_default();
        let skel = translate(&m, id, s, &hp, &table, alloc).map_err(|e| format!("linear certificate: function {id}: {e}"))?;
        let tpl = template(id, &skel);
        match matches(&tpl, st.as_bytes(), 0) {
            Some(n) if n == st.len() => {}
            _ => return Err(format!("linear certificate: procedure {id} is not the translation of its function")),
        }
        prims += skel.sites.len();
        // `Q<id>{ ` then the precondition's facts: empty when `|` follows.
        let pre = &st[st.find('{').map_or(0, |i| i + 1)..];
        if !pre.trim_start().starts_with('|') {
            nonempty_pre.insert(id);
        }
    }
    // Linkage: a procedure with a precondition is called only from the
    // table's bodies (each such call is an LCall there, its precondition
    // proven), never exported, never in an element segment.
    let exported = exported_or_tabled(wasm, m.imports)?;
    for &f in &nonempty_pre {
        if exported.contains(&f) {
            return Err(format!("linear certificate: procedure {f} has a precondition but is exported or in a table"));
        }
        for (pos, b) in m.bodies.iter().enumerate() {
            if table.contains(&pos) {
                continue;
            }
            if b.ops.iter().any(|op| matches!(op, Op::Call { function_index } | Op::ReturnCall { function_index }
                if *function_index == m.imports + f as u32))
            {
                return Err(format!("linear certificate: procedure {f} has a precondition and is called from uncertified function {pos}"));
            }
        }
    }
    let total: usize = sites.values().map(BTreeSet::len).sum();
    Ok(LinReport { procs: stretches.len(), sites: prims, counted: total - prims })
}

/// Defined function positions an export or an element segment names.
pub fn exported_or_tabled(wasm: &[u8], imports: u32) -> Result<BTreeSet<usize>, String> {
    let mut out = BTreeSet::new();
    for payload in Parser::new(0).parse_all(wasm) {
        match payload.map_err(|e| e.to_string())? {
            Payload::ExportSection(r) => {
                for e in r {
                    let e = e.map_err(|e| e.to_string())?;
                    if e.kind == wasmparser::ExternalKind::Func && e.index >= imports {
                        out.insert((e.index - imports) as usize);
                    }
                }
            }
            Payload::ElementSection(r) => {
                for el in r {
                    let el = el.map_err(|e| e.to_string())?;
                    if let wasmparser::ElementItems::Functions(fs) = el.items {
                        for f in fs {
                            let f = f.map_err(|e| e.to_string())?;
                            if f >= imports {
                                out.insert((f - imports) as usize);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Every function with prim sites, translated with the given table (or all
/// of them), its template rendered with `_` for each hole — the
/// translation's own view, for inspection and for the prover.
pub fn skeletons(wasm: &[u8], anchors: &str, table: Option<&BTreeSet<usize>>) -> Result<Vec<(usize, Result<(Skel, Vec<Tok>), String>)>, String> {
    let m = Module::read(wasm)?;
    let (alloc, ..) = crate::parse_anchors(anchors)?;
    let sites = prim_sites(anchors)?;
    let heap = selfhost_params(anchors);
    let all: BTreeSet<usize> = sites.keys().copied().collect();
    let table = table.unwrap_or(&all);
    let mut out = Vec::new();
    for (&pos, s) in &sites {
        let hp = heap.get(&pos).cloned().unwrap_or_default();
        out.push((pos, translate(&m, pos, s, &hp, table, alloc).map(|k| {
            let t = template(pos, &k);
            (k, t)
        })));
    }
    Ok(out)
}

/// A template with every hole as `_`.
pub fn render_holes(t: &[Tok]) -> String {
    t.iter()
        .map(|x| match x {
            Tok::Lit(l) => l.clone(),
            Tok::Hole(_) => "_".to_string(),
        })
        .collect()
}
