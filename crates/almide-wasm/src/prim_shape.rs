//! The address shapes of a body's raw `prim.load*` / `prim.store*` calls
//! (#4005 A4). Each address is read as an affine sum over the body's
//! immutable `let`s and classified by what a bounds certificate needs to
//! hold it in its block:
//!
//! - `hdr-local` / `hdr-param`: `B + c` with `c + w <= 12`, B a block handle
//!   (`prim.handle(x)`, through `let`s) or an Int parameter. A header word:
//!   liveness only (`blk`).
//! - `slot-local` / `slot-param`: `B + c + Σ k·W` with `c >= 12`: a payload
//!   slot, which needs a test of its index against a count of B's block.
//! - `ptr-param`: an Int parameter plus an index with no payload offset — a
//!   pointer into a sub-range the caller formed (a range fact).
//! - `ptr-walk`: a mutable local pointer.
//! - `other`: any other shape (an address loaded from memory, a computed
//!   table offset): the counted class, driven down by rewriting the body.
//!
//! Nothing here is trusted: it names what is to be certified and counts what
//! is not yet. `ALMIDE_PRIM_SHAPE_LOG=<path>` appends one line per site of
//! every body the emitter lowered (module, function, site, op, shape).

use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
use almide_ir::{BinOp, CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarId};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Atom {
    Handle(VarId),
    Var(VarId),
    Opaque,
}

#[derive(Clone, Debug, Default)]
struct Lin {
    c: i64,
    terms: Vec<(i64, Atom)>,
}

impl Lin {
    fn atom(a: Atom) -> Self {
        Lin { c: 0, terms: vec![(1, a)] }
    }
    fn add(mut self, o: Lin, sign: i64) -> Self {
        self.c = self.c.saturating_add(o.c.saturating_mul(sign));
        self.terms.extend(o.terms.into_iter().map(|(k, a)| (k.saturating_mul(sign), a)));
        self
    }
    fn scale(mut self, k: i64) -> Self {
        self.c = self.c.saturating_mul(k);
        for t in &mut self.terms {
            t.0 = t.0.saturating_mul(k);
        }
        self
    }
    fn konst(&self) -> Option<i64> {
        self.terms.is_empty().then_some(self.c)
    }
}

/// The prim memory ops and their access width in bytes.
pub(crate) fn mem_op(func: &str) -> Option<(u32, bool)> {
    Some(match func {
        "load8" => (1, false),
        "load32" => (4, false),
        "load64" => (8, false),
        "store8" => (1, true),
        "store32" => (4, true),
        "store64" => (8, true),
        _ => return None,
    })
}

fn prim_call<'e>(e: &'e IrExpr) -> Option<(&'e str, &'e [IrExpr])> {
    match &e.kind {
        IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } if module.as_str() == "prim" => {
            Some((func.as_str(), args.as_slice()))
        }
        _ => None,
    }
}

#[derive(Default)]
struct Scan {
    lets: HashMap<VarId, IrExpr>,
    mutated: HashSet<VarId>,
    sites: Vec<(String, IrExpr)>,
}

impl IrVisitor for Scan {
    fn visit_expr(&mut self, e: &IrExpr) {
        if let Some((func, args)) = prim_call(e)
            && mem_op(func).is_some()
            && let Some(a) = args.first()
        {
            self.sites.push((func.to_string(), a.clone()));
        }
        walk_expr(self, e);
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        match &s.kind {
            IrStmtKind::Bind { var, mutability: Mutability::Let, value, .. } => {
                self.lets.insert(*var, value.clone());
            }
            IrStmtKind::Bind { var, .. } | IrStmtKind::Assign { var, .. } => {
                self.mutated.insert(*var);
            }
            _ => {}
        }
        walk_stmt(self, s);
    }
}

struct Ctx<'a> {
    scan: &'a Scan,
    params: &'a HashSet<VarId>,
}

impl Ctx<'_> {
    fn lin(&self, e: &IrExpr, depth: u32) -> Lin {
        match &e.kind {
            IrExprKind::LitInt { value } => Lin { c: *value, terms: Vec::new() },
            IrExprKind::BinOp { op: BinOp::AddInt, left, right } => self.lin(left, depth).add(self.lin(right, depth), 1),
            IrExprKind::BinOp { op: BinOp::SubInt, left, right } => self.lin(left, depth).add(self.lin(right, depth), -1),
            IrExprKind::BinOp { op: BinOp::MulInt, left, right } => {
                let (l, r) = (self.lin(left, depth), self.lin(right, depth));
                match (l.konst(), r.konst()) {
                    (_, Some(k)) => l.scale(k),
                    (Some(k), _) => r.scale(k),
                    _ => Lin::atom(Atom::Opaque),
                }
            }
            IrExprKind::Var { id } => match self.scan.lets.get(id) {
                Some(v) if depth < 8 && !self.scan.mutated.contains(id) => {
                    if let Some(("handle", [_])) = prim_call(v) {
                        Lin::atom(Atom::Handle(*id))
                    } else {
                        self.lin(v, depth + 1)
                    }
                }
                _ => Lin::atom(Atom::Var(*id)),
            },
            _ => match prim_call(e) {
                Some(("handle", [_])) => Lin::atom(Atom::Handle(VarId(u32::MAX))),
                _ => Lin::atom(Atom::Opaque),
            },
        }
    }

    fn classify(&self, func: &str, a: &IrExpr) -> &'static str {
        let Some((w, _)) = mem_op(func) else { return "other" };
        let l = self.lin(a, 0);
        if l.terms.iter().any(|(_, t)| *t == Atom::Opaque) {
            return "other";
        }
        let is_base = |t: &Atom| match t {
            Atom::Handle(_) => true,
            Atom::Var(v) => self.params.contains(v) && !self.scan.mutated.contains(v),
            Atom::Opaque => false,
        };
        let bases: Vec<&Atom> = l.terms.iter().filter(|(k, t)| *k == 1 && is_base(t)).map(|(_, t)| t).collect();
        let Some(&base) = bases.first() else {
            let walk = l.terms.iter().any(|(k, t)| *k == 1 && matches!(t, Atom::Var(v) if self.scan.mutated.contains(v)));
            return if walk { "ptr-walk" } else { "other" };
        };
        let param = matches!(base, Atom::Var(_));
        let mut rest = l.terms.clone();
        if let Some(p) = rest.iter().position(|(k, t)| *k == 1 && t == base) {
            rest.remove(p);
        }
        // Every index term scales a counter by a positive stride; another
        // block's handle among them is not an index.
        if rest.iter().any(|(k, t)| *k <= 0 || matches!(t, Atom::Handle(_))) {
            return "other";
        }
        match (rest.is_empty(), l.c) {
            (true, c) if c >= 0 && c + i64::from(w) <= 12 => {
                if param { "hdr-param" } else { "hdr-local" }
            }
            (_, c) if c >= 12 => {
                if param { "slot-param" } else { "slot-local" }
            }
            (false, c) if param && c >= 0 => "ptr-param",
            _ => "other",
        }
    }
}

/// Log the shape of every prim memory op in `body` (see the module doc).
pub(crate) fn log_fn(name: &str, module: Option<&str>, params: &[VarId], body: &IrExpr) {
    let Some(path) = std::env::var_os("ALMIDE_PRIM_SHAPE_LOG") else { return };
    let mut scan = Scan::default();
    scan.visit_expr(body);
    if scan.sites.is_empty() {
        return;
    }
    let params: HashSet<VarId> = params.iter().copied().collect();
    let cx = Ctx { scan: &scan, params: &params };
    let mut out = String::new();
    for (n, (func, a)) in scan.sites.iter().enumerate() {
        out.push_str(&format!("{}\t{name}\t{n}\t{func}\t{}\n", module.unwrap_or("-"), cx.classify(func, a)));
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(out.as_bytes());
    }
}
