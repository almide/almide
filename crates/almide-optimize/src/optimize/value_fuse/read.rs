//! The reader half of the Value fusion: a parsed document whose every use
//! reads a scalar at a static key path is read off the text.
//!
//! **The rule.** `let d = <parse of t>` (`json.parse(t) ?? value.null()`, the
//! `match` spelling of it, or `json.parse(t)!`) is not built when every
//! occurrence of `d` after the bind is one of
//!
//! - a *leaf read* `value.get_<kind>(<steps>, "k")`, `<steps>` being `d` under
//!   any number of `value.get(_, "k") ?? value.null()` steps. The read is an
//!   Option, so whatever consumes it (`?? default`, a `match`, `!`, a
//!   comparison with `none`) is untouched;
//! - an *alias* `let m = <steps>`, whose uses are then held to the same
//!   rule (`m` stands for the value at that path);
//! - a call `g(d)` of a *reader function*: a plain function of one Value
//!   whose every use of its parameter is again a leaf read or such a call;
//! - one *row site*: a call of a `list` function,
//!   `list.f(<rows>, .., (.., x) => body)`, whose first argument is
//!   `value.as_array(d) ?? []` or `value.get_array(<steps>, "k") ?? []`,
//!   whose callback takes the element last and uses it only in leaf reads
//!   and reader calls, and whose own type, other arguments and other
//!   callback parameters hold no Value. The module is generic in its
//!   element type throughout, so such a function can tell its
//!   elements apart only through the callback, so running it over the row
//!   numbers with the callback reading row `i` of the scan is the same
//!   computation: `map`, `fold`, `filter_map`, `flat_map`, `any`, `all`,
//!   `count`, `each`, `find_index` all qualify by shape, and `filter`,
//!   `find`, `sort_by` (which hand elements back) do not.
//!
//! The leaf kinds are the four scalar members of the `value.get_*` family
//! (`Leaf::ALL`); the family's other members are deliberately not leaves:
//! `get_array` is the row site, `get` is a path step, and `get_object` hands
//! out the pairs of a tree that would then have to exist. The `value.as_*`
//! family answers a Result whose error text names the kind found, which the
//! scan does not keep, so it is not fused either
//! (`tests/value_fuse_controls_test.rs` holds the matrix).
//!
//! The rewrite keeps the bind where it is: `let cols = json.__spans(bytes,
//! <plan>)` runs where the parse ran, the document becomes `match cols {
//! some(_) => value.null(), none => <the parse> }`, and every site becomes
//! `match cols { some(c) => <the site over c>, none => <the site> }`. A
//! document the kernel refuses is parsed exactly as before and read by the
//! original sites, so every error and every edge case is the parser's own.

use std::rc::Rc;

use super::*;

pub(super) const SPANS: &str = "__spans";
const ROWS: &str = "__rows";

/// The scalar kinds a leaf read answers: the scalar members of `value.get_*`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Leaf {
    Int,
    Float,
    Bool,
    Str,
}

impl Leaf {
    pub(super) const ALL: [Leaf; 4] = [Leaf::Int, Leaf::Float, Leaf::Bool, Leaf::Str];

    /// The `value` accessor this kind stands behind.
    fn accessor(self) -> &'static str {
        match self {
            Leaf::Int => "get_int",
            Leaf::Float => "get_float",
            Leaf::Bool => "get_bool",
            Leaf::Str => "get_string",
        }
    }

    /// The `json` kernel that answers it from a scan.
    pub(super) fn kernel(self) -> &'static str {
        match self {
            Leaf::Int => "__leaf_int",
            Leaf::Float => "__leaf_float",
            Leaf::Bool => "__leaf_bool",
            Leaf::Str => "__leaf_string",
        }
    }
}

/// The reading plan of one document: a tree of object keys under the root
/// (node 0), the nodes whose scalar is read, and the row array with the node
/// standing for one of its elements.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Plan {
    keys: Vec<String>,
    par: Vec<i64>,
    read: Vec<bool>,
    row: Option<usize>,
    el: Option<usize>,
}

impl Plan {
    fn new() -> Self {
        Plan { keys: vec![String::new()], par: vec![-1], read: vec![false], row: None, el: None }
    }

    fn push(&mut self, par: i64, key: &str) -> usize {
        self.keys.push(key.to_string());
        self.par.push(par);
        self.read.push(false);
        self.keys.len() - 1
    }

    fn find(&self, base: usize, path: &[String]) -> Option<usize> {
        path.iter()
            .try_fold(base, |at, k| (1..self.keys.len()).find(|&j| self.par[j] == at as i64 && &self.keys[j] == k))
    }

    fn node(&mut self, base: usize, path: &[String]) -> usize {
        path.iter().fold(base, |at, k| match self.find(at, std::slice::from_ref(k)) {
            Some(j) => j,
            None => self.push(at as i64, k),
        })
    }

    fn read_at(&mut self, base: usize, path: &[String]) {
        let n = self.node(base, path);
        self.read[n] = true;
    }

    /// Marks the array at node `at` as the row array; the element node.
    fn open_rows(&mut self, at: usize) -> usize {
        self.row = Some(at);
        // No object key leads to the element node: its parent is no node.
        let el = self.push(-2, "");
        self.el = Some(el);
        el
    }

    fn under_el(&self, mut n: usize) -> bool {
        loop {
            if Some(n) == self.el {
                return true;
            }
            if self.par[n] < 0 {
                return false;
            }
            n = self.par[n] as usize;
        }
    }

    /// The slot of every node (-1: not read), the count read once per
    /// document and the count read per row. Document slots come first.
    fn slots(&self) -> (Vec<i64>, i64, i64) {
        let n = self.keys.len();
        let g = (0..n).filter(|&j| self.read[j] && !self.under_el(j)).count() as i64;
        let (mut gi, mut ki) = (0, 0);
        let slots = (0..n)
            .map(|j| {
                if !self.read[j] {
                    -1
                } else if self.under_el(j) {
                    ki += 1;
                    g + ki - 1
                } else {
                    gi += 1;
                    gi - 1
                }
            })
            .collect();
        (slots, g, ki)
    }

    /// The key path of every read node, from the root.
    fn paths(&self) -> Vec<Vec<String>> {
        (0..self.keys.len())
            .filter(|&j| self.read[j])
            .map(|mut j| {
                let mut path = Vec::new();
                while self.par[j] >= 0 {
                    path.push(self.keys[j].clone());
                    j = self.par[j] as usize;
                }
                path.reverse();
                path
            })
            .collect()
    }
}

/// A plan with its slots numbered: what one fused document reads by.
pub(super) struct Job {
    plan: Plan,
    slots: Vec<i64>,
    g: i64,
    k: i64,
}

/// A reader function wanted as a function of (bytes, scan, row).
pub(super) struct ReadTwin {
    pub(super) src: Sym,
    name: Sym,
    job: Rc<Job>,
    base: usize,
}

/// Reader functions: the key paths each reads under its parameter.
pub(super) type Readers = HashMap<Sym, Vec<Vec<String>>>;

// ── shapes ──────────────────────────────────────────────────────────────

/// The variables that stand for a value of the document, each with its
/// plan node: the document (or the element) and its aliases.
type Roots = HashMap<VarId, usize>;

/// `value.get(.. value.get(r, k0) ?? value.null() .., kn) ?? value.null()`
/// down from a root `r`: the root's node and the keys, outermost first.
fn steps(e: &IrExpr, roots: &Roots) -> Option<(usize, Vec<String>)> {
    if let IrExprKind::Var { id } = &e.kind {
        return roots.get(id).map(|base| (*base, Vec::new()));
    }
    let IrExprKind::UnwrapOr { expr, fallback } = &e.kind else { return None };
    let (Some([inner, key]), true) = (module_call(expr, "value", "get"), is_value_null(fallback)) else { return None };
    let IrExprKind::LitStr { value } = &key.kind else { return None };
    let (base, mut keys) = steps(inner, roots)?;
    keys.push(value.clone());
    Some((base, keys))
}

/// `value.get_<kind>(<steps from a root>, "k")`: the root's node, the key
/// path and the kind.
fn leaf_read(e: &IrExpr, roots: &Roots) -> Option<(usize, Vec<String>, Leaf)> {
    let kind = Leaf::ALL.into_iter().find(|l| module_call(e, "value", l.accessor()).is_some())?;
    let [inner, key] = module_call(e, "value", kind.accessor())? else { return None };
    let IrExprKind::LitStr { value: key } = &key.kind else { return None };
    let (base, mut keys) = steps(inner, roots)?;
    // A key the kernel compares raw is what the parser decodes: the kernel
    // refuses a document key holding an escape.
    keys.push(key.clone());
    Some((base, keys, kind))
}

/// `g(r)` of a root `r` → `g` and the root's node.
fn reader_call(e: &IrExpr, roots: &Roots) -> Option<(Sym, usize)> {
    let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = &e.kind else { return None };
    match args.as_slice() {
        [IrExpr { kind: IrExprKind::Var { id }, .. }] if type_args.is_empty() => roots.get(id).map(|base| (*name, *base)),
        _ => None,
    }
}

/// Conservative: a type whose spelling names `Value` anywhere.
fn mentions_value(ty: &Ty) -> bool {
    format!("{ty:?}").contains("Value")
}

struct RowSite<'e> {
    /// The root the row array is read from and its key path under it.
    base: usize,
    path: Vec<String>,
    param: VarId,
    body: &'e IrExpr,
    others: &'e [IrExpr],
}

/// `<rows of a root> ?? []` → the root's node and the key path of the array.
fn rows_of(xs: &IrExpr, roots: &Roots) -> Option<(usize, Vec<String>)> {
    let IrExprKind::UnwrapOr { expr, fallback } = &xs.kind else { return None };
    if !matches!(&fallback.kind, IrExprKind::List { elements } if elements.is_empty()) {
        return None;
    }
    if let Some([doc]) = module_call(expr, "value", "as_array") {
        return steps(doc, roots);
    }
    let [inner, key] = module_call(expr, "value", "get_array")? else { return None };
    let IrExprKind::LitStr { value: key } = &key.kind else { return None };
    let (base, mut keys) = steps(inner, roots)?;
    keys.push(key.clone());
    Some((base, keys))
}

/// A `list` function that takes a list of Values takes it as a list of any
/// element type — no function of the module is written over Value
/// (`tests/value_fuse_controls_test.rs` holds the module to that) — so it
/// can tell its elements apart only through the callback. The declaration
/// itself cannot be asked: an intrinsic's is not in the linked program.
fn row_site<'e>(e: &'e IrExpr, roots: &Roots) -> Option<RowSite<'e>> {
    let IrExprKind::Call { target: CallTarget::Module { module, .. }, args, type_args } = &e.kind else {
        return None;
    };
    if module.as_str() != "list" || !type_args.is_empty() || mentions_value(&e.ty) {
        return None;
    }
    let (xs, rest) = args.split_first()?;
    let (f, others) = rest.split_last()?;
    let (base, path) = rows_of(xs, roots)?;
    let IrExprKind::Lambda { params, body, .. } = &f.kind else { return None };
    let ((x, xty), accs) = params.split_last()?;
    if !is_value_ty(xty)
        || accs.len() != others.len()
        || accs.iter().any(|(_, t)| mentions_value(t))
        || others.iter().any(|a| mentions_value(&a.ty))
    {
        return None;
    }
    Some(RowSite { base, path, param: *x, body, others })
}

/// The text operand of a parse whose failure is a null document or leaves
/// the function: `json.parse(t) ?? value.null()`, its `match` spelling,
/// `json.parse(t)!`.
fn parse_of(e: &mut IrExpr) -> Option<&mut IrExpr> {
    if !is_value_ty(&e.ty) {
        return None;
    }
    let parse = match &mut e.kind {
        IrExprKind::UnwrapOr { expr, fallback } if is_value_null(fallback) => expr,
        IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } => expr,
        IrExprKind::Match { subject, arms } => {
            let [a, b] = arms.as_slice() else { return None };
            let ok = matches!(&a.pattern, IrPattern::Ok { inner }
                if matches!(**inner, IrPattern::Bind { var, .. } if is_var(tail(&a.body), var)));
            let err = matches!(&b.pattern, IrPattern::Err { inner }
                if matches!(**inner, IrPattern::Wildcard | IrPattern::Bind { .. }));
            if !ok || !err || a.guard.is_some() || b.guard.is_some() || !is_value_null(tail(&b.body)) {
                return None;
            }
            subject
        }
        _ => return None,
    };
    module_call(parse, "json", "parse")?;
    let IrExprKind::Call { args, .. } = &mut parse.kind else { return None };
    match args.as_mut_slice() {
        [text] if matches!(text.ty, Ty::String) => Some(text),
        _ => None,
    }
}

// ── phase 1: the plan ───────────────────────────────────────────────────

/// Builds the plan of the reads of its roots and counts the occurrences of
/// the roots they account for.
struct Collect<'a> {
    roots: Roots,
    plan: &'a mut Plan,
    readers: &'a Readers,
    rows_ok: bool,
    uses: usize,
}

impl<'a> Collect<'a> {
    fn new(root: VarId, base: usize, plan: &'a mut Plan, readers: &'a Readers) -> Self {
        Collect { roots: Roots::from([(root, base)]), plan, readers, rows_ok: false, uses: 0 }
    }

    /// Whether every occurrence of a root in the region walked is a use
    /// the plan answers, and there is one.
    fn accounted(&self, stmts: &[IrStmt], tail: Option<&IrExpr>) -> bool {
        self.uses >= 1 && self.uses == self.roots.keys().map(|v| occurrences_in(stmts, tail, *v)).sum::<usize>()
    }
}

impl IrVisitor for Collect<'_> {
    fn visit_stmt(&mut self, s: &IrStmt) {
        if let IrStmtKind::Bind { var, mutability: Mutability::Let, value, .. } = &s.kind {
            if let Some((base, keys)) = steps(value, &self.roots) {
                let n = self.plan.node(base, &keys);
                self.roots.insert(*var, n);
                self.uses += 1;
                return;
            }
        }
        almide_ir::visit::walk_stmt(self, s);
    }

    fn visit_expr(&mut self, e: &IrExpr) {
        if let Some((base, keys, _)) = leaf_read(e, &self.roots) {
            self.plan.read_at(base, &keys);
            self.uses += 1;
            return;
        }
        if let Some((paths, base)) = reader_call(e, &self.roots).and_then(|(g, b)| Some((self.readers.get(&g)?, b))) {
            for p in paths {
                self.plan.read_at(base, p);
            }
            self.uses += 1;
            return;
        }
        if let Some(site) = row_site(e, &self.roots).filter(|_| self.rows_ok) {
            let before = self.plan.clone();
            let at = self.plan.node(site.base, &site.path);
            // One row array per document; any number of sites over it.
            let el = match (self.plan.row, self.plan.el) {
                (None, _) => Some(self.plan.open_rows(at)),
                (Some(row), el) if row == at => el,
                _ => None,
            };
            if let Some(el) = el {
                let mut inner = Collect::new(site.param, el, &mut *self.plan, self.readers);
                inner.visit_expr(site.body);
                let read = inner.accounted(&[], Some(site.body));
                if read && self.plan.slots().2 >= 1 && renamable(e) {
                    self.uses += 1;
                    for a in site.others {
                        self.visit_expr(a);
                    }
                    return;
                }
            }
            *self.plan = before;
        }
        walk_expr(self, e);
    }
}

/// The reader functions of a program, grown to the fixpoint (a reader may
/// hand its parameter to another).
pub(super) fn reader_fns(functions: &[IrFunction]) -> Readers {
    let mut readers = Readers::new();
    loop {
        let before = readers.len();
        for f in functions {
            let [p] = f.params.as_slice() else { continue };
            if readers.contains_key(&f.name) || !plain_fn(f) || !is_value_ty(&p.ty) || !renamable(&f.body) {
                continue;
            }
            let mut plan = Plan::new();
            let mut c = Collect::new(p.var, 0, &mut plan, &readers);
            c.visit_expr(&f.body);
            if c.accounted(&[], Some(&f.body)) {
                readers.insert(f.name, plan.paths());
            }
        }
        if readers.len() == before {
            return readers;
        }
    }
}

// ── phase 2: the rewrite ────────────────────────────────────────────────

fn mk(kind: IrExprKind, ty: Ty, span: Option<almide_base::Span>) -> IrExpr {
    IrExpr { kind, ty, span, def_id: None }
}

fn call(module: &str, func: &str, args: Vec<IrExpr>, ty: Ty, span: Option<almide_base::Span>) -> IrExpr {
    let target = CallTarget::Module { module: sym(module), func: sym(func), def_id: None };
    mk(IrExprKind::Call { target, args, type_args: vec![] }, ty, span)
}

fn ints(xs: impl IntoIterator<Item = i64>, span: Option<almide_base::Span>) -> IrExpr {
    let elements = xs.into_iter().map(|value| mk(IrExprKind::LitInt { value }, Ty::Int, span)).collect();
    mk(IrExprKind::List { elements }, Ty::list(Ty::Int), span)
}

fn bind(var: VarId, ty: Ty, value: IrExpr) -> IrStmt {
    let span = value.span;
    IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value }, span }
}

/// `match cols { <some pattern> => then, none => otherwise }`.
fn on_scan(cols: VarId, some: IrPattern, then: IrExpr, otherwise: IrExpr) -> IrExpr {
    let (ty, span) = (otherwise.ty.clone(), otherwise.span);
    let subject = mk(IrExprKind::Var { id: cols }, Ty::option(Ty::list(Ty::Int)), span);
    let arms = vec![
        IrMatchArm { pattern: IrPattern::Some { inner: Box::new(some) }, guard: None, body: then },
        IrMatchArm { pattern: IrPattern::None, guard: None, body: otherwise },
    ];
    mk(IrExprKind::Match { subject: Box::new(subject), arms }, ty, span)
}

fn null_like(e: &IrExpr) -> IrExpr {
    call("value", "null", vec![], e.ty.clone(), e.span)
}

/// The alias a `let` binds, when its value is steps from a root: the
/// variable and its node.
fn alias(s: &IrStmt, roots: &Roots, plan: &Plan) -> Option<(VarId, usize)> {
    let IrStmtKind::Bind { var, mutability: Mutability::Let, value, .. } = &s.kind else { return None };
    let (base, keys) = steps(value, roots)?;
    Some((*var, plan.find(base, &keys)?))
}

/// Rewrites the reads of its roots over the scan `c`, row `i`.
struct OverScan<'a, 'b> {
    cx: &'a mut Fuser<'b>,
    job: &'a Rc<Job>,
    roots: Roots,
    b: VarId,
    c: VarId,
    i: Option<VarId>,
}

impl IrMutVisitor for OverScan<'_, '_> {
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        if let Some((var, node)) = alias(s, &self.roots, &self.job.plan) {
            // The alias is read nowhere now: every use of it was a site.
            self.roots.insert(var, node);
            if let IrStmtKind::Bind { value, .. } = &mut s.kind {
                *value = null_like(value);
            }
            return;
        }
        walk_stmt_mut(self, s);
    }

    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if let Some(fused) = self.cx.fused(e, self.job, &self.roots, self.b, self.c, self.i) {
            *e = fused;
            return;
        }
        if let IrExprKind::Lambda { lambda_id, .. } = &mut e.kind {
            *lambda_id = None;
        }
        walk_expr_mut(self, e);
    }
}

/// Wraps every site over the document in a match on the scan.
struct AtDoc<'a, 'b> {
    cx: &'a mut Fuser<'b>,
    job: &'a Rc<Job>,
    roots: Roots,
    b: VarId,
    cols: VarId,
}

impl IrMutVisitor for AtDoc<'_, '_> {
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        if let Some((var, node)) = alias(s, &self.roots, &self.job.plan) {
            self.roots.insert(var, node);
            if let IrStmtKind::Bind { value, .. } = &mut s.kind {
                let old = std::mem::take(value);
                *value = on_scan(self.cols, IrPattern::Wildcard, null_like(&old), old);
            }
            return;
        }
        walk_stmt_mut(self, s);
    }

    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        let site = leaf_read(e, &self.roots).is_some()
            || reader_call(e, &self.roots).is_some()
            || row_site(e, &self.roots).is_some();
        if site {
            let col_ty = Ty::list(Ty::Int);
            let c = self.cx.vt.alloc(sym("scan"), col_ty.clone(), Mutability::Let, e.span);
            if let Some(fused) = self.cx.fused(e, self.job, &self.roots, self.b, c, None) {
                let old = std::mem::take(e);
                *e = on_scan(self.cols, IrPattern::Bind { var: c, ty: col_ty }, fused, old);
                return;
            }
        }
        walk_expr_mut(self, e);
    }
}

impl Fuser<'_> {
    /// The site `e` over the scan `c`: a leaf read as its kernel, a reader
    /// call as a call of its twin, the row site as the same `list` call over
    /// the row numbers. `None` when `e` is not a site of a root.
    fn fused(
        &mut self,
        e: &IrExpr,
        job: &Rc<Job>,
        roots: &Roots,
        b: VarId,
        c: VarId,
        i: Option<VarId>,
    ) -> Option<IrExpr> {
        let span = e.span;
        let col_ty = Ty::list(Ty::Int);
        let var = |id: VarId, ty: Ty| mk(IrExprKind::Var { id }, ty, span);
        let int = |value: i64| mk(IrExprKind::LitInt { value }, Ty::Int, span);
        let row = || i.map_or_else(|| int(0), |i| var(i, Ty::Int));
        if let Some((base, keys, kind)) = leaf_read(e, roots) {
            let slot = job.slots[job.plan.find(base, &keys)?];
            let per_row = if job.plan.under_el(base) { job.k } else { 0 };
            let args = vec![var(b, Ty::Bytes), var(c, col_ty), row(), int(per_row), int(slot)];
            return Some(call("json", kind.kernel(), args, e.ty.clone(), span));
        }
        if let Some((g, base)) = reader_call(e, roots).filter(|(g, _)| self.readers.contains_key(g)) {
            let name = sym(&format!("__almd_json_read_{}_{}", g.as_str(), self.counter));
            self.counter += 1;
            self.read_twins.push(ReadTwin { src: g, name, job: job.clone(), base });
            let args = vec![var(b, Ty::Bytes), var(c, col_ty), row()];
            return Some(mk(
                IrExprKind::Call { target: CallTarget::Named { name }, args, type_args: vec![] },
                e.ty.clone(),
                span,
            ));
        }
        if i.is_some() {
            return None;
        }
        let site = row_site(e, roots)?;
        let el = job.plan.el?;
        if job.plan.row != job.plan.find(site.base, &site.path) {
            return None;
        }
        let mut out = e.clone();
        Rename { vt: &mut *self.vt, map: HashMap::new() }.visit_expr_mut(&mut out);
        let iv = self.vt.alloc(sym("row"), Ty::Int, Mutability::Let, span);
        let IrExprKind::Call { args, .. } = &mut out.kind else { return None };
        // The kernel hands out the row numbers itself: a program that
        // reads rows need not otherwise link `list.range`.
        args[0] = call("json", ROWS, vec![var(c, col_ty), int(job.g), int(job.k)], Ty::list(Ty::Int), span);
        let f = args.last_mut()?;
        if let Ty::Fn { params, .. } = &mut f.ty {
            *params.last_mut()? = Ty::Int;
        }
        let IrExprKind::Lambda { params, body, lambda_id } = &mut f.kind else { return None };
        let x = std::mem::replace(params.last_mut()?, (iv, Ty::Int)).0;
        *lambda_id = None;
        OverScan { cx: self, job, roots: Roots::from([(x, el)]), b, c, i: Some(iv) }.visit_expr_mut(body);
        Some(out)
    }

    /// The document `doc` (bound by `stmt` to a parse), read in `rest` and
    /// `tail`: when every use is a site, the sites read a scan and the binds
    /// of the text (when it was not a variable), the bytes and the scan are
    /// returned, to go before `stmt`.
    fn fuse_doc(
        &mut self,
        doc: VarId,
        stmt: &mut IrStmt,
        rest: &mut [IrStmt],
        tail: &mut Option<Box<IrExpr>>,
    ) -> Option<Vec<IrStmt>> {
        let mut plan = Plan::new();
        let mut c = Collect::new(doc, 0, &mut plan, self.readers);
        c.rows_ok = true;
        for s in rest.iter() {
            c.visit_stmt(s);
        }
        if let Some(t) = tail.as_deref() {
            c.visit_expr(t);
        }
        if !c.accounted(rest, tail.as_deref()) {
            return None;
        }
        let span = stmt.span;
        let (slots, g, k) = plan.slots();
        if g + k == 0 {
            return None;
        }
        let cols_ty = Ty::option(Ty::list(Ty::Int));
        let b = self.vt.alloc(sym("bytes"), Ty::Bytes, Mutability::Let, span);
        let cols = self.vt.alloc(sym("cols"), cols_ty.clone(), Mutability::Let, span);
        let strs = |xs: &[String]| {
            let elements = xs.iter().map(|k| mk(IrExprKind::LitStr { value: k.clone() }, Ty::String, span)).collect();
            mk(IrExprKind::List { elements }, Ty::list(Ty::String), span)
        };
        let node = |n: Option<usize>| n.map_or(-1, |n| n as i64);
        let scan = call(
            "json",
            SPANS,
            vec![
                mk(IrExprKind::Var { id: b }, Ty::Bytes, span),
                strs(&plan.keys),
                ints(plan.par.iter().copied(), span),
                ints(slots.iter().copied(), span),
                ints([node(plan.row), node(plan.el), g, k], span),
            ],
            cols_ty.clone(),
            span,
        );
        let IrStmtKind::Bind { value, .. } = &mut stmt.kind else { return None };
        let slot = parse_of(value)?;
        let mut binds = Vec::new();
        // The text is read twice (the scan, and the parse of a refused
        // document): anything but a variable is evaluated once, here.
        if !matches!(slot.kind, IrExprKind::Var { .. }) {
            let t = self.vt.alloc(sym("text"), Ty::String, Mutability::Let, span);
            let text = std::mem::replace(slot, mk(IrExprKind::Var { id: t }, Ty::String, span));
            binds.push(bind(t, Ty::String, text));
        }
        let text = slot.clone();
        binds.push(bind(b, Ty::Bytes, call("bytes", "from_string", vec![text], Ty::Bytes, span)));
        binds.push(bind(cols, cols_ty, scan));
        let job = Rc::new(Job { plan, slots, g, k });
        let mut at = AtDoc { cx: self, job: &job, roots: Roots::from([(doc, 0)]), b, cols };
        for s in rest.iter_mut() {
            at.visit_stmt_mut(s);
        }
        if let Some(t) = tail.as_deref_mut() {
            at.visit_expr_mut(t);
        }
        let IrStmtKind::Bind { value, .. } = &mut stmt.kind else { return None };
        let parse = std::mem::take(value);
        *value = on_scan(cols, IrPattern::Wildcard, null_like(&parse), parse);
        Some(binds)
    }

    /// Every `let d = <parse>` of a block whose uses are all sites.
    pub(super) fn fuse_bound_docs(&mut self, stmts: &mut Vec<IrStmt>, tail: &mut Option<Box<IrExpr>>) {
        let mut i = 0;
        while i < stmts.len() {
            let found = match &mut stmts[i].kind {
                IrStmtKind::Bind { var, mutability: Mutability::Let, value, .. } => parse_of(value).map(|_| *var),
                _ => None,
            };
            if let Some(doc) = found {
                let (head, rest) = stmts.split_at_mut(i + 1);
                if let Some(binds) = self.fuse_doc(doc, &mut head[i], rest, tail) {
                    let n = binds.len();
                    stmts.splice(i..i, binds);
                    i += n;
                }
            }
            i += 1;
        }
    }

    /// A row site over an unbound parse, `list.f(value.as_array(<parse>) ??
    /// [], ..)`: read as if the parse were bound just before it.
    pub(super) fn fuse_inline_doc(&mut self, e: &mut IrExpr) {
        let IrExprKind::Call { target: CallTarget::Module { module, .. }, args, .. } = &mut e.kind else { return };
        if module.as_str() != "list" {
            return;
        }
        let Some(IrExprKind::UnwrapOr { expr, .. }) = args.first_mut().map(|a| &mut a.kind) else { return };
        if module_call(expr, "value", "as_array").is_none() {
            return;
        }
        let IrExprKind::Call { args: inner, .. } = &mut expr.kind else { return };
        let [arg] = inner.as_mut_slice() else { return };
        if parse_of(arg).is_none() {
            return;
        }
        let doc = self.vt.alloc(sym("doc"), arg.ty.clone(), Mutability::Let, arg.span);
        let read = mk(IrExprKind::Var { id: doc }, arg.ty.clone(), arg.span);
        let parse = std::mem::replace(arg, read);
        let mut stmt = bind(doc, parse.ty.clone(), parse);
        let mut tail = Some(Box::new(std::mem::take(e)));
        let fused = self.fuse_doc(doc, &mut stmt, &mut [], &mut tail);
        let Some(site) = tail else { return };
        *e = match fused {
            Some(mut stmts) => {
                stmts.push(stmt);
                mk(IrExprKind::Block { stmts, expr: Some(site.clone()) }, site.ty.clone(), site.span)
            }
            None => {
                let mut site = *site;
                if let IrStmtKind::Bind { value, .. } = &stmt.kind {
                    Subst { from: doc, to: value }.visit_expr_mut(&mut site);
                }
                site
            }
        };
    }

    /// The reader function `src` as a function of (bytes, scan, row).
    pub(super) fn read_twin(&mut self, src: &IrFunction, tw: &ReadTwin) -> IrFunction {
        let mut body = src.body.clone();
        let mut root = src.params[0].var;
        let mut rn = Rename { vt: &mut *self.vt, map: HashMap::new() };
        rn.bind(&mut root);
        rn.visit_expr_mut(&mut body);
        let col_ty = Ty::list(Ty::Int);
        let b = self.vt.alloc(sym("bytes"), Ty::Bytes, Mutability::Let, None);
        let c = self.vt.alloc(sym("scan"), col_ty.clone(), Mutability::Let, None);
        let i = self.vt.alloc(sym("row"), Ty::Int, Mutability::Let, None);
        OverScan { cx: self, job: &tw.job, roots: Roots::from([(root, tw.base)]), b, c, i: Some(i) }.visit_expr_mut(&mut body);
        let params =
            vec![own_param(b, sym("bytes"), Ty::Bytes), own_param(c, sym("scan"), col_ty), own_param(i, sym("row"), Ty::Int)];
        new_fn(tw.name, params, src.ret_ty.clone(), body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn a_path_read_twice_is_one_node_and_one_slot() {
        let mut plan = Plan::new();
        plan.read_at(0, &p(&["meta", "score"]));
        plan.read_at(0, &p(&["meta", "score"]));
        plan.read_at(0, &p(&["meta", "name"]));
        assert_eq!(plan.keys, ["", "meta", "score", "name"]);
        assert_eq!(plan.par, [-1, 0, 1, 1]);
        assert_eq!(plan.slots(), (vec![-1, -1, 0, 1], 2, 0));
        assert_eq!(plan.paths(), [p(&["meta", "score"]), p(&["meta", "name"])]);
    }

    #[test]
    fn a_key_is_a_node_per_parent() {
        let mut plan = Plan::new();
        plan.read_at(0, &p(&["a", "id"]));
        plan.read_at(0, &p(&["b", "id"]));
        plan.read_at(0, &p(&["id"]));
        assert_eq!(plan.find(0, &p(&["a", "id"])), Some(2));
        assert_eq!(plan.find(0, &p(&["b", "id"])), Some(4));
        assert_eq!(plan.find(0, &p(&["id"])), Some(5));
        assert_eq!(plan.find(0, &p(&["c"])), None);
    }

    #[test]
    fn document_slots_come_before_row_slots() {
        let mut plan = Plan::new();
        // A read under the rows, met before a read of the document.
        let at = plan.node(0, &p(&["data"]));
        let el = plan.open_rows(at);
        plan.read_at(el, &p(&["id"]));
        plan.read_at(0, &p(&["total"]));
        plan.read_at(el, &p(&["meta", "on"]));
        assert_eq!(plan.row, Some(1));
        assert_eq!(plan.el, Some(2));
        assert_eq!(plan.par[2], -2);
        assert!(plan.under_el(3) && plan.under_el(6) && !plan.under_el(4) && !plan.under_el(1));
        // nodes: 0 root, 1 data, 2 element, 3 id, 4 total, 5 meta, 6 on
        assert_eq!(plan.slots(), (vec![-1, -1, -1, 1, 0, -1, 2], 1, 2));
    }

    #[test]
    fn no_key_leads_to_the_element_node() {
        let mut plan = Plan::new();
        let el = plan.open_rows(0);
        assert_eq!(plan.row, Some(0));
        plan.read_at(el, &p(&[""]));
        // The element node's own key is empty too, and is not found by it.
        assert_eq!(plan.find(0, &p(&[""])), None);
        assert_eq!(plan.find(el, &p(&[""])), Some(2));
    }

    #[test]
    fn the_scalar_members_of_the_accessor_family_are_the_leaves() {
        let names: Vec<_> = Leaf::ALL.iter().map(|l| (l.accessor(), l.kernel())).collect();
        assert_eq!(
            names,
            [
                ("get_int", "__leaf_int"),
                ("get_float", "__leaf_float"),
                ("get_bool", "__leaf_bool"),
                ("get_string", "__leaf_string")
            ]
        );
    }
}
