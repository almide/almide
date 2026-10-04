//! E008, generalized (ADR-0020 §3, #2697): no `var` is reachable from a body
//! that runs concurrently.
//!
//! A **site** is one argument expression in a *concurrent slot*: an arm of a
//! `fan { … }` block (and of the `fan.settle` / `fan.race` / `fan.bounded` /
//! `fan.timeout` block forms), every fn-valued argument of a `fan.<op>(…)`
//! call, a stdlib parameter named by `@concurrent(param, …)` (the `http`
//! handler slots), and — by inference — a parameter of a user fn that flows
//! into one of those (§3.1, "slot inference through user wrappers").
//!
//! The **executable closure** `X(E)` of a site (§3.2) is walked here: the
//! site's own body, every unfolded lambda in it, the bodies of the top-level
//! fns it names, and the unfolded lambdas and named fns of the `let`s it
//! reads. A lambda is *folded* into the body around it when it is a direct
//! argument of a non-concurrent stdlib call (`list.map` and the like), which
//! runs it before returning and never stores it.
//!
//! A site **reaches** a `var` when a body in `X(E)` names (reads or writes) a
//! `var` declared outside that body (§3.3). A `let` copy taken outside the
//! site is a snapshot: the var read in its initializer is not a reach.
//!
//! The analysis is purely syntactic plus scope resolution, so it runs on the
//! AST of any program. Facts that cross a module boundary — a fn's inferred
//! concurrent slots and whether its body reaches a `var` — are computed once
//! over the resolved module set ([`module_summaries`]) and carried in the
//! type environment, so a dependency's fn that touches its own top-level
//! `var` makes a downstream handler that calls it fail with a witness naming
//! that fn.

use std::collections::{HashMap, HashSet};

use almide_base::intern::{sym, Sym};
use almide_lang::ast::{self, Decl, Expr, ExprKind, Pattern, Program, Span, Stmt};

/// The `fan` surfaces whose fn-valued arguments are concurrent slots.
/// `__any_block` is the parser's spelling of `fan.any { … }`.
const FAN_OPS: &[&str] = &["map", "settle", "any", "any_map", "race", "timeout", "bounded", "__any_block"];

/// What kind of concurrent slot a site sits in; decides the diagnostic's
/// wording. `surface` is the stdlib spelling the writer used (`fan.map`,
/// `http.route`, or `fan` for the plain block).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SlotKind {
    /// A `fan { … }` arm (or an arm of a block-form fan head).
    FanBlock { surface: String },
    /// A fn-valued argument of a `fan.<op>(…)` call.
    FanCallback { surface: String },
    /// An `http` handler or middleware slot.
    Handler { surface: String },
}

/// One step of a witness path: a top-level fn the reach went through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hop {
    pub fn_name: String,
    pub module: Option<Sym>,
    pub line: Option<usize>,
}

/// Why a site (or a fn body) reaches a `var`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Witness {
    pub var: Sym,
    /// The module declaring the var (None = the program under analysis).
    pub var_module: Option<Sym>,
    /// The fns the reach went through, outermost first.
    pub path: Vec<Hop>,
    /// True when some statement may write the var (an assignment target, or
    /// passed as a call argument, which a `mut` parameter could write). A
    /// var nothing writes is fixed mechanically by declaring it `let`.
    pub maybe_assigned: bool,
}

/// The cross-module facts of one top-level fn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FnSummary {
    /// Inferred concurrent slots: (parameter index, the slot it flows into).
    pub slots: Vec<(usize, SlotKind)>,
    /// Some when the fn's body, as a body in `X`, reaches a `var`.
    pub reach: Option<Witness>,
}

/// module name → fn name → summary.
pub type Summaries = HashMap<Sym, HashMap<Sym, FnSummary>>;

/// A site that reaches a `var`.
#[derive(Clone, Debug)]
pub struct Finding {
    pub kind: SlotKind,
    /// The user fn the site is an argument of, when the slot was inferred
    /// through a wrapper (`serve_on(8080, h)`).
    pub wrapper: Option<String>,
    pub span: Option<Span>,
    pub witness: Witness,
}

/// What the analysis needs to know about the program's surroundings.
pub struct World<'a> {
    /// This program's module name (None = the entry program).
    pub module: Option<Sym>,
    /// alias → canonical module (the program's import table).
    pub aliases: &'a HashMap<Sym, Sym>,
    /// selectively imported bare name → canonical module.
    pub direct: &'a HashMap<Sym, Sym>,
    /// Facts about other user modules.
    pub ext: &'a Summaries,
    /// Whether an argument expression is fn-valued, from the checker's types.
    /// None when the type is not known (the module pre-pass runs before
    /// inference); the syntactic fallback decides then.
    pub arg_is_fn: &'a dyn Fn(&Expr) -> Option<bool>,
    /// Whether a declared parameter type is fn-valued.
    pub type_is_fn: &'a dyn Fn(&ast::TypeExpr) -> bool,
}

#[derive(Clone, Copy)]
enum Bind<'a> {
    Var { body: u32 },
    /// A binding whose value is `init` — followed as a snapshot.
    Let { body: u32, init: &'a Expr },
    /// A parameter of the top-level fn being discovered (`index`), or a
    /// lambda / pattern binder (`None`).
    Param { index: Option<usize>, fn_typed: bool },
}

type Scope<'a> = Vec<(Sym, Bind<'a>)>;

fn lookup<'s, 'a>(scope: &'s Scope<'a>, name: Sym) -> Option<&'s Bind<'a>> {
    scope.iter().rev().find(|(n, _)| *n == name).map(|(_, b)| b)
}

struct TopFn<'a> {
    params: &'a [ast::Param],
    body: &'a Expr,
    line: Option<usize>,
    attrs: &'a [ast::Attribute],
}

/// Mode of the walk: discovering sites in a fn body, or collecting the
/// reaches of one site's executable closure.
#[derive(Clone, Copy)]
enum Mode {
    Discover,
    /// `body` is the body being walked; `live` is false while evaluating a
    /// `let` initializer outside the site (a var read there is a snapshot).
    Reach { body: u32, live: bool },
}

pub struct Analyzer<'a> {
    w: World<'a>,
    top_vars: HashSet<Sym>,
    top_lets: HashMap<Sym, &'a Expr>,
    fns: HashMap<Sym, TopFn<'a>>,
    /// Names written somewhere in the program (see `Witness::maybe_assigned`),
    /// collected on the first witness: most programs never build one (#3340).
    written: std::cell::OnceCell<HashSet<Sym>>,
    prog: &'a Program,
    /// Inferred slots of this program's fns.
    pub slots: HashMap<Sym, Vec<(usize, SlotKind)>>,
    fn_memo: HashMap<Sym, Option<Witness>>,
    in_progress: HashSet<Sym>,
    next_body: u32,
    /// The top-level fn whose body discovery is in.
    current_fn: Option<Sym>,
    /// Discovery records findings only on its final round.
    record: bool,
    slots_changed: bool,
    findings: Vec<Finding>,
    /// Reach accumulator for the site being walked.
    reached: Vec<Witness>,
    visited_inits: HashSet<u32>,
    reported: HashSet<(Sym, Option<Sym>)>,
    /// Some fn of this program is named `Type.method` (see `method_fn`).
    has_dotted_fns: bool,
    /// The cheap syntactic pre-scan (`Shape`), taken once in `new`.
    shape: Shape,
}

/// What one pass over the program's syntax rules out before the walk (#3340).
/// Both flags are conservative: `false` is a proof, `true` only "maybe".
#[derive(Clone, Copy, Default)]
struct Shape {
    /// A `fan` form, a declared `@concurrent` fn, or a name that can resolve
    /// to a callee with concurrent slots appears. When false no argument of
    /// the program sits in a concurrent slot, so slot inference infers
    /// nothing and discovery records nothing.
    may_have_sites: bool,
    /// A `var` (top-level or local) is declared, a `fan` form appears (the
    /// AST visitor is shallow in those), or a module this program reads
    /// facts from has a fn that reaches a `var`. When false no body of the
    /// program reaches a `var`: every fn's reach is None.
    may_reach: bool,
}

impl<'a> Analyzer<'a> {
    pub fn new(prog: &'a Program, w: World<'a>) -> Self {
        let mut a = Analyzer {
            w,
            top_vars: HashSet::new(),
            top_lets: HashMap::new(),
            fns: HashMap::new(),
            written: std::cell::OnceCell::new(),
            prog,
            slots: HashMap::new(),
            fn_memo: HashMap::new(),
            in_progress: HashSet::new(),
            next_body: 1,
            current_fn: None,
            record: false,
            slots_changed: false,
            findings: Vec::new(),
            reached: Vec::new(),
            visited_inits: HashSet::new(),
            reported: HashSet::new(),
            has_dotted_fns: false,
            shape: Shape::default(),
        };
        for d in &prog.decls {
            match d {
                Decl::TopLet { name, mutable: true, .. } => {
                    a.top_vars.insert(*name);
                }
                Decl::TopLet { name, value, .. } => {
                    a.top_lets.insert(*name, value);
                }
                Decl::Fn { name, params, body: Some(body), span, attrs, .. } => {
                    a.fns.insert(*name, TopFn { params, body, line: span.map(|s| s.line), attrs });
                }
                _ => {}
            }
        }
        // A `@concurrent(p)` a program declares itself is a declared slot.
        for (name, f) in &a.fns {
            let declared = declared_slots_of(f.attrs, f.params);
            if !declared.is_empty() {
                let kind = SlotKind::FanCallback { surface: name.to_string() };
                a.slots.insert(*name, declared.into_iter().map(|i| (i, kind.clone())).collect());
            }
        }
        a.has_dotted_fns = a.fns.keys().any(|k| k.as_str().contains('.'));
        a.shape = a.scan_shape(prog);
        a
    }

    /// The pre-scan behind `Shape` (#3340).
    fn scan_shape(&self, prog: &Program) -> Shape {
        let mut shape = Shape { may_have_sites: !self.slots.is_empty(), may_reach: !self.top_vars.is_empty() };
        // A module this program can name (`import m` / `import m.{f}`)
        // carries facts the walk reads through `ext` (`call_slots`,
        // `ext_ref`); every such read starts from these two maps.
        for &m in self.w.aliases.values().chain(self.w.direct.values()) {
            if self.module_has_slots(m) {
                shape.may_have_sites = true;
            }
            if self.w.ext.get(&m).is_some_and(|fs| fs.values().any(|s| s.reach.is_some())) {
                shape.may_reach = true;
            }
        }
        let heads = slot_heads();
        let mut see = |e: &Expr| match &e.kind {
            ExprKind::Fan { .. }
            | ExprKind::FanSettle { .. }
            | ExprKind::FanRace { .. }
            | ExprKind::FanBounded { .. }
            | ExprKind::FanTimeout { .. }
            | ExprKind::FanRaceMap { .. } => {
                shape.may_have_sites = true;
                shape.may_reach = true;
            }
            // `fan.map(…)`, `http.serve(…)`: a callee head spelled as the
            // module itself, which `resolve_callee` honours with no import.
            ExprKind::Ident { name } => {
                if heads.contains(name) {
                    shape.may_have_sites = true;
                }
            }
            ExprKind::Block { stmts, .. } | ExprKind::ForIn { body: stmts, .. } | ExprKind::While { body: stmts, .. } => {
                if stmts.iter().any(|s| matches!(s, Stmt::Var { .. })) {
                    shape.may_reach = true;
                }
            }
            _ => {}
        };
        for d in &prog.decls {
            match d {
                Decl::Fn { body: Some(body), .. } => ast::visit_expr(body, &mut see),
                Decl::TopLet { value, .. } => ast::visit_expr(value, &mut see),
                Decl::Test { body, .. } => ast::visit_expr(body, &mut see),
                _ => {}
            }
        }
        shape
    }

    /// Whether a call resolved into module `m` can have concurrent slots:
    /// `fan`, a stdlib module that declares `@concurrent`, or a user module
    /// with a fn whose slots are known.
    fn module_has_slots(&self, m: Sym) -> bool {
        slot_heads().contains(&m)
            || self.w.ext.get(&m).is_some_and(|fs| fs.values().any(|s| !s.slots.is_empty()))
    }

    /// Run slot inference to a fixpoint, then collect every site that
    /// reaches a `var`.
    pub fn run(mut self, prog: &'a Program) -> (Vec<Finding>, HashMap<Sym, Vec<(usize, SlotKind)>>) {
        if !self.shape.may_have_sites {
            // No site: nothing to infer and nothing to record (`Shape`).
            return (self.findings, self.slots);
        }
        self.infer_slots(prog);
        self.record = true;
        self.discover_program(prog);
        (self.findings, self.slots)
    }

    /// Slot inference only (§3.1), to a fixpoint.
    pub fn infer_slots(&mut self, prog: &'a Program) {
        if !self.shape.may_have_sites {
            return;
        }
        let saved = self.record;
        self.record = false;
        for _ in 0..=self.fns.len() {
            self.slots_changed = false;
            self.discover_program(prog);
            if !self.slots_changed {
                break;
            }
        }
        self.record = saved;
    }

    /// The cross-module summary of every fn of this program.
    pub fn summaries(&mut self) -> HashMap<Sym, FnSummary> {
        let names: Vec<Sym> = self.fns.keys().copied().collect();
        names
            .into_iter()
            .map(|n| {
                let reach = if self.shape.may_reach { self.fn_reach(n) } else { None };
                let slots = self.slots.get(&n).cloned().unwrap_or_default();
                (n, FnSummary { slots, reach })
            })
            .collect()
    }

    fn discover_program(&mut self, prog: &'a Program) {
        for d in &prog.decls {
            let mut scope: Scope<'a> = Vec::new();
            match d {
                Decl::Fn { name, params, body: Some(body), .. } => {
                    self.current_fn = Some(*name);
                    for (i, p) in params.iter().enumerate() {
                        let fn_typed = (self.w.type_is_fn)(&p.ty);
                        scope.push((p.name, Bind::Param { index: Some(i), fn_typed }));
                    }
                    self.walk(body, &mut scope, Mode::Discover);
                }
                Decl::TopLet { value, .. } => {
                    self.current_fn = None;
                    self.walk(value, &mut scope, Mode::Discover);
                }
                Decl::Test { body, .. } => {
                    self.current_fn = None;
                    self.walk(body, &mut scope, Mode::Discover);
                }
                _ => {}
            }
        }
        self.current_fn = None;
    }

    fn fresh_body(&mut self) -> u32 {
        self.next_body += 1;
        self.next_body
    }

    // ── Name resolution ─────────────────────────────────────────────────

    fn module_of_alias(&self, alias: Sym) -> Option<Sym> {
        self.w.aliases.get(&alias).copied()
    }

    fn is_user_module(&self, m: Sym) -> bool {
        self.w.ext.contains_key(&m) || !almide_lang::stdlib_info::is_stdlib_module(m.as_str())
    }

    /// A dotted member chain (`a.b.c`) spelled as a module path, if it is one.
    fn member_path(e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Ident { name } => Some(name.to_string()),
            ExprKind::Member { object, field } => Self::member_path(object).map(|p| format!("{p}.{field}")),
            _ => None,
        }
    }

    fn resolve_callee(&self, callee: &Expr, scope: &Scope<'a>) -> Callee {
        match &callee.kind {
            ExprKind::Paren { expr } => self.resolve_callee(expr, scope),
            ExprKind::Ident { name } => {
                if lookup(scope, *name).is_some() {
                    Callee::Value
                } else if self.fns.contains_key(name) {
                    Callee::Local(*name)
                } else if let Some(m) = self.w.direct.get(name) {
                    self.module_callee(*m, *name)
                } else {
                    Callee::Unknown
                }
            }
            ExprKind::Member { object, field } => {
                if let ExprKind::Ident { name } = &object.kind {
                    if lookup(scope, *name).is_none() && !self.fns.contains_key(name) {
                        if name.as_str() == "fan" {
                            return Callee::Fan(*field);
                        }
                        if let Some(m) = self.module_of_alias(*name) {
                            return self.module_callee(m, *field);
                        }
                        if almide_lang::stdlib_info::is_stdlib_module(name.as_str()) {
                            return Callee::Stdlib(*name, *field);
                        }
                    }
                }
                if let Some(path) = Self::member_path(object) {
                    if let Some(m) = self.module_of_alias(sym(&path)) {
                        return self.module_callee(m, *field);
                    }
                }
                // A method call on a value (UFCS): a local fn of that name,
                // else a stdlib method.
                match self.method_fn(*field) {
                    Some(f) => Callee::Local(f),
                    None => Callee::Method,
                }
            }
            _ => Callee::Unknown,
        }
    }

    fn method_fn(&self, field: Sym) -> Option<Sym> {
        if self.fns.contains_key(&field) {
            return Some(field);
        }
        // No dotted (`Type.method`) fn in this program: nothing can match the
        // suffix scan below, so skip building it per method call.
        if !self.has_dotted_fns {
            return None;
        }
        let suffix = format!(".{field}");
        let mut hits = self.fns.keys().filter(|k| k.as_str().ends_with(&suffix));
        hits.next().copied()
    }

    fn module_callee(&self, m: Sym, f: Sym) -> Callee {
        if m.as_str() == "fan" {
            Callee::Fan(f)
        } else if self.is_user_module(m) && self.w.module != Some(m) {
            Callee::Ext(m, f)
        } else if self.w.module == Some(m) && self.fns.contains_key(&f) {
            Callee::Local(f)
        } else {
            Callee::Stdlib(m, f)
        }
    }

    /// The concurrent slots of a call, as (argument index, kind).
    fn call_slots(&self, callee: &Callee, args: &[&'a Expr], scope: &Scope<'a>) -> Vec<(usize, SlotKind)> {
        match callee {
            Callee::Fan(op) if FAN_OPS.contains(&op.as_str()) => {
                let surface = match op.as_str() {
                    "__any_block" => "fan.any".to_string(),
                    o => format!("fan.{o}"),
                };
                (0..args.len())
                    .filter(|&i| self.is_fn_valued(args[i], scope))
                    .map(|i| (i, SlotKind::FanCallback { surface: surface.clone() }))
                    .collect()
            }
            Callee::Stdlib(m, f) => {
                let declared = stdlib_declared_slots(m.as_str(), f.as_str());
                if declared.is_empty() {
                    return Vec::new();
                }
                let surface = format!("{m}.{f}");
                declared
                    .into_iter()
                    .map(|i| {
                        let kind = if m.as_str() == "http" {
                            SlotKind::Handler { surface: surface.clone() }
                        } else {
                            SlotKind::FanCallback { surface: surface.clone() }
                        };
                        (i, kind)
                    })
                    .collect()
            }
            Callee::Local(f) => self.slots.get(f).cloned().unwrap_or_default(),
            Callee::Ext(m, f) => self
                .w
                .ext
                .get(m)
                .and_then(|fs| fs.get(f))
                .map(|s| s.slots.clone())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    fn is_fn_valued(&self, e: &Expr, scope: &Scope<'a>) -> bool {
        if let Some(b) = (self.w.arg_is_fn)(e) {
            return b;
        }
        self.looks_fn_valued(e, scope, 0)
    }

    /// The syntactic fallback when no type is known: a lambda, a list of
    /// fn values, or a name bound to one.
    fn looks_fn_valued(&self, e: &Expr, scope: &Scope<'a>, depth: u32) -> bool {
        if depth > 8 {
            return false;
        }
        match &e.kind {
            ExprKind::Lambda { .. } => true,
            ExprKind::Paren { expr } => self.looks_fn_valued(expr, scope, depth + 1),
            ExprKind::List { elements } => elements.iter().any(|x| self.looks_fn_valued(x, scope, depth + 1)),
            ExprKind::Ident { name } => match lookup(scope, *name) {
                Some(Bind::Param { fn_typed, .. }) => *fn_typed,
                Some(Bind::Let { init, .. }) => self.looks_fn_valued(init, scope, depth + 1),
                Some(Bind::Var { .. }) => false,
                None => self.fns.contains_key(name) || self.w.direct.contains_key(name),
            },
            ExprKind::Member { object, .. } => Self::member_path(object)
                .and_then(|p| self.module_of_alias(sym(&p)))
                .is_some(),
            _ => false,
        }
    }

    /// Whether a lambda argument of this call is folded into the body around it.
    fn folds(&self, callee: &Callee, index: usize) -> bool {
        match callee {
            Callee::Stdlib(m, f) => {
                !matches!(m.as_str(), "fan" | "http")
                    && !stdlib_declared_slots(m.as_str(), f.as_str()).contains(&index)
            }
            Callee::Method => true,
            _ => false,
        }
    }

    // ── The walk ────────────────────────────────────────────────────────

    fn walk(&mut self, e: &'a Expr, scope: &mut Scope<'a>, mode: Mode) {
        match &e.kind {
            ExprKind::Ident { name } | ExprKind::TypeName { name } => self.name_ref(*name, scope, mode),
            ExprKind::Lambda { params, body } => self.lambda(params, body, scope, mode, false),
            ExprKind::Call { callee, args, named_args, .. } => {
                let args: Vec<&'a Expr> = args.iter().chain(named_args.iter().map(|(_, a)| a)).collect();
                self.call(callee, &args, scope, mode);
            }
            ExprKind::Pipe { left, right } => match &right.kind {
                ExprKind::Call { callee, args, named_args, .. } => {
                    let args: Vec<&'a Expr> = std::iter::once(&**left)
                        .chain(args.iter())
                        .chain(named_args.iter().map(|(_, a)| a))
                        .collect();
                    self.call(callee, &args, scope, mode);
                }
                _ => {
                    let args = [&**left];
                    self.call(right, &args, scope, mode);
                }
            },
            ExprKind::Block { stmts, expr } => self.block(stmts, expr.as_deref(), scope, mode),
            ExprKind::Match { subject, arms } => {
                self.walk(subject, scope, mode);
                for arm in arms {
                    let mark = scope.len();
                    let body = mode_body(mode);
                    for n in pattern_binders(&arm.pattern) {
                        scope.push((n, Bind::Let { body, init: subject }));
                    }
                    if let Some(g) = &arm.guard {
                        self.walk(g, scope, mode);
                    }
                    self.walk(&arm.body, scope, mode);
                    scope.truncate(mark);
                }
            }
            ExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.walk(iterable, scope, mode);
                let mark = scope.len();
                let b = mode_body(mode);
                scope.push((*var, Bind::Let { body: b, init: iterable }));
                for n in var_tuple.iter().flatten() {
                    scope.push((*n, Bind::Let { body: b, init: iterable }));
                }
                self.stmts(body, scope, mode);
                scope.truncate(mark);
            }
            ExprKind::While { cond, body } => {
                self.walk(cond, scope, mode);
                let mark = scope.len();
                self.stmts(body, scope, mode);
                scope.truncate(mark);
            }
            ExprKind::IfLet { name, scrutinee, then, else_ } => {
                self.walk(scrutinee, scope, mode);
                let mark = scope.len();
                scope.push((*name, Bind::Let { body: mode_body(mode), init: scrutinee }));
                self.walk(then, scope, mode);
                scope.truncate(mark);
                self.walk(else_, scope, mode);
            }
            ExprKind::Fan { exprs } => self.fan_arms(exprs.iter(), "fan", scope, mode),
            ExprKind::FanSettle { arms } => self.fan_arms(arms.iter(), "fan.settle", scope, mode),
            ExprKind::FanRace { budget, arms } => {
                if let Some(b) = budget {
                    self.walk(b, scope, mode);
                }
                self.fan_arms(arms.iter(), "fan.race", scope, mode);
            }
            ExprKind::FanBounded { budget, body } => {
                self.walk(budget, scope, mode);
                self.fan_arms(std::iter::once(&**body), "fan.bounded", scope, mode);
            }
            ExprKind::FanTimeout { deadline, body } => {
                self.walk(deadline, scope, mode);
                self.fan_arms(std::iter::once(&**body), "fan.timeout", scope, mode);
            }
            ExprKind::FanRaceMap { budget, list, mapper } => {
                if let Some(b) = budget {
                    self.walk(b, scope, mode);
                }
                self.walk(list, scope, mode);
                let kind = SlotKind::FanCallback { surface: "fan.race".to_string() };
                self.site(mapper, kind, None, scope, mode);
            }
            ExprKind::Member { object, field } => {
                // `m.f` naming another module's fn as a value
                if let ExprKind::Ident { name } = &object.kind {
                    if lookup(scope, *name).is_none() {
                        if let Some(m) = self.module_of_alias(*name) {
                            if let Callee::Ext(m, f) = self.module_callee(m, *field) {
                                self.ext_ref(m, f, mode);
                                return;
                            }
                        }
                    }
                }
                self.walk(object, scope, mode);
            }
            ExprKind::Scoped { body: x, .. }
            | ExprKind::TupleIndex { object: x, .. }
            | ExprKind::Unary { operand: x, .. }
            | ExprKind::Try { expr: x }
            | ExprKind::Unwrap { expr: x }
            | ExprKind::ToOption { expr: x }
            | ExprKind::Paren { expr: x }
            | ExprKind::Some { expr: x }
            | ExprKind::Ok { expr: x }
            | ExprKind::Err { expr: x }
            | ExprKind::OptionalChain { expr: x, .. }
            | ExprKind::TypeAscription { expr: x, .. } => self.walk(x, scope, mode),
            ExprKind::Binary { left: a, right: b, .. }
            | ExprKind::Compose { left: a, right: b }
            | ExprKind::UnwrapOr { expr: a, fallback: b }
            | ExprKind::IndexAccess { object: a, index: b }
            | ExprKind::Range { start: a, end: b, .. } => {
                self.walk(a, scope, mode);
                self.walk(b, scope, mode);
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk(cond, scope, mode);
                self.walk(then, scope, mode);
                self.walk(else_, scope, mode);
            }
            ExprKind::List { elements } | ExprKind::Tuple { elements } => {
                for x in elements {
                    self.walk(x, scope, mode);
                }
            }
            ExprKind::MapLiteral { entries } => {
                for (k, v) in entries {
                    self.walk(k, scope, mode);
                    self.walk(v, scope, mode);
                }
            }
            ExprKind::Record { fields, .. } => {
                for f in fields {
                    self.walk(&f.value, scope, mode);
                }
            }
            ExprKind::SpreadRecord { base, fields } => {
                self.walk(base, scope, mode);
                for f in fields {
                    self.walk(&f.value, scope, mode);
                }
            }
            ExprKind::InterpolatedString { parts, .. } => {
                for p in parts {
                    if let ast::StringPart::Expr { expr } = p {
                        self.walk(expr, scope, mode);
                    }
                }
            }
            ExprKind::Int { .. }
            | ExprKind::Float { .. }
            | ExprKind::String { .. }
            | ExprKind::Bool { .. }
            | ExprKind::EmptyMap
            | ExprKind::Hole
            | ExprKind::Todo { .. }
            | ExprKind::Break
            | ExprKind::Continue
            | ExprKind::Placeholder
            | ExprKind::Unit
            | ExprKind::None
            | ExprKind::Error => {}
        }
    }

    fn block(&mut self, stmts: &'a [Stmt], tail: Option<&'a Expr>, scope: &mut Scope<'a>, mode: Mode) {
        let mark = scope.len();
        self.stmts(stmts, scope, mode);
        if let Some(t) = tail {
            self.walk(t, scope, mode);
        }
        scope.truncate(mark);
    }

    /// A statement list; its bindings stay in `scope` (the caller truncates).
    fn stmts(&mut self, stmts: &'a [Stmt], scope: &mut Scope<'a>, mode: Mode) {
        let body = mode_body(mode);
        for s in stmts {
            match s {
                Stmt::Let { name, value, .. } => {
                    self.walk(value, scope, mode);
                    scope.push((*name, Bind::Let { body, init: value }));
                }
                Stmt::Var { name, value, .. } => {
                    self.walk(value, scope, mode);
                    scope.push((*name, Bind::Var { body }));
                }
                Stmt::LetDestructure { pattern, value, .. } => {
                    self.walk(value, scope, mode);
                    for n in pattern_binders(pattern) {
                        scope.push((n, Bind::Let { body, init: value }));
                    }
                }
                Stmt::GuardLet { name, scrutinee, else_, .. } => {
                    self.walk(scrutinee, scope, mode);
                    self.walk(else_, scope, mode);
                    scope.push((*name, Bind::Let { body, init: scrutinee }));
                }
                Stmt::Assign { name, value, .. } => {
                    self.name_ref(*name, scope, mode);
                    self.walk(value, scope, mode);
                }
                Stmt::IndexAssign { target, index, value, .. } => {
                    self.name_ref(*target, scope, mode);
                    self.walk(index, scope, mode);
                    self.walk(value, scope, mode);
                }
                Stmt::FieldAssign { target, value, .. } => {
                    self.name_ref(*target, scope, mode);
                    self.walk(value, scope, mode);
                }
                Stmt::Guard { cond, else_, .. } => {
                    self.walk(cond, scope, mode);
                    self.walk(else_, scope, mode);
                }
                Stmt::Expr { expr, .. } => self.walk(expr, scope, mode),
                Stmt::Comment { .. } | Stmt::Error { .. } => {}
            }
        }
    }

    fn lambda(&mut self, params: &'a [ast::LambdaParam], body: &'a Expr, scope: &mut Scope<'a>, mode: Mode, folded: bool) {
        let mark = scope.len();
        for p in params {
            scope.push((p.name, Bind::Param { index: None, fn_typed: false }));
            for n in p.tuple_names.iter().flatten() {
                scope.push((*n, Bind::Param { index: None, fn_typed: false }));
            }
        }
        let inner = match mode {
            Mode::Reach { body: b, live } if folded => Mode::Reach { body: b, live },
            // An unfolded lambda is its own body, and it may be called: live.
            Mode::Reach { .. } => Mode::Reach { body: self.fresh_body(), live: true },
            Mode::Discover => Mode::Discover,
        };
        self.walk(body, scope, inner);
        scope.truncate(mark);
    }

    fn call(&mut self, callee: &'a Expr, args: &[&'a Expr], scope: &mut Scope<'a>, mode: Mode) {
        let resolved = self.resolve_callee(callee, scope);
        // The callee itself: a name reference (a fn body joins `X`).
        match &callee.kind {
            ExprKind::Member { object, .. } => {
                if matches!(resolved, Callee::Ext(..)) {
                    if let Callee::Ext(m, f) = resolved {
                        self.ext_ref(m, f, mode);
                    }
                } else if !matches!(resolved, Callee::Fan(_) | Callee::Stdlib(..)) {
                    self.walk(object, scope, mode);
                }
            }
            _ => self.walk(callee, scope, mode),
        }
        let slots = match mode {
            Mode::Discover => self.call_slots(&resolved, args, scope),
            Mode::Reach { .. } => Vec::new(),
        };
        let wrapper = match &resolved {
            Callee::Local(f) | Callee::Ext(_, f) => Some(f.to_string()),
            _ => None,
        };
        for (i, a) in args.iter().enumerate() {
            if let Some((_, kind)) = slots.iter().find(|(j, _)| *j == i) {
                self.site(a, kind.clone(), wrapper.clone(), scope, mode);
                continue;
            }
            match (&a.kind, self.folds(&resolved, i)) {
                (ExprKind::Lambda { params, body }, true) => self.lambda(params, body, scope, mode, true),
                _ => self.walk(a, scope, mode),
            }
        }
    }

    fn fan_arms(&mut self, arms: impl Iterator<Item = &'a Expr>, surface: &str, scope: &mut Scope<'a>, mode: Mode) {
        for arm in arms {
            let kind = SlotKind::FanBlock { surface: surface.to_string() };
            self.site(arm, kind, None, scope, mode);
        }
    }

    /// One argument in a concurrent slot.
    fn site(&mut self, e: &'a Expr, kind: SlotKind, wrapper: Option<String>, scope: &mut Scope<'a>, mode: Mode) {
        match mode {
            Mode::Reach { .. } => {
                // Inside another site's closure an arm is its own body.
                let inner = Mode::Reach { body: self.fresh_body(), live: true };
                self.walk(e, scope, inner);
            }
            Mode::Discover => {
                if let Some(index) = self.flows_from_param(e, scope, 0) {
                    if let Some(f) = self.current_fn {
                        self.add_slot(f, index, kind);
                    }
                } else if self.record {
                    self.check_site(e, kind, wrapper, scope);
                }
                // Sites nested inside the argument are discovered too.
                self.walk(e, scope, Mode::Discover);
            }
        }
    }

    /// `Some(i)` when the argument, after following `let`s, is parameter `i`
    /// of the enclosing top-level fn: the slot moves to that fn's callers.
    fn flows_from_param(&self, e: &'a Expr, scope: &Scope<'a>, depth: u32) -> Option<usize> {
        if depth > 8 {
            return None;
        }
        match &e.kind {
            ExprKind::Paren { expr } => self.flows_from_param(expr, scope, depth + 1),
            ExprKind::Ident { name } => match lookup(scope, *name)? {
                Bind::Param { index: Some(i), .. } => Some(*i),
                Bind::Let { init, .. } => self.flows_from_param(init, scope, depth + 1),
                _ => None,
            },
            _ => None,
        }
    }

    fn add_slot(&mut self, f: Sym, index: usize, kind: SlotKind) {
        let entry = self.slots.entry(f).or_default();
        if !entry.iter().any(|(i, _)| *i == index) {
            entry.push((index, kind));
            self.slots_changed = true;
        }
    }

    fn check_site(&mut self, e: &'a Expr, kind: SlotKind, wrapper: Option<String>, scope: &mut Scope<'a>) {
        let saved = std::mem::take(&mut self.reached);
        self.visited_inits.clear();
        let body = self.fresh_body();
        self.walk(e, scope, Mode::Reach { body, live: true });
        let reached = std::mem::replace(&mut self.reached, saved);
        for w in reached {
            // One diagnostic per var: the first (most direct) site names it,
            // and declaring it otherwise fixes every site that reaches it.
            if self.reported.insert((w.var, w.var_module)) {
                self.findings.push(Finding { kind: kind.clone(), wrapper: wrapper.clone(), span: e.span, witness: w });
            }
        }
    }

    /// A reference to `name` (read or write) in the current mode.
    fn name_ref(&mut self, name: Sym, scope: &mut Scope<'a>, mode: Mode) {
        let Mode::Reach { body, live } = mode else { return };
        match lookup(scope, name).copied() {
            Some(Bind::Var { body: owner }) => {
                if live && owner != body {
                    let w = self.witness(name, None);
                    self.reached.push(w);
                }
            }
            Some(Bind::Let { body: owner, init }) => {
                if owner != body && self.visited_inits.insert(init.id.0) {
                    self.walk(init, scope, Mode::Reach { body: owner, live: false });
                }
            }
            Some(Bind::Param { .. }) => {}
            None => self.top_ref(name, scope, live),
        }
    }

    fn top_ref(&mut self, name: Sym, scope: &mut Scope<'a>, live: bool) {
        if self.top_vars.contains(&name) {
            if live {
                let w = self.witness(name, None);
                self.reached.push(w);
            }
        } else if let Some(init) = self.top_lets.get(&name).copied() {
            if self.visited_inits.insert(init.id.0) {
                let mut fresh: Scope<'a> = Vec::new();
                let body = self.fresh_body();
                self.walk(init, &mut fresh, Mode::Reach { body, live: false });
            }
        } else if self.fns.contains_key(&name) {
            if let Some(mut w) = self.fn_reach(name) {
                let line = self.fns.get(&name).and_then(|f| f.line);
                w.path.insert(0, Hop { fn_name: name.to_string(), module: None, line });
                self.reached.push(w);
            }
        } else if let Some(m) = self.w.direct.get(&name).copied() {
            if self.is_user_module(m) {
                self.ext_ref(m, name, Mode::Reach { body: 0, live });
            }
        }
        let _ = scope;
    }

    fn ext_ref(&mut self, m: Sym, f: Sym, mode: Mode) {
        if !matches!(mode, Mode::Reach { .. }) {
            return;
        }
        if let Some(w) = self.w.ext.get(&m).and_then(|fs| fs.get(&f)).and_then(|s| s.reach.clone()) {
            let mut w = w;
            if w.var_module.is_none() {
                w.var_module = Some(m);
            }
            for hop in w.path.iter_mut() {
                if hop.module.is_none() {
                    hop.module = Some(m);
                }
            }
            w.path.insert(0, Hop { fn_name: format!("{m}.{f}"), module: Some(m), line: None });
            self.reached.push(w);
        }
    }

    /// Whether top-level fn `name`'s body, as a body in `X`, reaches a var.
    fn fn_reach(&mut self, name: Sym) -> Option<Witness> {
        if let Some(r) = self.fn_memo.get(&name) {
            return r.clone();
        }
        if !self.in_progress.insert(name) {
            return None;
        }
        let saved = std::mem::take(&mut self.reached);
        let saved_inits = std::mem::take(&mut self.visited_inits);
        let (params, body) = match self.fns.get(&name) {
            Some(f) => (f.params, f.body),
            None => {
                self.in_progress.remove(&name);
                return None;
            }
        };
        let mut scope: Scope<'a> = params
            .iter()
            .map(|p| (p.name, Bind::Param { index: None, fn_typed: (self.w.type_is_fn)(&p.ty) }))
            .collect();
        let b = self.fresh_body();
        // The fn's own vars are per invocation: they belong to body `b`.
        self.walk(body, &mut scope, Mode::Reach { body: b, live: true });
        let found = std::mem::replace(&mut self.reached, saved);
        self.visited_inits = saved_inits;
        self.in_progress.remove(&name);
        let r = found.into_iter().next();
        self.fn_memo.insert(name, r.clone());
        r
    }

    fn witness(&self, var: Sym, var_module: Option<Sym>) -> Witness {
        let written = self.written.get_or_init(|| {
            let mut out = HashSet::new();
            for d in &self.prog.decls {
                match d {
                    Decl::TopLet { value, .. } => collect_written(value, &mut out),
                    Decl::Fn { body: Some(body), .. } | Decl::Test { body, .. } => collect_written(body, &mut out),
                    _ => {}
                }
            }
            out
        });
        Witness { var, var_module, path: Vec::new(), maybe_assigned: written.contains(&var) }
    }
}

#[derive(Clone, Copy)]
enum Callee {
    Fan(Sym),
    Stdlib(Sym, Sym),
    Local(Sym),
    Ext(Sym, Sym),
    /// A method call on a value, resolved to no local fn: a stdlib method.
    Method,
    /// A local closure value.
    Value,
    Unknown,
}

fn mode_body(mode: Mode) -> u32 {
    match mode {
        Mode::Reach { body, .. } => body,
        Mode::Discover => 0,
    }
}

fn pattern_binders(p: &Pattern) -> Vec<Sym> {
    let mut out = Vec::new();
    fn go(p: &Pattern, out: &mut Vec<Sym>) {
        match p {
            Pattern::Ident { name } => out.push(*name),
            Pattern::As { name, inner } => {
                out.push(*name);
                go(inner, out);
            }
            Pattern::Constructor { args, .. } => args.iter().for_each(|a| go(a, out)),
            Pattern::RecordPattern { fields, .. } => {
                for f in fields {
                    match &f.pattern {
                        Some(p) => go(p, out),
                        None => out.push(f.name),
                    }
                }
            }
            Pattern::Tuple { elements } => elements.iter().for_each(|a| go(a, out)),
            Pattern::List { elements, rest } => {
                elements.iter().for_each(|a| go(a, out));
                if let Some(Some(r)) = rest {
                    out.push(*r);
                }
            }
            Pattern::Some { inner } | Pattern::Ok { inner } | Pattern::Err { inner } => go(inner, out),
            Pattern::Or { alts } => alts.iter().for_each(|a| go(a, out)),
            Pattern::Wildcard | Pattern::Literal { .. } | Pattern::None => {}
        }
    }
    go(p, &mut out);
    out
}

/// Every name that may be written: an assignment target, or an identifier
/// passed directly as a call argument (a `mut` parameter may write it).
fn collect_written(e: &Expr, out: &mut HashSet<Sym>) {
    ast::visit_expr(e, &mut |x| match &x.kind {
        ExprKind::Call { args, .. } => {
            for a in args {
                if let ExprKind::Ident { name } = &a.kind {
                    out.insert(*name);
                }
            }
        }
        ExprKind::Pipe { left, .. } => {
            if let ExprKind::Ident { name } = &left.kind {
                out.insert(*name);
            }
        }
        ExprKind::Block { stmts, .. } => collect_written_stmts(stmts, out),
        ExprKind::ForIn { body, .. } | ExprKind::While { body, .. } => collect_written_stmts(body, out),
        _ => {}
    });
}

fn collect_written_stmts(stmts: &[Stmt], out: &mut HashSet<Sym>) {
    for s in stmts {
        match s {
            Stmt::Assign { name, .. } => {
                out.insert(*name);
            }
            Stmt::IndexAssign { target, .. } | Stmt::FieldAssign { target, .. } => {
                out.insert(*target);
            }
            _ => {}
        }
    }
}

/// The parameter indices a fn's `@concurrent(p, …)` attribute names.
fn declared_slots_of(attrs: &[ast::Attribute], params: &[ast::Param]) -> Vec<usize> {
    let mut out = Vec::new();
    for a in attrs.iter().filter(|a| a.name.as_str() == "concurrent") {
        for arg in &a.args {
            let name = match &arg.value {
                ast::AttrValue::Ident { name } => name.to_string(),
                ast::AttrValue::String { value } => value.clone(),
                _ => continue,
            };
            if let Some(i) = params.iter().position(|p| p.name.as_str() == name) {
                if !out.contains(&i) {
                    out.push(i);
                }
            }
        }
    }
    out
}

/// Every bundled stdlib module's `@concurrent` declarations: module → fn →
/// declared slot indices. Only modules that declare at least one slot have an
/// entry. Built once; read without a lock on every call the walk resolves to
/// the stdlib (#3340: the per-call lock and key allocation showed in profiles).
fn stdlib_slot_table() -> &'static HashMap<&'static str, HashMap<String, Vec<usize>>> {
    use std::sync::OnceLock;
    static TABLE: OnceLock<HashMap<&'static str, HashMap<String, Vec<usize>>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut out = HashMap::new();
        for &module in almide_lang::stdlib_info::BUNDLED_MODULES {
            let Some(src) = crate::stdlib::get_bundled_source(module) else { continue };
            if !src.contains("@concurrent") {
                continue;
            }
            let Some(prog) = almide_lang::parse_cached(src) else { continue };
            let mut t = HashMap::new();
            for d in &prog.decls {
                if let Decl::Fn { name, attrs, params, .. } = d {
                    let slots = declared_slots_of(attrs, params);
                    if !slots.is_empty() {
                        t.insert(name.to_string(), slots);
                    }
                }
            }
            if !t.is_empty() {
                out.insert(module, t);
            }
        }
        out
    })
}

/// The concurrent slots a bundled stdlib fn declares with `@concurrent`.
pub fn stdlib_declared_slots(module: &str, func: &str) -> Vec<usize> {
    stdlib_slot_table().get(module).and_then(|t| t.get(func)).cloned().unwrap_or_default()
}

/// The names a call's head can spell to land on a callee with concurrent
/// slots: `fan`, and every bundled stdlib module whose source mentions
/// `@concurrent`. A text match, not a parse — a superset, which is all the
/// pre-scan needs — and interned, so the scan compares integers per
/// identifier instead of resolving each one to its text.
fn slot_heads() -> &'static [Sym] {
    use std::sync::OnceLock;
    static HEADS: OnceLock<Vec<Sym>> = OnceLock::new();
    HEADS.get_or_init(|| {
        let mut out = vec![sym("fan")];
        for &module in almide_lang::stdlib_info::BUNDLED_MODULES {
            if crate::stdlib::get_bundled_source(module).is_some_and(|src| src.contains("@concurrent")) {
                out.push(sym(module));
            }
        }
        out
    })
}

/// The cross-module facts of every user module, to a fixpoint over the
/// module set (a module's slots and reaches may depend on another's).
pub fn module_summaries(
    modules: &[(Sym, &Program, HashMap<Sym, Sym>, HashMap<Sym, Sym>)],
    type_is_fn: &dyn Fn(&ast::TypeExpr) -> bool,
) -> Summaries {
    let mut ext: Summaries = HashMap::new();
    let no_type = |_: &Expr| -> Option<bool> { None };
    // The modules each module's analysis can read facts of: every `ext`
    // lookup starts from a value of its alias or direct-import map (#3340).
    let deps: Vec<Vec<Sym>> = modules
        .iter()
        .map(|(_, _, aliases, direct)| {
            let mut d: Vec<Sym> = aliases.values().chain(direct.values()).copied().collect();
            d.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
            d.dedup();
            d
        })
        .collect();
    // The `ext` the previous round ran against. A module none of whose deps'
    // facts moved since then would compute exactly what it computed last
    // round — which is its entry in the current `ext` — so it is not re-run.
    // Same rounds, same fixpoint, same result as re-running every module.
    let mut before: Option<Summaries> = None;
    for _ in 0..=modules.len() {
        let mut next: Summaries = HashMap::new();
        for ((name, prog, aliases, direct), deps) in modules.iter().zip(&deps) {
            if let (Some(prev), Some(mine)) = (&before, ext.get(name)) {
                if deps.iter().all(|d| same_facts(prev.get(d), ext.get(d))) {
                    next.insert(*name, mine.clone());
                    continue;
                }
            }
            let w = World { module: Some(*name), aliases, direct, ext: &ext, arg_is_fn: &no_type, type_is_fn };
            let mut a = Analyzer::new(prog, w);
            a.infer_slots(prog);
            next.insert(*name, a.summaries());
        }
        if next == ext {
            break;
        }
        before = Some(std::mem::replace(&mut ext, next));
    }
    ext
}

/// Whether two rounds' facts about one module read the same to the walk. The
/// walk only ever looks a fn up (`ext.get(m).and_then(|fs| fs.get(f))`) and
/// takes its slots or reach, so a missing module, a missing fn and a fn with
/// no slots and no reach all read alike. (Whether the module is a key at all
/// does not matter either: `is_user_module` is already true for every key,
/// since no stdlib-named module is ever summarized.)
fn same_facts(a: Option<&HashMap<Sym, FnSummary>>, b: Option<&HashMap<Sym, FnSummary>>) -> bool {
    let empty = FnSummary::default();
    let covers = |x: Option<&HashMap<Sym, FnSummary>>, y: Option<&HashMap<Sym, FnSummary>>| {
        x.into_iter().flatten().all(|(f, s)| y.and_then(|y| y.get(f)).unwrap_or(&empty) == s)
    };
    covers(a, b) && covers(b, a)
}
