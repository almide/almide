//! E110, phase 2 (row 22, #4232): request data decoded by hand into a type
//! that is also sent out.
//!
//! Phase 1 judges the stdlib's own request-decode path: `http.decode_json[T]`
//! is an `@input_sink`, and its `T` must be an input type (a `: Decode` type
//! that is not also `Encode`). A handler that parses the body itself,
//! `User.decode(json.parse(http.req_body(req))!)`, never calls the sink, so
//! the checker never saw it. This pass follows request data the way E109
//! follows an effect: through bindings, patterns, containers, closures and
//! user helpers, to the place it is decoded.
//!
//! - **Source.** A value whose type is an *untrusted-input type*: the
//!   declared parameter type of an `@input_sink` fn (`http.decode_json(req:
//!   HttpRequest)` makes `HttpRequest` one). Read off the declarations of
//!   the modules the program reaches, never a list of names. Everything a
//!   call computes from such a value (`http.req_body(req)`, `json.parse(..)`,
//!   `value.*`) carries the taint; a user helper carries it exactly where its
//!   body does (a per-parameter summary, solved to a fixpoint).
//! - **Sink.** A call of a `Decode` requirement (`T.decode`,
//!   `T.decode_strict`) whose `T` is also sent out — its protocol closure
//!   holds `Encode` — or is an input type holding such a record at any depth,
//!   or is a type parameter of a fn that is not itself `@input_sink` (its
//!   callers' types are then never judged).
//! - **Verdict.** A source reaching a sink is E110 at the decode site, or at
//!   the call that hands request data to a helper that decodes it.
//!
//! The body of an `@input_sink` fn is not judged here: its type parameters
//! are judged at each of its call sites by the checker (phase 1).
//!
//! Like E109 the analysis runs on the lowered program, before the optimizer,
//! on every route (`check`, `build`, `run`, `test`, both targets).

use std::collections::{HashMap, HashSet};

use almide_base::intern::Sym;
use almide_base::Span;
use almide_ir::free_vars::collect_pattern_bindings;
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::*;
use almide_lang::types::Ty;

use crate::canonicalize::protocols::{
    builtin_protocol, protocol_closure, DECODE_PROTOCOL, ENCODE_PROTOCOL, PROTOCOL_PARTS, STRICT_DECODE_METHOD,
};

/// The attribute that declares an untrusted-input sink (row 22, C-436).
pub const INPUT_SINK_ATTR: &str = "input_sink";

/// Why the decoded type may not take request data.
#[derive(Debug, Clone, PartialEq)]
pub enum HandDecode {
    /// The type is also written out (`Codec` / `Encode`).
    SentOut { ty: String, proto: &'static str },
    /// A record nested in the decoded type is also written out.
    NestedSentOut { outer: String, field: String, inner: String, proto: &'static str },
    /// A type parameter of `holder`, which is not an input sink.
    Forwarded { tv: String, holder: String },
}

/// One request-data decode the gate refuses.
#[derive(Debug, Clone)]
pub struct HandDecodeViolation {
    /// `name`, or `module.name` in a module.
    pub function: String,
    /// The module the function is declared in (`None`: the entry program).
    pub module: Option<String>,
    /// `HttpRequest → User.decode`, or through a helper
    /// `HttpRequest → parse_user(arg 1) → User.decode`.
    pub path: String,
    /// The decode name (`User.decode`) the data reaches.
    pub decode: String,
    pub why: HandDecode,
    pub site: Option<Span>,
}

/// A sink some parameter of a fn reaches.
#[derive(Debug, Clone)]
struct Hit {
    why: HandDecode,
    decode: String,
    /// The calls between the parameter and the decode, outermost first.
    via: Vec<String>,
    site: Option<Span>,
}

#[derive(Debug, Clone, Default)]
struct Summary {
    /// Parameter `i` flows to the return value.
    ret_from: Vec<bool>,
    /// Parameter `i` reaches a sink.
    sink_from: Vec<Option<Hit>>,
}

struct FnEntry<'a> {
    module: Option<&'a str>,
    f: &'a IrFunction,
}

struct TypeInfo<'a> {
    decl: &'a IrTypeDecl,
    /// Declared by a bundled stdlib module: a leaf value with no input
    /// surface of its own.
    stdlib: bool,
}

struct Cx<'a> {
    fns: Vec<FnEntry<'a>>,
    by_key: HashMap<(Option<String>, String), usize>,
    types: HashMap<String, TypeInfo<'a>>,
    sources: Vec<Ty>,
    decode_methods: Vec<String>,
}

fn last_segment(s: &str) -> &str {
    s.rsplit('.').next().unwrap_or(s)
}

fn has_attr(f: &IrFunction, name: &str) -> bool {
    f.attrs.iter().any(|a| a.name.as_str() == name)
}

impl<'a> Cx<'a> {
    fn is_source(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Named(n, _) => self.sources.iter().any(|s| match s {
                Ty::Named(sn, _) => last_segment(sn.as_str()) == last_segment(n.as_str()),
                _ => false,
            }),
            other => self.sources.iter().any(|s| s == other),
        }
    }

    fn callee(&self, cur: Option<&str>, target: &CallTarget) -> Option<usize> {
        let key = |m: Option<&str>, n: &str| (m.map(str::to_string), n.to_string());
        match target {
            CallTarget::Named { name } => {
                let name = name.as_str();
                self.by_key.get(&key(cur, name))
                    .or_else(|| self.by_key.get(&key(None, name)))
                    .or_else(|| name.rsplit_once('.').and_then(|(m, n)| self.by_key.get(&key(Some(m), n))))
                    .copied()
            }
            CallTarget::Module { module, func, .. } => self.by_key.get(&key(Some(module.as_str()), func.as_str())).copied(),
            _ => None,
        }
    }

    fn type_info(&self, name: &str) -> Option<&TypeInfo<'a>> {
        self.types.get(name).or_else(|| self.types.get(last_segment(name)))
    }

    /// `Some("Codec")` / `Some("Encode")` when the type's declared protocol
    /// closure holds `Encode`.
    fn sent_out_as(&self, name: &str) -> Option<&'static str> {
        let declared: Vec<Sym> = self.type_info(name)?.decl.deriving.clone().unwrap_or_default();
        if !declared.iter().any(|p| protocol_closure(p.as_str()).contains(&ENCODE_PROTOCOL)) {
            return None;
        }
        Some(PROTOCOL_PARTS.iter()
            .find(|(c, parts)| parts.contains(&ENCODE_PROTOCOL) && declared.iter().any(|d| d.as_str() == *c))
            .map(|(c, _)| *c)
            .unwrap_or(ENCODE_PROTOCOL))
    }

    fn nested_sent_out(&self, outer: &str, seen: &mut HashSet<String>) -> Option<HandDecode> {
        if !seen.insert(last_segment(outer).to_string()) {
            return None;
        }
        let info = self.type_info(outer)?;
        let fields: Vec<(String, &Ty)> = match &info.decl.kind {
            IrTypeDeclKind::Record { fields } => fields.iter().map(|f| (f.name.to_string(), &f.ty)).collect(),
            IrTypeDeclKind::Variant { cases, .. } => cases.iter().flat_map(|c| match &c.kind {
                IrVariantKind::Unit => Vec::new(),
                IrVariantKind::Tuple { fields } => fields.iter().enumerate()
                    .map(|(i, t)| (format!("{}.{}", c.name, i), t)).collect(),
                IrVariantKind::Record { fields } => fields.iter()
                    .map(|f| (format!("{}.{}", c.name, f.name), &f.ty)).collect(),
            }).collect(),
            IrTypeDeclKind::Alias { .. } => return None,
        };
        for (field, fty) in fields {
            let mut leaves = Vec::new();
            leaf_nominals(fty, &mut leaves);
            for leaf in leaves {
                let leaf = leaf.as_str();
                if last_segment(leaf) == last_segment(outer) {
                    continue;
                }
                let Some(li) = self.type_info(leaf) else { continue };
                if li.stdlib {
                    continue;
                }
                if let Some(proto) = self.sent_out_as(leaf) {
                    return Some(HandDecode::NestedSentOut {
                        outer: last_segment(outer).to_string(), field, inner: last_segment(leaf).to_string(), proto,
                    });
                }
                if let Some(found) = self.nested_sent_out(leaf, seen) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// The verdict on a call of `name` inside `holder`: `Some` when `name`
    /// is a `Decode` requirement of a type that may not take request data.
    fn sink(&self, name: &str, holder: &FnEntry<'a>) -> Option<HandDecode> {
        let (ty, method) = name.rsplit_once('.')?;
        if !self.decode_methods.iter().any(|m| m == method) {
            return None;
        }
        let bare = last_segment(ty);
        let is_type_param = holder.f.generics.as_ref()
            .is_some_and(|gs| gs.iter().any(|g| g.name.as_str() == bare));
        if is_type_param {
            return Some(HandDecode::Forwarded { tv: bare.to_string(), holder: qualified(holder) });
        }
        let info = self.type_info(ty)?;
        if info.stdlib {
            return None;
        }
        if let Some(proto) = self.sent_out_as(ty) {
            return Some(HandDecode::SentOut { ty: bare.to_string(), proto });
        }
        self.nested_sent_out(ty, &mut HashSet::new())
    }
}

fn qualified(e: &FnEntry<'_>) -> String {
    match e.module {
        Some(m) => format!("{}.{}", m, e.f.name),
        None => e.f.name.to_string(),
    }
}

/// The nominal types a field type names, through the standard containers.
fn leaf_nominals(ty: &Ty, out: &mut Vec<Sym>) {
    match ty {
        Ty::Named(n, args) => {
            out.push(*n);
            for a in args { leaf_nominals(a, out); }
        }
        Ty::Variant { name, .. } => out.push(*name),
        Ty::Applied(_, args) | Ty::Tuple(args) => for a in args { leaf_nominals(a, out); },
        Ty::Record { fields } | Ty::OpenRecord { fields } => for (_, t) in fields { leaf_nominals(t, out); },
        _ => {}
    }
}

/// One function's taint walk: the variables request data reaches, solved to
/// a fixpoint over the body, and the sinks it reaches.
struct Walk<'c, 'a> {
    cx: &'c Cx<'a>,
    summaries: &'c [Summary],
    entry: &'c FnEntry<'a>,
    /// Values of an untrusted-input type are sources (the judging run);
    /// off for a summary run, which seeds one parameter only.
    source_mode: bool,
    tainted: HashSet<VarId>,
    changed: bool,
    hits: Vec<Hit>,
}

impl<'c, 'a> Walk<'c, 'a> {
    fn mark(&mut self, v: VarId) {
        if self.tainted.insert(v) {
            self.changed = true;
        }
    }

    fn any(&self, es: &[IrExpr]) -> bool {
        es.iter().any(|e| self.taint(e))
    }

    /// Does `e`'s value carry request data?
    fn taint(&self, e: &IrExpr) -> bool {
        if self.source_mode && self.cx.is_source(&e.ty) {
            return true;
        }
        match &e.kind {
            IrExprKind::Var { id } => self.tainted.contains(id),
            IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
                if let Some(ix) = self.cx.callee(self.entry.module, target) {
                    let s = &self.summaries[ix];
                    return args.iter().enumerate().any(|(i, a)| s.ret_from.get(i).copied().unwrap_or(false) && self.taint(a));
                }
                let object = match target {
                    CallTarget::Method { object, .. } => self.taint(object),
                    CallTarget::Computed { callee } => self.taint(callee),
                    _ => false,
                };
                object || self.any(args)
            }
            IrExprKind::Block { expr, .. } => expr.as_deref().is_some_and(|t| self.taint(t)),
            IrExprKind::If { then, else_, .. } => self.taint(then) || self.taint(else_),
            IrExprKind::Match { arms, .. } => arms.iter().any(|a| self.taint(&a.body)),
            IrExprKind::ForIn { .. } | IrExprKind::While { .. } => false,
            IrExprKind::Lambda { body, .. } => self.taint(body),
            _ => {
                let mut any = AnyChild { w: self, found: false };
                walk_expr(&mut any, e);
                any.found
            }
        }
    }

    fn mark_pattern(&mut self, p: &IrPattern) {
        let mut bound = HashSet::new();
        collect_pattern_bindings(p, &mut bound);
        for v in bound {
            self.mark(v);
        }
    }

    fn mark_lambda_params(&mut self, e: &IrExpr) {
        if let IrExprKind::Lambda { params, .. } = &e.kind {
            for (v, _) in params {
                self.mark(*v);
            }
        }
    }

    fn call(&mut self, target: &CallTarget, args: &[IrExpr], site: Option<Span>) {
        let (cx, summaries) = (self.cx, self.summaries);
        // The sink first: a derived `User.decode` is also a fn of the
        // program, and its summary must not stand in for the verdict.
        if let CallTarget::Named { name } = target {
            if let Some(why) = cx.sink(name.as_str(), self.entry) {
                if self.any(args) {
                    self.hits.push(Hit { why, decode: name.to_string(), via: Vec::new(), site });
                }
                return;
            }
        }
        if let Some(ix) = cx.callee(self.entry.module, target) {
            let s = &summaries[ix];
            let callee = qualified(&cx.fns[ix]);
            let mut found = Vec::new();
            for (i, a) in args.iter().enumerate() {
                if let Some(Some(hit)) = s.sink_from.get(i) {
                    if self.taint(a) {
                        let mut via = vec![format!("{}(arg {})", callee, i + 1)];
                        via.extend(hit.via.iter().cloned());
                        found.push(Hit { why: hit.why.clone(), decode: hit.decode.clone(), via, site });
                    }
                }
            }
            self.hits.extend(found);
            return;
        }
        let tainted_in = self.any(args) || match target {
            CallTarget::Method { object, .. } => self.taint(object),
            CallTarget::Computed { callee } => self.taint(callee),
            _ => false,
        };
        // A higher-order stdlib call hands its data to the callbacks.
        if tainted_in {
            for a in args {
                self.mark_lambda_params(a);
            }
        }
    }

    /// Bind and judge until no variable is added.
    fn solve(&mut self, body: &IrExpr) {
        loop {
            self.changed = false;
            self.hits.clear();
            self.visit_expr(body);
            if !self.changed {
                break;
            }
        }
    }
}

struct AnyChild<'w, 'c, 'a> {
    w: &'w Walk<'c, 'a>,
    found: bool,
}

impl IrVisitor for AnyChild<'_, '_, '_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        if !self.found && self.w.taint(e) {
            self.found = true;
        }
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        walk_stmt(self, s);
    }
    fn visit_pattern(&mut self, _: &IrPattern) {}
}

impl IrVisitor for Walk<'_, '_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Match { subject, arms } => {
                if self.taint(subject) {
                    for a in arms {
                        self.mark_pattern(&a.pattern);
                    }
                }
            }
            IrExprKind::ForIn { var, var_tuple, iterable, .. } => {
                if self.taint(iterable) {
                    self.mark(*var);
                    for v in var_tuple.iter().flatten() {
                        self.mark(*v);
                    }
                }
            }
            IrExprKind::IterChain { source, steps, collector, .. } => {
                if self.taint(source) {
                    for st in steps {
                        if let Some(l) = st.lambda() {
                            self.mark_lambda_params(l);
                        }
                    }
                    if let Some(l) = collector.lambda() {
                        self.mark_lambda_params(l);
                    }
                }
            }
            IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
                self.call(target, args, e.span);
            }
            _ => {}
        }
        walk_expr(self, e);
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        match &s.kind {
            IrStmtKind::Bind { var, value, .. } | IrStmtKind::Assign { var, value } => {
                if self.taint(value) {
                    self.mark(*var);
                }
            }
            IrStmtKind::BindDestructure { pattern, value } => {
                if self.taint(value) {
                    self.mark_pattern(pattern);
                }
            }
            IrStmtKind::FieldAssign { target, value, .. }
            | IrStmtKind::IndexAssign { target, value, .. }
            | IrStmtKind::MapInsert { target, value, .. } => {
                if self.taint(value) {
                    self.mark(*target);
                }
            }
            _ => {}
        }
        walk_stmt(self, s);
    }

    fn visit_pattern(&mut self, _: &IrPattern) {}
}

/// Does any user fn name a `Decode` requirement at all? Most programs do
/// not, and then there is nothing to judge.
struct NamesDecode<'d> {
    methods: &'d [String],
    found: bool,
}

impl IrVisitor for NamesDecode<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        if self.found {
            return;
        }
        if let IrExprKind::Call { target: CallTarget::Named { name }, .. } = &e.kind {
            if name.as_str().rsplit_once('.').is_some_and(|(_, m)| self.methods.iter().any(|d| d == m)) {
                self.found = true;
                return;
            }
        }
        walk_expr(self, e);
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        walk_stmt(self, s);
    }
    fn visit_pattern(&mut self, _: &IrPattern) {}
}

/// The modules a program reaches through a call target.
struct ModulesCalled(HashSet<String>);

impl IrVisitor for ModulesCalled {
    fn visit_expr(&mut self, e: &IrExpr) {
        if let IrExprKind::Call { target: CallTarget::Module { module, .. }, .. } = &e.kind {
            self.0.insert(module.to_string());
        }
        walk_expr(self, e);
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        walk_stmt(self, s);
    }
    fn visit_pattern(&mut self, _: &IrPattern) {}
}

/// Every hand decode of request data into a type that may not take it.
/// Empty under the negative control `ALMIDE_CONTROL_ADMIT_HAND_DECODE`.
pub fn hand_decode_violations(program: &IrProgram) -> Vec<HandDecodeViolation> {
    if almide_base::env::flag("ALMIDE_CONTROL_ADMIT_HAND_DECODE") {
        return Vec::new();
    }
    let mut decode_methods: Vec<String> = builtin_protocol(DECODE_PROTOCOL)
        .map(|p| p.method_names())
        .unwrap_or_default();
    if !decode_methods.iter().any(|m| m == STRICT_DECODE_METHOD) {
        decode_methods.push(STRICT_DECODE_METHOD.to_string());
    }

    // The fns that are judged: the entry's and every user module's. A
    // bundled module's fns are called as opaque transforms.
    let mut fns: Vec<FnEntry<'_>> = Vec::new();
    for f in &program.functions {
        fns.push(FnEntry { module: None, f });
    }
    for m in &program.modules {
        if almide_lang::stdlib_info::is_bundled_module(m.name.as_str()) {
            continue;
        }
        for f in &m.functions {
            fns.push(FnEntry { module: Some(m.name.as_str()), f });
        }
    }
    let mut names = NamesDecode { methods: &decode_methods, found: false };
    for e in &fns {
        names.visit_expr(&e.f.body);
    }
    if !names.found {
        return Vec::new();
    }

    // Sources: the declared parameter types of every stdlib `@input_sink` fn
    // in the modules the program reaches. A user's own `@input_sink` fn names
    // a sink, not a source: its parameter (often a plain `String`) is not
    // request data by declaration.
    let mut reached: HashSet<String> = program.used_stdlib_modules.iter().cloned().collect();
    reached.extend(program.modules.iter().map(|m| m.name.to_string()));
    let mut called = ModulesCalled(HashSet::new());
    for e in &fns {
        called.visit_expr(&e.f.body);
    }
    reached.extend(called.0);
    let mut sources: Vec<Ty> = Vec::new();
    let mut reached: Vec<String> = reached.into_iter().collect();
    reached.sort();
    for m in &reached {
        for t in crate::bundled_sigs::input_sink_param_types(m) {
            if !sources.contains(&t) {
                sources.push(t);
            }
        }
    }
    if sources.is_empty() {
        return Vec::new();
    }

    let mut types: HashMap<String, TypeInfo<'_>> = HashMap::new();
    for d in &program.type_decls {
        types.entry(d.name.to_string()).or_insert(TypeInfo { decl: d, stdlib: false });
    }
    for m in &program.modules {
        let stdlib = almide_lang::stdlib_info::is_bundled_module(m.name.as_str());
        for d in &m.type_decls {
            types.entry(d.name.to_string()).or_insert(TypeInfo { decl: d, stdlib });
            types.entry(format!("{}.{}", m.name, last_segment(d.name.as_str()))).or_insert(TypeInfo { decl: d, stdlib });
        }
    }
    let mut by_key = HashMap::new();
    for (i, e) in fns.iter().enumerate() {
        by_key.entry((e.module.map(str::to_string), e.f.name.to_string())).or_insert(i);
        if let Some(m) = e.module {
            by_key.entry((Some(m.to_string()), last_segment(e.f.name.as_str()).to_string())).or_insert(i);
        }
    }
    let cx = Cx { fns, by_key, types, sources, decode_methods };

    // Per-parameter summaries, to a fixpoint (a summary only grows).
    let judged = |e: &FnEntry<'_>| !has_attr(e.f, INPUT_SINK_ATTR);
    let mut summaries: Vec<Summary> = cx.fns.iter()
        .map(|e| Summary { ret_from: vec![false; e.f.params.len()], sink_from: vec![None; e.f.params.len()] })
        .collect();
    loop {
        let mut grew = false;
        for (ix, e) in cx.fns.iter().enumerate() {
            if !judged(e) {
                continue;
            }
            for (pi, p) in e.f.params.iter().enumerate() {
                let mut w = Walk {
                    cx: &cx, summaries: &summaries, entry: e, source_mode: false,
                    tainted: HashSet::from([p.var]), changed: false, hits: Vec::new(),
                };
                w.solve(&e.f.body);
                let ret = w.taint(&e.f.body);
                let hit = w.hits.into_iter().next();
                let s = &mut summaries[ix];
                if ret && !s.ret_from[pi] {
                    s.ret_from[pi] = true;
                    grew = true;
                }
                if hit.is_some() && s.sink_from[pi].is_none() {
                    s.sink_from[pi] = hit;
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }

    // The judging run: untrusted-input values are the sources.
    let mut out = Vec::new();
    let mut seen: HashSet<(String, usize, usize)> = HashSet::new();
    for e in cx.fns.iter() {
        if !judged(e) {
            continue;
        }
        let mut w = Walk {
            cx: &cx, summaries: &summaries, entry: e, source_mode: true,
            tainted: HashSet::new(), changed: false, hits: Vec::new(),
        };
        w.solve(&e.f.body);
        for hit in w.hits {
            let function = qualified(e);
            let key = (function.clone(), hit.site.map_or(0, |s| s.line), hit.site.map_or(0, |s| s.col));
            if !seen.insert(key) {
                continue;
            }
            let source = cx.sources.first().map(|t| t.display()).unwrap_or_default();
            let mut path = vec![source];
            path.extend(hit.via.iter().cloned());
            path.push(hit.decode.clone());
            out.push(HandDecodeViolation {
                function,
                module: e.module.map(str::to_string),
                path: path.join(" → "),
                decode: hit.decode,
                why: hit.why,
                site: hit.site,
            });
        }
    }
    out.sort_by(|a, b| a.function.cmp(&b.function));
    out
}
