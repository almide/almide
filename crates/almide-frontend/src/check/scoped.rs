// The `scoped` admission checker (#1997) — diagnostics E086 / E087 / E088.
//
// `scoped { … }` declares a reclamation boundary and `scoped fn` makes region
// eligibility part of a signature. Both are OBLIGATIONS: the checker admits
// exactly the stage-1 fragment both backends honour (docs/specs/scoped.md),
// and refuses everything else here, so native and wasm give one accept /
// refuse verdict and no shape outside the fragment passes silently. The
// backends never decide admission — a leg that cannot realize an admitted
// shape reports a compiler defect (E083 on wasm, a pass postcondition on
// native), never a wall.
//
// The rules mirror the region-pure vocabulary of `crates/almide-wasm/src/
// region.rs` and `crates/almide-codegen/src/pass_region_window.rs`, restricted
// to what the native twin machinery can clone: scalars, non-generic variant
// types whose payloads are scalars or such variants, records of scalars, and
// tuples / Option over those. Every rule names the construct it refuses and
// the declaration that required it.

// `BSpan` and `TypeConstructorId` come from bounded.rs, spliced into the
// same module ahead of this file.
use std::collections::{HashMap, HashSet};

impl Checker {
    pub(crate) fn check_scoped(&mut self, program: &ast::Program) {
        let mut fns: HashMap<&str, ScopedFn<'_>> = HashMap::new();
        for decl in &program.decls {
            if let ast::Decl::Fn { name, scoped, body, span, .. } = decl {
                fns.insert(
                    name.as_str(),
                    ScopedFn { scoped: *scoped, body: body.as_ref(), span: *span },
                );
            }
        }
        let any_scoped_fn = fns.values().any(|f| f.scoped);
        let any_block = program.decls.iter().any(|d| decl_has_scoped_block(d));
        if !any_scoped_fn && !any_block {
            return;
        }
        let mut diags: Vec<Diagnostic> = Vec::new();
        let file = self.source_file.clone().unwrap_or_default();
        if self.current_module_prefix.is_some() {
            // Stage 1 admits `scoped` in the entry program only: the native
            // region pass twins root functions and root type declarations.
            self.refuse_scoped_in_module(program, &fns, &file, &mut diags);
            self.diagnostics.extend(diags);
            return;
        }
        let admitted = AdmittedTypes::collect(program);
        for decl in &program.decls {
            match decl {
                ast::Decl::Fn { name, scoped: true, effect, params, generics, return_type, body, span, .. } => {
                    let mut cx = ScopedCx::new(self, &mut diags, &fns, &admitted, Subject::Fn(name.as_str()), *span, &file);
                    cx.check_scoped_fn_decl(name.as_str(), effect.unwrap_or(false), params, generics, return_type);
                    if let Some(body) = body {
                        cx.walk_scoped_fn_body(name.as_str(), params, body);
                    }
                }
                ast::Decl::Fn { name, body: Some(body), span, .. } => {
                    self.check_blocks_in(body, Subject::Block(name.as_str()), *span, &fns, &admitted, &file, &mut diags);
                }
                ast::Decl::Test { name, body, span, .. } => {
                    self.check_blocks_in(body, Subject::Block(name.as_str()), *span, &fns, &admitted, &file, &mut diags);
                }
                _ => {}
            }
        }
        self.diagnostics.extend(diags);
    }

    /// Every `scoped { … }` under `body` (outside scoped fns), each judged
    /// under the block rules with `subject` as the enclosing declaration.
    #[allow(clippy::too_many_arguments)]
    fn check_blocks_in(
        &self,
        body: &ast::Expr,
        subject: Subject<'_>,
        span: Option<BSpan>,
        fns: &HashMap<&str, ScopedFn<'_>>,
        admitted: &AdmittedTypes,
        file: &str,
        diags: &mut Vec<Diagnostic>,
    ) {
        let mut blocks: Vec<&ast::Expr> = Vec::new();
        collect_outermost_scoped_blocks(body, &mut blocks);
        for block in blocks {
            let mut cx = ScopedCx::new(self, diags, fns, admitted, subject, span, file);
            cx.walk_scoped_block(block);
        }
    }

    fn refuse_scoped_in_module(
        &self,
        program: &ast::Program,
        fns: &HashMap<&str, ScopedFn<'_>>,
        file: &str,
        diags: &mut Vec<Diagnostic>,
    ) {
        for (name, f) in fns {
            if f.scoped {
                diags.push(scoped_err(
                    format!("`scoped fn {name}` is not permitted in an imported module"),
                    "move the scoped worker into the entry file, or call a plain fn from a `scoped` block there",
                    "E087",
                    &format!("scoped fn {name}()"),
                    file,
                    f.span,
                ).with_note("stage 1 admits `scoped` in the entry program only"));
            }
        }
        for decl in &program.decls {
            let (name, body) = match decl {
                ast::Decl::Fn { name, body: Some(body), .. } => (name.as_str(), body),
                ast::Decl::Test { name, body, .. } => (name.as_str(), body),
                _ => continue,
            };
            let mut blocks = Vec::new();
            collect_outermost_scoped_blocks(body, &mut blocks);
            for b in blocks {
                diags.push(scoped_err(
                    "a `scoped` block is not permitted in an imported module".to_string(),
                    "move the block into the entry file",
                    "E087",
                    &format!("fn {name}()"),
                    file,
                    b.span,
                ).with_note("stage 1 admits `scoped` in the entry program only"));
            }
        }
    }
}

struct ScopedFn<'a> {
    scoped: bool,
    body: Option<&'a ast::Expr>,
    span: Option<BSpan>,
}

#[derive(Clone, Copy)]
enum Subject<'a> {
    /// The body of `scoped fn <name>`.
    Fn(&'a str),
    /// A `scoped { … }` block inside declaration `<name>`.
    Block(&'a str),
}

fn decl_has_scoped_block(d: &ast::Decl) -> bool {
    let body = match d {
        ast::Decl::Fn { body: Some(b), .. } => b,
        ast::Decl::Test { body, .. } => body,
        ast::Decl::TopLet { value, .. } => value,
        _ => return false,
    };
    let mut found = false;
    ast::visit_expr(body, &mut |e| {
        if matches!(e.kind, ast::ExprKind::Scoped { .. }) {
            found = true;
        }
    });
    found
}

/// The outermost `scoped` blocks under `e` — a block nested in another is
/// judged by the enclosing block's walk.
fn collect_outermost_scoped_blocks<'e>(e: &'e ast::Expr, out: &mut Vec<&'e ast::Expr>) {
    if matches!(e.kind, ast::ExprKind::Scoped { .. }) {
        out.push(e);
        return;
    }
    for c in expr_children(e) {
        collect_outermost_scoped_blocks(c, out);
    }
    for s in stmt_children(e) {
        for c in stmt_exprs(s) {
            collect_outermost_scoped_blocks(c, out);
        }
    }
}

// ── the admitted type fragment ────────────────────────────────────────

struct AdmittedTypes {
    variants: HashSet<String>,
    records: HashSet<String>,
}

fn is_scalar_name(n: &str) -> bool {
    matches!(n, "Int" | "Float" | "Bool" | "Unit")
}

fn is_scalar_ty(t: &Ty) -> bool {
    matches!(t, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit)
}

impl AdmittedTypes {
    /// Variants: declared here, non-generic, every case unit or tuple, every
    /// payload a scalar or another admitted variant (a greatest fixpoint, so
    /// `Tree = Leaf | Node(Tree, Tree)` admits itself). Records: declared
    /// here, non-generic, every field a scalar.
    fn collect(program: &ast::Program) -> Self {
        let mut candidates: HashMap<String, Vec<String>> = HashMap::new();
        let mut records = HashSet::new();
        for decl in &program.decls {
            let ast::Decl::Type { name, ty, generics, .. } = decl else { continue };
            if generics.as_ref().is_some_and(|g| !g.is_empty()) {
                continue;
            }
            match ty {
                ast::TypeExpr::Variant { cases, .. } => {
                    let mut payloads = Vec::new();
                    let mut ok = true;
                    for c in cases {
                        match c {
                            ast::VariantCase::Unit { .. } => {}
                            ast::VariantCase::Tuple { fields, .. } => {
                                for f in fields {
                                    match f {
                                        ast::TypeExpr::Simple { name } => payloads.push(name.to_string()),
                                        _ => ok = false,
                                    }
                                }
                            }
                            ast::VariantCase::Record { .. } => ok = false,
                        }
                    }
                    if ok {
                        candidates.insert(name.to_string(), payloads);
                    }
                }
                ast::TypeExpr::Record { fields } => {
                    let scalar = fields.iter().all(|f| matches!(&f.ty, ast::TypeExpr::Simple { name } if is_scalar_name(name.as_str())));
                    if scalar {
                        records.insert(name.to_string());
                    }
                }
                _ => {}
            }
        }
        let mut variants: HashSet<String> = candidates.keys().cloned().collect();
        loop {
            let before = variants.len();
            let snapshot = variants.clone();
            variants.retain(|v| candidates[v].iter().all(|p| is_scalar_name(p) || snapshot.contains(p)));
            if variants.len() == before {
                break;
            }
        }
        AdmittedTypes { variants, records }
    }

    /// Is a resolved type inside the fragment? Unresolved types are admitted
    /// (another diagnostic owns them).
    fn admits(&self, t: &Ty) -> bool {
        match t {
            _ if is_scalar_ty(t) => true,
            Ty::Unknown | Ty::TypeVar(_) => true,
            Ty::Named(n, args) if args.is_empty() => {
                self.variants.contains(n.as_str()) || self.records.contains(n.as_str())
            }
            Ty::Variant { name, .. } => self.variants.contains(name.as_str()),
            Ty::Record { fields } => fields.iter().all(|(_, ft)| is_scalar_ty(ft)),
            Ty::Tuple(ts) => ts.iter().all(|x| self.admits(x)),
            Ty::Applied(TypeConstructorId::Option, args) => args.iter().all(|x| self.admits(x)),
            _ => false,
        }
    }
}

// ── diagnostics ───────────────────────────────────────────────────────

fn scoped_err(msg: String, hint: &str, code: &'static str, ctx: &str, file: &str, span: Option<BSpan>) -> Diagnostic {
    let d = Diagnostic::error(msg, hint.to_string(), ctx.to_string());
    // spelled as literals so the diagnostic-coverage scanner sees each code
    let mut d = match code {
        "E086" => d.with_code("E086"),
        "E087" => d.with_code("E087"),
        _ => d.with_code("E088"),
    };
    if !file.is_empty() {
        d.file = Some(file.to_string());
    }
    if let Some(s) = span {
        d.line = Some(s.line);
        d.col = Some(s.col);
        if s.end_col > s.col {
            d.end_col = Some(s.end_col);
        }
    }
    d
}

fn at(file: &str, s: Option<BSpan>) -> String {
    match s {
        Some(s) if !file.is_empty() => format!("{file}:{}:{}", s.line, s.col),
        Some(s) => format!("{}:{}", s.line, s.col),
        None => "an unknown position".to_string(),
    }
}

/// Stdlib modules whose members are scalar-in / scalar-out: the region-pure
/// whitelist of both backends (`SCALAR_MODULES`).
const SCOPED_SCALAR_MODULES: &[&str] = &["int", "float", "math", "bool"];

struct ScopedCx<'a, 'c> {
    checker: &'c Checker,
    diags: &'a mut Vec<Diagnostic>,
    fns: &'a HashMap<&'a str, ScopedFn<'a>>,
    admitted: &'a AdmittedTypes,
    subject: Subject<'a>,
    subject_span: Option<BSpan>,
    file: &'a str,
    /// Locally bound names, innermost scope last.
    scopes: Vec<Vec<String>>,
    /// Scopes at index >= this floor were opened INSIDE the scoped block, so
    /// a name bound below the floor is a capture from the enclosing fn.
    block_floor: usize,
    /// The span of the `}` that ends the scoped block being judged.
    block_end: Option<BSpan>,
    loop_depth: u32,
    /// E088: the ExprIds of self-calls in tail position of the scoped fn.
    tail_calls: HashSet<ast::ExprId>,
    /// E086: where each `let`-bound name inside the block got its value —
    /// the `allocated at` row names the allocation, not the last mention.
    let_sites: HashMap<String, Option<BSpan>>,
}

impl<'a, 'c> ScopedCx<'a, 'c> {
    fn new(
        checker: &'c Checker,
        diags: &'a mut Vec<Diagnostic>,
        fns: &'a HashMap<&'a str, ScopedFn<'a>>,
        admitted: &'a AdmittedTypes,
        subject: Subject<'a>,
        subject_span: Option<BSpan>,
        file: &'a str,
    ) -> Self {
        ScopedCx {
            checker, diags, fns, admitted, subject, subject_span, file,
            scopes: vec![Vec::new()], block_floor: usize::MAX, block_end: None,
            loop_depth: 0, tail_calls: HashSet::new(), let_sites: HashMap::new(),
        }
    }

    fn where_(&self) -> &'static str {
        match self.subject {
            Subject::Fn(_) => "in a scoped function",
            Subject::Block(_) => "in a scoped block",
        }
    }

    fn required_by(&self) -> String {
        match self.subject {
            Subject::Fn(n) => format!("required by the `scoped` declaration of `{n}`"),
            Subject::Block(n) => format!("required by the `scoped` block in `{n}`"),
        }
    }

    fn ctx(&self) -> String {
        match self.subject {
            Subject::Fn(n) => format!("scoped fn {n}()"),
            Subject::Block(n) => format!("scoped block in {n}()"),
        }
    }

    /// E087: `<construct> is not permitted in a scoped …`, with the reason
    /// row and the declaration that required it.
    fn refuse(&mut self, construct: &str, reason: &str, hint: &str, span: Option<BSpan>) {
        let d = scoped_err(
            format!("{construct} is not permitted {}", self.where_()),
            hint,
            "E087",
            &self.ctx(),
            self.file,
            span.or(self.subject_span),
        )
        .with_note(reason)
        .with_note(self.required_by());
        self.diags.push(d);
    }

    fn ty_of(&self, e: &ast::Expr) -> Option<&Ty> {
        self.checker.type_map.get(&e.id)
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.iter().any(|n| n == name))
    }

    /// Bound inside the scoped block (at or above the floor)?
    fn is_block_local(&self, name: &str) -> bool {
        self.scopes.iter().skip(self.block_floor.min(self.scopes.len())).any(|s| s.iter().any(|n| n == name))
    }

    fn is_global(&self, name: &str) -> bool {
        self.checker.env.top_lets.contains_key(&sym(name))
    }

    fn bind(&mut self, name: &str) {
        if let Some(s) = self.scopes.last_mut() {
            s.push(name.to_string());
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    // ── declaration-level rules ───────────────────────────────────────

    fn check_scoped_fn_decl(
        &mut self,
        name: &str,
        effect: bool,
        params: &[ast::Param],
        generics: &Option<Vec<ast::GenericParam>>,
        return_type: &ast::TypeExpr,
    ) {
        let span = self.subject_span;
        if name == "main" {
            self.refuse("`main`", "the entry point is never a region worker", "drop the qualifier from `main` and open a `scoped` block inside it", span);
        }
        if effect {
            self.refuse("`effect`", "an effect fn reaches host resources outside the allocation region", "keep the effect outside: call the scoped fn from an effect fn", span);
        }
        if generics.as_ref().is_some_and(|g| !g.is_empty()) {
            self.refuse("a generic parameter", "the region needs one concrete layout per type", "declare a monomorphic `scoped fn` per element type", span);
        }
        for p in params {
            if p.is_mut {
                self.refuse(&format!("the `mut` parameter `{}`", p.name), "it writes through to storage outside the allocation region", "take the value by `let` and return the updated value", span);
            }
            if !self.admits_type_expr(&p.ty) {
                let shown = type_expr_text(&p.ty);
                self.refuse(&format!("the `{shown}` parameter `{}`", p.name), &self.fragment_reason(&shown), "pass a scalar or an admitted variant, or build the value inside the scope", span);
            }
        }
        if !self.admits_type_expr(return_type) {
            let shown = type_expr_text(return_type);
            self.refuse(&format!("the `{shown}` return type"), &self.fragment_reason(&shown), "return a scalar or an admitted variant", span);
        }
    }

    fn fragment_reason(&self, shown: &str) -> String {
        format!("`{shown}` is outside the scoped fragment (scalars, variants of scalars, records of scalars, tuples and Option of those)")
    }

    fn admits_type_expr(&self, t: &ast::TypeExpr) -> bool {
        match t {
            ast::TypeExpr::Simple { name } => {
                let n = name.as_str();
                is_scalar_name(n) || self.admitted.variants.contains(n) || self.admitted.records.contains(n)
            }
            ast::TypeExpr::Tuple { elements } => elements.iter().all(|e| self.admits_type_expr(e)),
            ast::TypeExpr::Generic { name, args } if name.as_str() == "Option" => args.iter().all(|e| self.admits_type_expr(e)),
            ast::TypeExpr::Record { fields } => fields.iter().all(|f| matches!(&f.ty, ast::TypeExpr::Simple { name } if is_scalar_name(name.as_str()))),
            _ => false,
        }
    }

    // ── the two walks ─────────────────────────────────────────────────

    fn walk_scoped_fn_body(&mut self, name: &str, params: &[ast::Param], body: &ast::Expr) {
        for p in params {
            self.bind(p.name.as_str());
        }
        self.check_mutual_recursion(name, body);
        let mut leaves = Vec::new();
        Checker::tail_leaves(body, &mut leaves);
        for leaf in leaves {
            match &leaf.kind {
                ast::ExprKind::Call { .. } => { self.tail_calls.insert(leaf.id); }
                ast::ExprKind::Pipe { right, .. } => { self.tail_calls.insert(right.id); }
                _ => {}
            }
        }
        self.check_tail_shape(name, body, body);
        self.walk_expr(body);
    }

    /// The block rules: scalar captures only, no outer writes, a scalar
    /// value — then the shared body rules.
    fn walk_scoped_block(&mut self, block: &ast::Expr) {
        let ast::ExprKind::Scoped { body, end } = &block.kind else { return };
        let saved = (self.block_floor, self.block_end);
        self.block_floor = self.scopes.len();
        self.block_end = end.or(block.span);
        self.push_scope();
        self.walk_expr(body);
        self.pop_scope();
        // E086: the block's value must be a scalar — anything else would
        // outlive the allocation it was built from.
        if let Some(t) = self.ty_of(body).cloned() {
            if !is_scalar_ty(&t) && !matches!(t, Ty::Unknown | Ty::TypeVar(_)) {
                let tail = block_tail(body).unwrap_or(body);
                let (what, allocated) = match &tail.kind {
                    ast::ExprKind::Ident { name } => (
                        format!("`{name}`"),
                        self.let_sites.get(name.as_str()).copied().flatten().or(tail.span),
                    ),
                    _ => (format!("this `{}`", t.display()), tail.span),
                };
                let d = scoped_err(
                    format!("{what} would outlive its scoped allocation"),
                    "compute and return the scalar summary inside the scope",
                    "E086",
                    &self.ctx(),
                    self.file,
                    tail.span,
                )
                .with_note(format!("allocated at {}", at(self.file, allocated)))
                .with_note(format!("scope ends at {}", at(self.file, self.block_end)));
                self.diags.push(d);
            }
        }
        (self.block_floor, self.block_end) = saved;
    }

    /// Every expression's resolved type must be inside the fragment.
    fn check_type(&mut self, e: &ast::Expr) {
        let Some(t) = self.ty_of(e).cloned() else { return };
        if !self.admitted.admits(&t) {
            let shown = t.display();
            self.refuse(&format!("a `{shown}` value"), &self.fragment_reason(&shown), "compute with scalars and admitted variants inside the scope", e.span);
        }
    }

    fn check_ident(&mut self, name: &str, e: &ast::Expr) {
        if self.is_local(name) {
            if self.block_floor != usize::MAX && !self.is_block_local(name) {
                self.check_capture(name, e);
            }
            return;
        }
        if self.is_global(name) {
            self.refuse(&format!("`{name}`"), "a global lives outside the allocation region", "pass the value in as a parameter", e.span);
            return;
        }
        if self.fns.contains_key(name) || self.checker.env.functions.contains_key(&sym(name)) {
            self.refuse(&format!("the function value `{name}`"), "an indirect callee is unknown, so what it retains is unknown", "call a `scoped fn` by name", e.span);
            return;
        }
        // Not bound inside the block: a capture from the enclosing fn.
        if self.block_floor != usize::MAX {
            self.check_capture(name, e);
        }
    }

    /// A scoped block may capture scalars only: the value is snapshotted
    /// into the region's entry, and a heap value would be a view into
    /// storage the region does not own.
    fn check_capture(&mut self, name: &str, e: &ast::Expr) {
        let Some(t) = self.ty_of(e).cloned() else { return };
        if !is_scalar_ty(&t) && !matches!(t, Ty::Unknown | Ty::TypeVar(_)) {
            self.refuse(&format!("`{name}`"), &format!("captured from outside the allocation region (a `{}`)", t.display()), "pass a scalar in, or build the value inside the scope", e.span);
        }
    }

    /// `left |> right`, judged as the call it spells (`a |> f(b)` is
    /// `f(a, b)`, `a |> f` is `f(a)`).
    fn walk_pipe(&mut self, pipe: &ast::Expr, left: &ast::Expr, right: &ast::Expr) {
        match &right.kind {
            ast::ExprKind::Call { callee, args, .. } => {
                let spelled: Vec<ast::Expr> = std::iter::once(left.clone()).chain(args.iter().cloned()).collect();
                if self.check_call(right, callee, &spelled) {
                    self.check_type(pipe);
                }
                self.walk_expr(left);
                for a in args {
                    self.walk_expr(a);
                }
            }
            ast::ExprKind::Ident { .. } | ast::ExprKind::Member { .. } => {
                if self.check_call(pipe, right, std::slice::from_ref(left)) {
                    self.check_type(pipe);
                }
                self.walk_expr(left);
            }
            _ => {
                self.refuse("an indirect call", "the callee is unknown, so what it retains is unknown", "call a `scoped fn` by name", pipe.span);
                self.walk_expr(left);
            }
        }
    }

    /// The callee rule. Returns whether the call itself was admitted (so the
    /// caller judges its result type; a refused call is reported once).
    fn check_call(&mut self, call: &ast::Expr, callee: &ast::Expr, args: &[ast::Expr]) -> bool {
        match &callee.kind {
            ast::ExprKind::Ident { name } => {
                let n = name.as_str();
                if self.is_local(n) {
                    self.refuse("an indirect call", "the callee is unknown, so what it retains is unknown", "call a `scoped fn` by name", call.span);
                    return false;
                }
                if let Some(f) = self.fns.get(n) {
                    if !f.scoped {
                        let retained = self.retained_arg(args);
                        self.refuse(&format!("`{n}`"), &format!("may retain {retained} outside the allocation region (it is not declared `scoped`)"), &format!("declare it `scoped fn {n}`, or do the work inside the scope"), call.span);
                        return false;
                    }
                    return true;
                }
                if n.starts_with("print") || n.starts_with("eprint") {
                    self.refuse(&format!("`{n}`"), "output goes to the host, outside the allocation region", "return the value and print it outside the scope", call.span);
                    return false;
                }
                if n == "assert" || n == "assert_eq" {
                    self.refuse(&format!("`{n}`"), "an assertion reports through the host, outside the allocation region", "return the value and assert outside the scope", call.span);
                    return false;
                }
                // A bare unknown callee: E002 owns it.
                false
            }
            ast::ExprKind::Member { object, field } => {
                let ast::ExprKind::Ident { name: module } = &object.kind else {
                    self.refuse("an indirect call", "the callee is unknown, so what it retains is unknown", "call a `scoped fn` by name", call.span);
                    return false;
                };
                let m0 = module.as_str();
                if self.is_local(m0) {
                    // UFCS on a local: a method call the region cannot follow.
                    self.refuse(&format!("`{m0}.{field}`"), "a method call on a value is an indirect callee", "call a `scoped fn` by name", call.span);
                    return false;
                }
                let resolved = self.checker.env.import_table.resolve(m0).map(|s| s.as_str().to_string()).unwrap_or_else(|| m0.to_string());
                let m = resolved.as_str();
                if SCOPED_SCALAR_MODULES.contains(&m) {
                    return true;
                }
                let retained = self.retained_arg(args);
                let reason = if almide_lang::stdlib_info::is_any_stdlib(m) {
                    format!("may retain {retained} outside the allocation region (only `int`, `float`, `math` and `bool` calls are admitted)")
                } else {
                    format!("may retain {retained} outside the allocation region (a call across the module boundary)")
                };
                self.refuse(&format!("`{m0}.{field}`"), &reason, "compute with scalar operators and `scoped fn` helpers inside the scope", call.span);
                false
            }
            // A variant constructor builds a value; its type is judged by the caller.
            ast::ExprKind::TypeName { .. } => true,
            _ => {
                self.refuse("an indirect call", "the callee is unknown, so what it retains is unknown", "call a `scoped fn` by name", call.span);
                false
            }
        }
    }

    /// "`tree`" when an argument names a region value, else a generic phrase.
    fn retained_arg(&self, args: &[ast::Expr]) -> String {
        for a in args {
            if let ast::ExprKind::Ident { name } = &a.kind
                && let Some(t) = self.ty_of(a)
                && !is_scalar_ty(t)
            {
                return format!("`{name}`");
            }
        }
        "the region's values".to_string()
    }
}
