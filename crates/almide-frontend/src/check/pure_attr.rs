//! E092 (#3250): `@pure` is checked.
//!
//! The attribute was in `KNOWN_ATTRS` with no semantics, so a model that
//! wrote it got no diagnostic and no check, and believed a guarantee it did
//! not have. It now means what ADR-0027 §2 proposes for `pure fn`: the empty
//! effect set — no effect category, no output, no declared abort. A `@pure`
//! fn is rejected when it is an `effect fn` or an `@extern`, or when anything
//! it reaches calls an output/abort builtin (ADR-0022's six), a stdlib
//! function that carries a category (`almide_ir::effect`), or an `@extern`.
//! A language-defined trap (division by zero, an index out of bounds) is not
//! a declared abort and is not counted (ADR-0027 §4).
//!
//! The call graph is the checker's own resolution: every named call it
//! resolves and every fn it types as a value is recorded under the fn whose
//! body it sits in (lambdas included), keyed the way #2496 keys callers and
//! callees, so a module's fns are reached across files. A call through a
//! fn-typed parameter or local is transparent (ADR-0026 D1): the argument is
//! judged where it is written.
//!
//! A function handed to a `@pure` fn runs inside its promise (#4928): the
//! promise is the `@pure` fn's DECLARATION, so a call that passes it a
//! function — a lambda, or a fn named as a value — is judged against it
//! whoever the caller is. Each lambda's calls are also recorded under a key of
//! its own (the enclosing fn's key and the lambda's expression id), and every
//! function-valued argument of a call to a user fn is kept as an obligation;
//! post-solve, an obligation whose callee is `@pure` and whose argument
//! reaches an effect is E092 at the argument.
//!
//! A function value reaches the `@pure` fn the same way through a local and
//! through a parameter (#4928): a `let` / `var` / assignment of a function to
//! a local binds it (every value a name was ever bound to in the fn — a
//! union, so a shadowing inner `let` adds and never hides), and a parameter
//! handed on — as an argument, or called inside a lambda that is — is a
//! forward edge `(fn, param) -> (callee, slot)`. An obligation whose callee is
//! not `@pure` follows the forward edges of its slot (every edge when the slot
//! is not known) to the `@pure` fns they end at, and is judged against each.
//!
//! The entry program and its modules are inferred in an order the source does
//! not show, so — as for #2496 — a `@pure` fn stays pending and is re-judged
//! at every program's post-solve. A violation becomes visible while the file
//! holding its last missing call is inferred, and it is reported there,
//! anchored on the first site of the chain in that file.
use super::Checker;
use crate::ast;
use almide_base::diagnostic::Diagnostic;
use almide_base::intern::{Sym, sym};
use almide_base::span::Span;
use std::collections::{HashMap, HashSet, VecDeque};

/// ADR-0022's admitted builtins — the writes and aborts a plain `fn` may make
/// and a `@pure` fn may not — with what each does: the one builtin table.
use super::builtin_calls::OUTPUT_ABORT_BUILTINS;

/// What one recorded call reaches.
#[derive(Clone, Debug)]
pub(crate) enum PurityCallee {
    Builtin(&'static str),
    Stdlib(Sym, Sym),
    User(Sym),
}

/// Where a call was written.
#[derive(Clone, Debug, PartialEq)]
struct Site {
    file: Option<String>,
    span: Option<Span>,
}

/// A `@pure` fn not yet judged impure.
#[derive(Clone, Debug)]
struct PendingPure {
    key: Sym,
    name: Sym,
    at: Site,
}

/// Checker-wide (it survives the per-program swaps): the resolved calls of
/// every fn body, and the `@pure` fns still to judge.
#[derive(Default, Debug, Clone)]
pub(crate) struct PurityFacts {
    calls: HashMap<Sym, Vec<(PurityCallee, Site)>>,
    pending: Vec<PendingPure>,
    /// The lambdas being inferred, innermost last: a call is recorded under
    /// each of them as well as under the enclosing fn.
    lambdas: Vec<Sym>,
    /// Every `@pure` fn declared so far, with where it was declared.
    declared: HashMap<Sym, (Sym, Site)>,
    /// A function passed to a user fn, not yet judged: `(caller, callee,
    /// the callee's parameter it fills, argument, site)`.
    passed: Vec<(Sym, Sym, Option<Sym>, PurityCallee, Site)>,
    /// `(fn, local)` -> every function value the local was bound to there.
    bindings: HashMap<(Sym, Sym), FnValues>,
    /// A lambda's key -> the parameters of its enclosing fn it calls.
    lambda_param_calls: HashMap<Sym, Vec<Sym>>,
    /// `(fn, param)` -> each `(callee, slot)` the parameter is handed to.
    forwards: HashMap<(Sym, Sym), Vec<(Sym, Option<Sym>)>>,
}

/// The function values an expression may be: functions, and parameters of
/// the fn it sits in.
#[derive(Clone, Debug, Default)]
struct FnValues {
    fns: Vec<PurityCallee>,
    params: Vec<Sym>,
}

/// The chain from a `@pure` fn to the first impure thing it reaches.
struct Witness {
    /// One call site per hop: the call in the `@pure` fn's body, then the
    /// call in each fn it passes through, ending at the impure call.
    sites: Vec<Site>,
    path: Vec<Sym>,
    what: String,
    does: String,
}

impl Checker {
    /// Record a named call the checker is resolving, for the `@pure` check.
    pub(crate) fn record_purity_call(&mut self, name: &str) {
        if self.env.pure_fns_absent {
            return;
        }
        if self.env.lookup_var(name).is_some() {
            // A fn-typed local or parameter: what it is bound to is called.
            self.record_local_call(name);
            return;
        }
        let callee = match OUTPUT_ABORT_BUILTINS.iter().find(|(b, _)| *b == name) {
            Some((b, _)) => PurityCallee::Builtin(b),
            None => self.purity_callee(name),
        };
        // The whole call (`println("x")`), not the argument inferred last.
        self.record_purity_ref(callee, self.call_span_hint.or(self.current_span));
    }

    /// Record a fn used as a VALUE (`list.map(xs, helper)`): it runs where
    /// the receiving call runs, so it is an edge like a call.
    pub(crate) fn record_purity_ref(&mut self, callee: PurityCallee, span: Option<Span>) {
        let Some((caller, _)) = &self.current_fn else { return };
        let caller = *caller;
        let site = Site { file: self.source_file.clone(), span };
        for lambda in self.purity.lambdas.clone() {
            self.purity.calls.entry(lambda).or_default().push((callee.clone(), site.clone()));
        }
        self.purity.calls.entry(caller).or_default().push((callee, site));
    }

    /// A call through the local or parameter `name`: each function it was
    /// bound to is an edge, and each parameter it is (or aliases) is called by
    /// every enclosing lambda (a forward edge once that lambda is handed on).
    pub(crate) fn record_local_call(&mut self, name: &str) {
        if self.env.pure_fns_absent {
            return;
        }
        let Some(v) = self.local_fn_values(name) else { return };
        let span = self.call_span_hint.or(self.current_span);
        for f in v.fns {
            self.record_purity_ref(f, span);
        }
        for p in v.params {
            for lambda in self.purity.lambdas.clone() {
                let calls = self.purity.lambda_param_calls.entry(lambda).or_default();
                if !calls.contains(&p) {
                    calls.push(p);
                }
            }
        }
    }

    /// What the local or parameter `name` of the current fn may hold.
    fn local_fn_values(&self, name: &str) -> Option<FnValues> {
        let (caller, _) = self.current_fn.as_ref()?;
        let n = sym(name);
        let mut v = self.purity.bindings.get(&(*caller, n)).cloned().unwrap_or_default();
        if self.env.param_vars.contains(&n) && !v.params.contains(&n) {
            v.params.push(n);
        }
        Some(v)
    }

    /// `let name = value` / `var` / `name = value` in a fn body: a function
    /// value bound to a local is what a later use of the local passes or calls.
    pub(crate) fn record_purity_binding(&mut self, name: &str, value: &ast::Expr) {
        if self.env.pure_fns_absent {
            return;
        }
        let Some((caller, _)) = self.current_fn.as_ref() else { return };
        let caller = *caller;
        let v = self.fn_values(value);
        if v.fns.is_empty() && v.params.is_empty() {
            return;
        }
        let slot = self.purity.bindings.entry((caller, sym(name))).or_default();
        slot.fns.extend(v.fns);
        for p in v.params {
            if !slot.params.contains(&p) {
                slot.params.push(p);
            }
        }
    }

    /// The key a lambda's own calls are recorded under.
    fn lambda_key(&self, id: ast::ExprId) -> Option<Sym> {
        let (caller, _) = self.current_fn.as_ref()?;
        Some(sym(&format!("{caller}::<lambda#{}>", id.0)))
    }

    /// Entering a lambda's body: its calls are recorded under its own key too.
    pub(crate) fn enter_purity_lambda(&mut self, id: ast::ExprId) -> bool {
        if self.env.pure_fns_absent {
            return false;
        }
        let Some(key) = self.lambda_key(id) else { return false };
        self.purity.lambdas.push(key);
        true
    }

    pub(crate) fn exit_purity_lambda(&mut self, entered: bool) {
        if entered {
            self.purity.lambdas.pop();
        }
    }

    /// A call to `callee` with `args` (`receiver` first for a method-form
    /// call) and `named` arguments: every argument that is a function — a
    /// lambda, a fn named as a value, a local bound to one — is an obligation,
    /// judged post-solve if `callee` is `@pure` or hands that parameter on to
    /// one; a parameter of the caller passed here is a forward edge.
    pub(crate) fn record_purity_args(&mut self, callee: &ast::Expr, receiver: Option<&ast::Expr>, args: &[ast::Expr], named: &[(Sym, ast::Expr)]) {
        if self.env.pure_fns_absent || self.current_fn.is_none() {
            return;
        }
        let name = match &callee.kind {
            ast::ExprKind::Ident { name, .. } if self.env.lookup_var(name).is_none() => name.to_string(),
            ast::ExprKind::Member { object, field } => match &object.kind {
                ast::ExprKind::Ident { name, .. } if self.env.lookup_var(name).is_none() => format!("{name}.{field}"),
                // `x.f(g)`: the method form of `f(x, g)`.
                _ => field.to_string(),
            },
            _ => return,
        };
        let PurityCallee::User(key) = self.purity_callee(&name) else { return };
        let caller = self.current_fn.as_ref().map(|(k, _)| *k).expect("checked above");
        let receiver = match &callee.kind {
            ast::ExprKind::Member { object, .. } if !name.contains('.') => Some(&**object),
            _ => receiver,
        };
        // The callee's parameter names, when its signature is known: the
        // slot each positional argument fills.
        let slots: Option<Vec<Sym>> = self.lookup_call_sig(callee).or_else(|| self.env.functions.get(&key).cloned())
            .map(|sig| sig.params.iter().map(|(n, _)| *n).collect());
        let positional = receiver.into_iter().chain(args).enumerate().map(|(i, a)| (slots.as_ref().and_then(|s| s.get(i).copied()), a));
        let all: Vec<(Option<Sym>, &ast::Expr)> = positional.chain(named.iter().map(|(n, a)| (Some(*n), a))).collect();
        for (slot, arg) in all {
            let v = self.fn_values(arg);
            let site = Site { file: self.source_file.clone(), span: arg.span };
            let mut params = v.params.clone();
            for f in &v.fns {
                if let PurityCallee::User(l) = f
                    && let Some(ps) = self.purity.lambda_param_calls.get(l)
                {
                    params.extend(ps.iter().copied());
                }
                self.purity.passed.push((caller, key, slot, f.clone(), site.clone()));
            }
            for p in params {
                let edges = self.purity.forwards.entry((caller, p)).or_default();
                if !edges.contains(&(key, slot)) {
                    edges.push((key, slot));
                }
            }
        }
    }

    /// The function values an expression is, when it is one: a lambda, a fn
    /// named as a value, or a local / parameter (what it was bound to).
    fn fn_values(&self, arg: &ast::Expr) -> FnValues {
        let name = match &arg.kind {
            ast::ExprKind::Lambda { .. } => {
                return FnValues { fns: self.lambda_key(arg.id).map(PurityCallee::User).into_iter().collect(), params: Vec::new() };
            }
            ast::ExprKind::Paren { expr } => return self.fn_values(expr),
            ast::ExprKind::Ident { name, .. } if self.env.lookup_var(name).is_some() => {
                return self.local_fn_values(name).unwrap_or_default();
            }
            ast::ExprKind::Ident { name, .. } => name.to_string(),
            ast::ExprKind::Member { object, field } => match &object.kind {
                ast::ExprKind::Ident { name, .. } if self.env.lookup_var(name).is_none() => format!("{name}.{field}"),
                _ => return FnValues::default(),
            },
            _ => return FnValues::default(),
        };
        let f = match OUTPUT_ABORT_BUILTINS.iter().find(|(b, _)| *b == name) {
            Some((b, _)) => PurityCallee::Builtin(b),
            // A name that is not a fn (a constant, a constructor) reaches no
            // recorded call, so it is never reported.
            None => self.purity_callee(&name),
        };
        FnValues { fns: vec![f], params: Vec::new() }
    }

    /// The `@pure` fns an argument in `slot` of `callee` runs inside: `callee`
    /// itself when declared `@pure`, and those its forward edges for that
    /// parameter end at (every edge when the slot is not known), with the fns
    /// passed through on the way.
    fn pure_targets(&self, callee: Sym, slot: Option<Sym>) -> Vec<(Sym, Vec<Sym>)> {
        let mut out = Vec::new();
        let mut seen: HashSet<(Sym, Option<Sym>)> = HashSet::from([(callee, slot)]);
        let mut queue: VecDeque<(Sym, Option<Sym>, Vec<Sym>)> = VecDeque::from([(callee, slot, Vec::new())]);
        while let Some((f, s, via)) = queue.pop_front() {
            if self.purity.declared.contains_key(&f) {
                out.push((f, via.clone()));
            }
            // Source order of the parameters' names, not the map's.
            let mut from: Vec<(&Sym, &Vec<(Sym, Option<Sym>)>)> = self.purity.forwards.iter()
                .filter(|((g, p), _)| *g == f && s.is_none_or(|s| s == *p))
                .map(|((_, p), e)| (p, e))
                .collect();
            from.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
            for (_, edges) in from {
                for (next, next_slot) in edges {
                    if seen.insert((*next, *next_slot)) {
                        let mut v = via.clone();
                        v.push(f);
                        queue.push_back((*next, *next_slot, v));
                    }
                }
            }
        }
        out
    }

    /// The callee a resolved name names: a stdlib fn or a user fn's key,
    /// by the resolution `generic_call_key` mirrors.
    pub(crate) fn purity_callee(&self, name: &str) -> PurityCallee {
        let qualified = self.env.import_table.resolve_direct(name);
        if let Some(key) = self.generic_call_key(name, qualified.as_deref()) {
            return PurityCallee::User(key);
        }
        let full = qualified.unwrap_or_else(|| match name.split_once('.') {
            Some((m, f)) => match self.env.import_table.resolve(m) {
                Some(c) => format!("{}.{}", c.as_str(), f),
                None => name.to_string(),
            },
            None => name.to_string(),
        });
        match full.rsplit_once('.') {
            Some((m, f)) => PurityCallee::Stdlib(sym(m), sym(f)),
            None => PurityCallee::User(sym(&full)),
        }
    }

    /// Post-solve, per program: queue this program's `@pure` fns, then judge
    /// every pending one against the calls recorded so far.
    pub(crate) fn check_pure_attrs(&mut self, program: &ast::Program) {
        for d in &program.decls {
            let ast::Decl::Fn { name, effect, attrs, extern_attrs, span, .. } = d else { continue };
            let Some(attr) = attrs.iter().find(|a| a.name.as_str() == "pure") else { continue };
            debug_assert!(!self.env.pure_fns_absent, "a @pure fn in a compilation canonicalized as having none");
            let at = Site { file: self.source_file.clone(), span: attr.span.or(*span) };
            let message = if effect.unwrap_or(false) {
                format!("`@pure` fn `{name}` is an `effect fn`")
            } else if !extern_attrs.is_empty() {
                format!("`@pure` cannot be checked on `@extern` fn `{name}`: its body is foreign")
            } else if self.env.inline_rust_fns.contains(&self.fn_decl_key(name)) {
                format!("`@pure` cannot be checked on `@inline_rust` fn `{name}`: its body is foreign")
            } else {
                let key = self.fn_decl_key(name);
                self.purity.declared.insert(key, (*name, at.clone()));
                self.purity.pending.push(PendingPure { key, name: *name, at });
                continue;
            };
            self.diagnostics.push(pure_diagnostic(message, at.span, None, at.file));
        }
        let pending = std::mem::take(&mut self.purity.pending);
        for p in pending {
            match self.purity_witness(p.key) {
                Some(w) if self.report_impure(&p, &w) => {}
                _ => self.purity.pending.push(p),
            }
        }
        let passed = std::mem::take(&mut self.purity.passed);
        for (caller, callee, slot, arg, site) in passed {
            // A `@pure` caller's own reach already holds what it passes (the
            // lambda's calls and the named fn are recorded under it).
            if self.purity.declared.contains_key(&caller) {
                continue;
            }
            let reported = self.pure_targets(callee, slot).into_iter().any(|(target, via)| self.report_impure_argument(target, &via, &arg, &site));
            if !reported {
                self.purity.passed.push((caller, callee, slot, arg, site));
            }
        }
    }

    /// E092 at a function passed to a `@pure` fn that reaches an effect.
    /// `false` while the callee is not known to be `@pure`, the argument
    /// reaches nothing impure yet, or the site is in another file.
    fn report_impure_argument(&mut self, callee: Sym, via: &[Sym], arg: &PurityCallee, site: &Site) -> bool {
        let Some((name, at)) = self.purity.declared.get(&callee).cloned() else { return false };
        if site.file != self.source_file || site.span.is_none() {
            return false;
        }
        let (passed, what, does) = match self.impure_callee(arg) {
            Some((what, does)) => (format!("`{what}`"), what, does),
            None => {
                let PurityCallee::User(k) = arg else { return false };
                let Some(w) = self.purity_witness(*k) else { return false };
                let passed = if self.purity.calls.contains_key(k) && k.as_str().contains("::<lambda#") {
                    "a lambda".to_string()
                } else {
                    format!("`{k}`")
                };
                (passed, w.what, w.does)
            }
        };
        let named = if at.file == self.source_file {
            format!("`{name}`")
        } else {
            format!("`{name}` ({}:{})", at.file.as_deref().unwrap_or("?"), at.span.map_or(0, |s| s.line))
        };
        // Handed on through plain fns' parameters: name them.
        let through = if via.is_empty() {
            String::new()
        } else {
            let hops: Vec<String> = via.iter().map(|k| format!("`{k}`")).collect();
            format!(" through {}", hops.join(" → "))
        };
        let message = if passed == format!("`{what}`") {
            format!("`@pure` fn {named} is passed `{what}`{through}, which {does}")
        } else {
            format!("`@pure` fn {named} is passed {passed}{through} that calls `{what}`, which {does}")
        };
        let mut d = super::err(message, PASSED_HINT, "@pure").with_code("E092");
        d.file = self.source_file.clone();
        if let Some(s) = site.span {
            d.line = Some(s.line);
            d.col = Some(s.col);
            d.end_col = Some(s.end_col);
        }
        if at.file == self.source_file
            && let Some(a) = at.span
        {
            d = d.with_secondary(a.line, Some(a.col), "`@pure` declared here".to_string());
        }
        self.diagnostics.push(d);
        true
    }

    /// Report `p` at the first site of its chain that lies in the file being
    /// inferred, so the snippet is real. `false` when none does yet.
    fn report_impure(&mut self, p: &PendingPure, w: &Witness) -> bool {
        let here = |s: &&Site| s.file == self.source_file && s.span.is_some();
        let Some(anchor) = w.sites.iter().find(here) else {
            return false;
        };
        let local = p.at.file == self.source_file;
        let named = if local {
            format!("`{}`", p.name)
        } else {
            let line = p.at.span.map_or(0, |s| s.line);
            format!("`{}` ({}:{line})", p.name, p.at.file.as_deref().unwrap_or("?"))
        };
        let message = if w.path.is_empty() {
            format!("`@pure` fn {named} calls `{}`, which {}", w.what, w.does)
        } else {
            let hops: Vec<String> = w.path.iter().map(|k| format!("`{k}`")).collect();
            format!("`@pure` fn {named} reaches `{}` via {}, and `{}` {}", w.what, hops.join(" → "), w.what, w.does)
        };
        let decl = if local { p.at.span } else { None };
        let d = pure_diagnostic(message, anchor.span, decl, self.source_file.clone());
        self.diagnostics.push(d);
        true
    }

    /// The first impure thing `root` reaches, breadth-first over the
    /// recorded calls in source order.
    fn purity_witness(&self, root: Sym) -> Option<Witness> {
        let mut seen: HashSet<Sym> = HashSet::from([root]);
        let mut queue: VecDeque<(Sym, Vec<Site>, Vec<Sym>)> = VecDeque::from([(root, Vec::new(), Vec::new())]);
        while let Some((f, sites, path)) = queue.pop_front() {
            for (callee, site) in self.purity.calls.get(&f).into_iter().flatten() {
                let mut chain = sites.clone();
                chain.push(site.clone());
                if let Some((what, does)) = self.impure_callee(callee) {
                    return Some(Witness { sites: chain, path, what, does });
                }
                if let PurityCallee::User(k) = callee
                    && seen.insert(*k)
                {
                    let mut next = path.clone();
                    next.push(*k);
                    queue.push_back((*k, chain, next));
                }
            }
        }
        None
    }

    /// `Some((name, what it does))` when the callee itself is impure.
    fn impure_callee(&self, callee: &PurityCallee) -> Option<(String, String)> {
        match callee {
            PurityCallee::Builtin(b) => {
                let does = OUTPUT_ABORT_BUILTINS.iter().find(|(n, _)| n == b).map_or("", |(_, d)| d);
                Some((b.to_string(), does.to_string()))
            }
            PurityCallee::Stdlib(m, f) => {
                let cats = almide_ir::effect::stdlib_call_effect(m.as_str(), f.as_str());
                let names: Vec<&str> = cats.iter().map(|e| e.name()).collect();
                (!cats.is_empty()).then(|| (format!("{m}.{f}"), format!("has the {} effect", names.join(", "))))
            }
            PurityCallee::User(k) if self.env.extern_fns.contains(k) => {
                Some((k.to_string(), "is an `@extern` fn, whose body cannot be checked".to_string()))
            }
            // #4824: an `@inline_rust` body outside the stdlib is foreign too.
            PurityCallee::User(k) if self.env.inline_rust_fns.contains(k) => {
                Some((k.to_string(), "is an `@inline_rust` fn, whose body is Rust text the checker cannot see".to_string()))
            }
            PurityCallee::User(_) => None,
        }
    }
}

const PURE_HINT: &str = "`@pure` promises the empty effect set: no effect category, no output (`println`, \
    `eprintln`) and no `panic` / `assert*`, in the fn and everything it calls. Move that call to a caller, \
    or remove `@pure`.";

const PASSED_HINT: &str = "A function passed to a `@pure` fn runs inside its promise of the empty effect set, \
    so it may not print, abort or carry an effect category. Pass a function without that call (do the output \
    in the caller, around the `@pure` call), or remove `@pure` from the callee.";

fn pure_diagnostic(message: String, at: Option<Span>, decl: Option<Span>, file: Option<String>) -> Diagnostic {
    let mut d = super::err(message, PURE_HINT, "@pure").with_code("E092");
    d.file = file;
    if let Some(s) = at {
        d.line = Some(s.line);
        d.col = Some(s.col);
        d.end_col = Some(s.end_col);
    }
    if let (Some(a), Some(s)) = (decl, at)
        && a != s
    {
        d = d.with_secondary(a.line, Some(a.col), "`@pure` declared here".to_string());
    }
    d
}
