//! A `Value` built and consumed exactly once by a known consumer is not
//! materialized.
//!
//! Two rewrites, both over the ROOT program (one var table, one binding site
//! per id — the `sink_inline` discipline), both read off the shape of the
//! code; no user function is named.
//!
//! **Reader.** A parsed document whose every use reads a scalar at a static
//! key path is read off the text by one scan of it; `read.rs` holds the
//! rule, the plan and the rewrite.
//!
//! **Writer.** `json.stringify(value.array(xs |> list.map((x) => g(x))))`,
//! where the tail of `g` is a constructor tree (`value.object` /
//! `value.array` literals over `value.int` / `value.bool` / `value.string` /
//! `value.null`, and calls to other such functions), writes the text
//! directly: `g` gets a writer twin (`__almd_json_text_<g>`) with the same
//! parameters and statements whose tail is one interpolation, the field
//! expressions its holes IN SOURCE ORDER, and the site becomes
//! `json.__array_text(xs |> list.map((x) => twin(x)))`. Each hole is
//! evaluated exactly where its constructor argument was, so effects and
//! aborts keep their order; the text between them is pure. A sub-value the
//! pass cannot see through is a hole holding `json.stringify(<it>)`, the
//! runtime writer's own answer. A key or a literal that needs an escape, a
//! repeated key and a string hole that is not an Int or a Bool leave that
//! constructor to the runtime writer. A `let`-bound array is rewritten
//! where it is bound (nothing moves), when its only use is the stringify.
//! A lone tree written once is left alone: nothing per element to save.
//!
//! Both kernels are functions of the `json` module, kept live by
//! `bundled_reach::COMPILER_INSERTED`, stamped `@on_demand` here and so
//! dropped after mono when no rewrite named them.
//!
//! Ablation: `ALMIDE_VALUE_FUSE_OFF=1` skips the pass.

use std::collections::{HashMap, HashSet};

use almide_base::intern::{sym, Sym};
use almide_ir::visit::{walk_expr, IrVisitor};
use almide_ir::visit_mut::{walk_expr_mut, walk_pattern_mut, walk_stmt_mut, IrMutVisitor};
use almide_ir::*;
use almide_lang::types::Ty;

pub const ABLATION_ENV: &str = "ALMIDE_VALUE_FUSE_OFF";

mod read;

const ARRAY_WRITER: &str = "__array_text";

/// The kernels and their helpers (`__jr_*`) in the `json` module.
fn is_kernel(name: &str) -> bool {
    name == read::SPANS || name == "__rows" || name.starts_with("__leaf_") || name == ARRAY_WRITER || name.starts_with("__jr_")
}

/// Stamps the kernels `@on_demand`: `bundled_reach` keeps them live for
/// every program that loads `json`, and `on_demand::prune_unreached` drops
/// them after mono unless a rewrite named them. Runs whether or not the
/// rewrites do.
pub fn stamp_kernels(program: &mut IrProgram) {
    for m in program.modules.iter_mut().filter(|m| m.name.as_str() == "json") {
        for f in m.functions.iter_mut().filter(|f| is_kernel(f.name.as_str())) {
            if !f.attrs.iter().any(|a| a.name.as_str() == ON_DEMAND_ATTR) {
                f.attrs.push(almide_lang::ast::Attribute { name: sym(ON_DEMAND_ATTR), args: vec![], span: None });
            }
        }
    }
}

pub fn fuse_value_round_trips(program: &mut IrProgram) {
    if almide_base::env::flag(ABLATION_ENV) {
        return;
    }
    let Some(json) = program.modules.iter().find(|m| m.name.as_str() == "json") else { return };
    let has = |f: &str| json.functions.iter().any(|g| g.name.as_str() == f);
    let (reader_live, writer_live) = (has(read::SPANS), has(ARRAY_WRITER));
    if !reader_live && !writer_live {
        return;
    }

    let readers = if reader_live { read::reader_fns(&program.functions) } else { read::Readers::new() };
    let mut writers: HashSet<Sym> = HashSet::new();
    if writer_live {
        // A producer may call another producer: grow to the fixpoint.
        loop {
            let before = writers.len();
            for f in &program.functions {
                if !writers.contains(&f.name) && writer_fn(f, &writers) {
                    writers.insert(f.name);
                }
            }
            if writers.len() == before {
                break;
            }
        }
    }

    let IrProgram { functions, var_table, .. } = program;
    let mut cx = Fuser {
        vt: var_table,
        readers: &readers,
        writers: &writers,
        reader_live,
        writer_live,
        read_twins: Vec::new(),
        new_funcs: Vec::new(),
        twins_wanted: Vec::new(),
        counter: 0,
    };
    for f in functions.iter_mut() {
        cx.visit_expr_mut(&mut f.body);
    }
    // Twins, and the twins their bodies call.
    let mut built: HashSet<Sym> = HashSet::new();
    while let Some(name) = cx.twins_wanted.pop() {
        if !built.insert(name) {
            continue;
        }
        let Some(src) = functions.iter().find(|f| f.name == name) else { continue };
        let twin = cx.twin(src);
        cx.new_funcs.push(twin);
    }
    while let Some(tw) = cx.read_twins.pop() {
        let Some(src) = functions.iter().find(|f| f.name == tw.src) else { continue };
        let twin = cx.read_twin(src, &tw);
        cx.new_funcs.push(twin);
    }
    let new_funcs = std::mem::take(&mut cx.new_funcs);
    functions.extend(new_funcs);
}

// ── shapes ──────────────────────────────────────────────────────────────

fn module_call<'e>(e: &'e IrExpr, module: &str, func: &str) -> Option<&'e [IrExpr]> {
    match &e.kind {
        IrExprKind::Call { target: CallTarget::Module { module: m, func: f, .. }, args, .. }
            if m.as_str() == module && f.as_str() == func =>
        {
            Some(args.as_slice())
        }
        _ => None,
    }
}

fn is_var(e: &IrExpr, id: VarId) -> bool {
    matches!(e.kind, IrExprKind::Var { id: v } if v == id)
}

fn is_value_null(e: &IrExpr) -> bool {
    module_call(e, "value", "null").is_some_and(|a| a.is_empty())
}

fn is_value_ty(ty: &Ty) -> bool {
    matches!(ty, Ty::Named(n, args) if n.as_str() == "Value" && args.is_empty())
}

/// A body is the tail of an empty block as much as it is itself.
fn tail(e: &IrExpr) -> &IrExpr {
    match &e.kind {
        IrExprKind::Block { stmts, expr: Some(t) } if stmts.is_empty() => tail(t),
        _ => e,
    }
}

fn plain_fn(f: &IrFunction) -> bool {
    !f.is_effect
        && !f.is_test
        && f.extern_attrs.is_empty()
        && f.export_attrs.is_empty()
        && f.name.as_str() != "main"
        && !f.generics.as_ref().is_some_and(|g| !g.is_empty())
        && f.params.iter().all(|p| !p.is_mut && p.default.is_none() && p.open_record.is_none())
}

fn clean_text(s: &str) -> bool {
    s.bytes().all(|b| (0x20..0x7f).contains(&b) && b != b'"' && b != b'\\')
}

fn is_literal_ctor(e: &IrExpr) -> bool {
    ["object", "array"].iter().any(|c| {
        module_call(e, "value", c).is_some_and(|a| matches!(a, [l] if matches!(l.kind, IrExprKind::List { .. })))
    })
}

/// A function whose tail is a constructor tree the writer can spell.
fn writer_fn(f: &IrFunction, writers: &HashSet<Sym>) -> bool {
    if !plain_fn(f) || !is_value_ty(&f.ret_ty) {
        return false;
    }
    let t = match &f.body.kind {
        IrExprKind::Block { expr: Some(t), .. } => t.as_ref(),
        IrExprKind::Block { .. } => return false,
        _ => &f.body,
    };
    let producer_call = matches!(&t.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. } if writers.contains(name));
    (is_literal_ctor(t) || producer_call) && renamable(&f.body) && Spell { writers, wanted: None }.text_of(t).is_some()
}

// ── the writer's spelling of a constructor tree ─────────────────────────

struct Spell<'a> {
    writers: &'a HashSet<Sym>,
    /// `None` while only asking whether a tree can be spelled.
    wanted: Option<&'a mut Vec<Sym>>,
}

fn twin_name(g: Sym) -> Sym {
    sym(&format!("__almd_json_text_{}", g.as_str()))
}

fn lit(parts: &mut Vec<IrStringPart>, s: &str) {
    if let Some(IrStringPart::Lit { value }) = parts.last_mut() {
        value.push_str(s);
    } else {
        parts.push(IrStringPart::Lit { value: s.to_string() });
    }
}

fn str_expr(kind: IrExprKind, span: Option<almide_base::Span>) -> IrExpr {
    IrExpr { kind, ty: Ty::String, span, def_id: None }
}

fn json_call(func: &str, arg: IrExpr) -> IrExpr {
    let span = arg.span;
    str_expr(
        IrExprKind::Call {
            target: CallTarget::Module { module: sym("json"), func: sym(func), def_id: None },
            args: vec![arg],
            type_args: vec![],
        },
        span,
    )
}

impl Spell<'_> {
    /// The text of the Value `e` as interpolation parts, or `None` when `e`
    /// is not a shape the writer spells.
    fn text_of(&mut self, e: &IrExpr) -> Option<Vec<IrStringPart>> {
        let mut parts = Vec::new();
        self.spell(e, &mut parts).then_some(parts)
    }

    /// A sub-value: spelled, or left to the runtime writer as one hole.
    fn value(&mut self, e: &IrExpr, parts: &mut Vec<IrStringPart>) {
        let mut own = Vec::new();
        if self.spell(e, &mut own) {
            for p in own {
                match p {
                    IrStringPart::Lit { value } => lit(parts, &value),
                    p => parts.push(p),
                }
            }
        } else {
            parts.push(IrStringPart::Expr { expr: json_call("stringify", e.clone()) });
        }
    }

    fn spell(&mut self, e: &IrExpr, parts: &mut Vec<IrStringPart>) -> bool {
        if let Some([list]) = module_call(e, "value", "object") {
            let IrExprKind::List { elements } = &list.kind else { return false };
            let mut seen = HashSet::new();
            let mut fields = Vec::new();
            for el in elements {
                let IrExprKind::Tuple { elements: kv } = &el.kind else { return false };
                let [k, v] = kv.as_slice() else { return false };
                let IrExprKind::LitStr { value: key } = &k.kind else { return false };
                if !clean_text(key) || !seen.insert(key.as_str()) {
                    return false;
                }
                fields.push((key, v));
            }
            lit(parts, "{");
            for (i, (key, v)) in fields.into_iter().enumerate() {
                lit(parts, &format!("{}\"{}\":", if i > 0 { "," } else { "" }, key));
                self.value(v, parts);
            }
            lit(parts, "}");
            return true;
        }
        if let Some([arg]) = module_call(e, "value", "array") {
            if let IrExprKind::List { elements } = &arg.kind {
                lit(parts, "[");
                for (i, v) in elements.iter().enumerate() {
                    if i > 0 {
                        lit(parts, ",");
                    }
                    self.value(v, parts);
                }
                lit(parts, "]");
                return true;
            }
            // value.array(xs |> list.map((x) => g(x))) over a producer g
            let Some([src, f]) = module_call(arg, "list", "map") else { return false };
            let IrExprKind::Lambda { params, body, lambda_id } = &f.kind else { return false };
            let [(p, _)] = params.as_slice() else { return false };
            let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = &body.kind else { return false };
            if !self.writers.contains(name) || !matches!(args.as_slice(), [a] if is_var(a, *p)) {
                return false;
            }
            if let Some(w) = self.wanted.as_deref_mut() {
                w.push(*name);
            }
            let twin_call = str_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: twin_name(*name) },
                    args: args.clone(),
                    type_args: type_args.clone(),
                },
                body.span,
            );
            let lambda = IrExpr {
                kind: IrExprKind::Lambda { params: params.clone(), body: Box::new(twin_call), lambda_id: *lambda_id },
                ty: match &f.ty {
                    Ty::Fn { params, is_effect, .. } => Ty::Fn { params: params.clone(), ret: Box::new(Ty::String), is_effect: *is_effect },
                    _ => return false,
                },
                span: f.span,
                def_id: None,
            };
            let IrExprKind::Call { target, type_args, .. } = &arg.kind else { return false };
            let type_args = type_args.iter().map(|t| if is_value_ty(t) { Ty::String } else { t.clone() }).collect();
            let mapped = IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: vec![src.clone(), lambda], type_args },
                ty: Ty::list(Ty::String),
                span: arg.span,
                def_id: None,
            };
            parts.push(IrStringPart::Expr { expr: json_call(ARRAY_WRITER, mapped) });
            return true;
        }
        if let Some([x]) = module_call(e, "value", "int") {
            if !matches!(x.ty, Ty::Int) {
                return false;
            }
            parts.push(IrStringPart::Expr { expr: x.clone() });
            return true;
        }
        if let Some([x]) = module_call(e, "value", "bool") {
            if !matches!(x.ty, Ty::Bool) {
                return false;
            }
            parts.push(IrStringPart::Expr { expr: x.clone() });
            return true;
        }
        if is_value_null(e) {
            lit(parts, "null");
            return true;
        }
        if let Some([x]) = module_call(e, "value", "string") {
            return match &x.kind {
                IrExprKind::LitStr { value } if clean_text(value) => {
                    lit(parts, &format!("\"{value}\""));
                    true
                }
                IrExprKind::StringInterp { parts: inner }
                    if inner.iter().all(|p| match p {
                        IrStringPart::Lit { value } => clean_text(value),
                        IrStringPart::Expr { expr } => matches!(expr.ty, Ty::Int | Ty::Bool),
                    }) =>
                {
                    lit(parts, "\"");
                    for p in inner {
                        match p {
                            IrStringPart::Lit { value } => lit(parts, value),
                            p => parts.push(p.clone()),
                        }
                    }
                    lit(parts, "\"");
                    true
                }
                _ => false,
            };
        }
        if let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = &e.kind {
            if !self.writers.contains(name) || !type_args.is_empty() {
                return false;
            }
            if let Some(w) = self.wanted.as_deref_mut() {
                w.push(*name);
            }
            let call = str_expr(
                IrExprKind::Call { target: CallTarget::Named { name: twin_name(*name) }, args: args.clone(), type_args: vec![] },
                e.span,
            );
            parts.push(IrStringPart::Expr { expr: call });
            return true;
        }
        false
    }
}

// ── the rewrite ─────────────────────────────────────────────────────────

struct Fuser<'a> {
    vt: &'a mut VarTable,
    readers: &'a read::Readers,
    read_twins: Vec<read::ReadTwin>,
    writers: &'a HashSet<Sym>,
    reader_live: bool,
    writer_live: bool,
    new_funcs: Vec<IrFunction>,
    twins_wanted: Vec<Sym>,
    counter: u32,
}

fn occurrences_in(stmts: &[IrStmt], tail: Option<&IrExpr>, var: VarId) -> usize {
    struct Count {
        var: VarId,
        n: usize,
    }
    impl IrVisitor for Count {
        fn visit_expr(&mut self, e: &IrExpr) {
            if is_var(e, self.var) {
                self.n += 1;
            }
            walk_expr(self, e);
        }
    }
    let mut c = Count { var, n: 0 };
    for s in stmts {
        c.visit_stmt(s);
    }
    if let Some(t) = tail {
        c.visit_expr(t);
    }
    c.n
}

/// Replaces every read of `from` with a copy of `to`.
struct Subst<'e> {
    from: VarId,
    to: &'e IrExpr,
}

impl IrMutVisitor for Subst<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if is_var(e, self.from) {
            *e = self.to.clone();
            return;
        }
        walk_expr_mut(self, e);
    }
}

/// Gives every variable a copied body binds a fresh id (`sink_inline`'s
/// rule: binding sites are met before their uses in walk order).
struct Rename<'v> {
    vt: &'v mut VarTable,
    map: HashMap<VarId, VarId>,
}

impl Rename<'_> {
    fn bind(&mut self, id: &mut VarId) {
        let info = self.vt.get(*id);
        let (ty, m) = (info.ty.clone(), info.mutability);
        let fresh = self.vt.alloc_like(*id, ty, m);
        self.map.insert(*id, fresh);
        *id = fresh;
    }
}

impl IrMutVisitor for Rename<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        match &mut e.kind {
            IrExprKind::Var { id } => {
                if let Some(n) = self.map.get(id) {
                    *id = *n;
                }
            }
            IrExprKind::Lambda { params, .. } => {
                for (id, _) in params.iter_mut() {
                    self.bind(id);
                }
            }
            IrExprKind::ForIn { var, var_tuple, .. } => {
                self.bind(var);
                for id in var_tuple.iter_mut().flatten() {
                    self.bind(id);
                }
            }
            _ => {}
        }
        walk_expr_mut(self, e);
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        walk_stmt_mut(self, s);
        if let IrStmtKind::Bind { var, .. } = &mut s.kind {
            self.bind(var);
        }
    }
    fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
        if let IrPattern::Bind { var, .. } | IrPattern::As { var, .. } = p {
            self.bind(var);
        }
        walk_pattern_mut(self, p);
    }
}

/// Only `let`, destructuring, expression, guard and comment statements name
/// their variables where `Rename` rewrites them.
fn renamable(e: &IrExpr) -> bool {
    struct Scan {
        ok: bool,
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }) {
                self.ok = false;
            }
            if self.ok {
                walk_expr(self, e);
            }
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if !matches!(
                s.kind,
                IrStmtKind::Bind { .. }
                    | IrStmtKind::BindDestructure { .. }
                    | IrStmtKind::Expr { .. }
                    | IrStmtKind::Guard { .. }
                    | IrStmtKind::Comment { .. }
            ) {
                self.ok = false;
            }
            if self.ok {
                almide_ir::visit::walk_stmt(self, s);
            }
        }
    }
    let mut s = Scan { ok: true };
    s.visit_expr(e);
    s.ok
}

fn new_fn(name: Sym, params: Vec<IrParam>, ret_ty: Ty, body: IrExpr) -> IrFunction {
    IrFunction {
        name,
        params,
        ret_ty,
        body,
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Private,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![],
        module_origin: None,
    }
}

fn own_param(var: VarId, name: Sym, ty: Ty) -> IrParam {
    IrParam { var, ty, name, borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] }
}

impl Fuser<'_> {
    /// `let d = <tree>` read once, by `json.stringify(d)`: the bind holds
    /// the text and the stringify reads it.
    fn fuse_bound_trees(&mut self, stmts: &mut [IrStmt], tail: &mut Option<Box<IrExpr>>) {
        for i in 0..stmts.len() {
            let (head, rest) = stmts.split_at_mut(i + 1);
            let IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value } = &mut head[i].kind else { continue };
            if !is_value_ty(ty) || occurrences_in(rest, tail.as_deref(), *var) != 1 {
                continue;
            }
            let mut probe = StringifyOf { var: *var, text: None, hits: 0 };
            for s in rest.iter() {
                probe.visit_stmt(s);
            }
            if let Some(t) = tail.as_deref() {
                probe.visit_expr(t);
            }
            if probe.hits != 1 {
                continue;
            }
            let Some(text) = self.spelled(value, true) else { continue };
            let tv = self.vt.alloc_like(*var, Ty::String, Mutability::Let);
            *var = tv;
            *ty = Ty::String;
            *value = text;
            let mut swap = StringifyOf { var: probe.var, text: Some(tv), hits: 0 };
            for s in rest.iter_mut() {
                swap.visit_stmt_mut(s);
            }
            if let Some(t) = tail.as_deref_mut() {
                swap.visit_expr_mut(t);
            }
        }
    }

    /// The Value expression `e` as the String expression that is its text.
    /// At a `json.stringify` site (`site`) only an array of produced records
    /// is taken — `value.array(xs |> list.map((x) => g(x)))` — where the
    /// tree not built is one per element; a lone tree written once would
    /// trade a few constructor calls for a longer interpolation and gain
    /// nothing. Inside a twin every spelled shape is taken.
    fn spelled(&mut self, e: &IrExpr, site: bool) -> Option<IrExpr> {
        if site && !module_call(e, "value", "array").is_some_and(|a| matches!(a, [x] if module_call(x, "list", "map").is_some())) {
            return None;
        }
        let parts = Spell { writers: self.writers, wanted: Some(&mut self.twins_wanted) }.text_of(e)?;
        Some(match <[IrStringPart; 1]>::try_from(parts) {
            Ok([IrStringPart::Expr { expr }]) if matches!(expr.ty, Ty::String) => expr,
            Ok([IrStringPart::Lit { value }]) => str_expr(IrExprKind::LitStr { value }, e.span),
            Ok(one) => str_expr(IrExprKind::StringInterp { parts: one.into() }, e.span),
            Err(parts) => str_expr(IrExprKind::StringInterp { parts }, e.span),
        })
    }

    /// The writer twin of producer `src`: the same parameters and
    /// statements under fresh ids, the tail tree spelled as text.
    fn twin(&mut self, src: &IrFunction) -> IrFunction {
        let mut body = src.body.clone();
        let mut params = src.params.clone();
        let mut rn = Rename { vt: self.vt, map: HashMap::new() };
        for p in params.iter_mut() {
            rn.bind(&mut p.var);
        }
        rn.visit_expr_mut(&mut body);
        let spelled = match &mut body.kind {
            IrExprKind::Block { expr: Some(t), .. } => {
                let text = self.spelled(&**t, false).expect("a producer's tail is spelled (writer_fn)");
                **t = text;
                None
            }
            _ => Some(self.spelled(&body, false).expect("a producer's tail is spelled (writer_fn)")),
        };
        if let Some(text) = spelled {
            body = text;
        } else {
            body.ty = Ty::String;
        }
        new_fn(twin_name(src.name), params, Ty::String, body)
    }
}

/// Counts (`text` unset) or rewrites (`text` set) `json.stringify(<var>)`.
struct StringifyOf {
    var: VarId,
    text: Option<VarId>,
    hits: usize,
}

impl IrVisitor for StringifyOf {
    fn visit_expr(&mut self, e: &IrExpr) {
        if module_call(e, "json", "stringify").is_some_and(|a| matches!(a, [x] if is_var(x, self.var))) {
            self.hits += 1;
        }
        walk_expr(self, e);
    }
}

impl IrMutVisitor for StringifyOf {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if module_call(e, "json", "stringify").is_some_and(|a| matches!(a, [x] if is_var(x, self.var))) {
            if let Some(t) = self.text {
                e.kind = IrExprKind::Var { id: t };
                e.ty = Ty::String;
            }
            return;
        }
        walk_expr_mut(self, e);
    }
}

impl IrMutVisitor for Fuser<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if self.reader_live {
            self.fuse_inline_doc(e);
        }
        // Outermost first: a block's bound forms are rewritten where they
        // are bound before the inline forms inside them are looked at.
        if let IrExprKind::Block { stmts, expr } = &mut e.kind {
            if self.reader_live {
                self.fuse_bound_docs(stmts, expr);
            }
            if self.writer_live {
                self.fuse_bound_trees(stmts, expr);
            }
        }
        if self.writer_live {
            if let Some([v]) = module_call(e, "json", "stringify") {
                if !matches!(v.kind, IrExprKind::Var { .. }) {
                    if let Some(text) = self.spelled(v, true) {
                        *e = text;
                    }
                }
            }
        }
        walk_expr_mut(self, e);
    }
}
