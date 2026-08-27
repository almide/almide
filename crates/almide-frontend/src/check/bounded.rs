use almide_base::span::Span as BSpan;
use almide_lang::types::constructor::TypeConstructorId;

// The @bounded profile checker — ALS §B (ADR-0017), diagnostics E070–E078.
//
// `@bounded` marks a fn as belonging to the BOUNDED PROFILE: statically
// bounded time and storage, a closed acyclic call graph, capability-bounded
// effects. The profile is a SUBSET, not a dialect (ALS-B2): the attribute
// changes nothing about types, values or observable behaviour — it only
// widens what the checker rejects, and only INSIDE attributed functions.
//
// Every rule below is normative in docs/specs/als/bounded.md (canonical copy
// in almide/als) and pinned by tests/diagnostics/e07x-bounded-*; the message
// shape is `<construct> is not admissible in a @bounded function` with the
// hint each section names.

impl Checker {
    pub(crate) fn check_bounded_profile(&mut self, program: &ast::Program) {
        use std::collections::HashMap;
        // the user fn index: name -> (is_bounded, body) — the call-graph nodes
        let mut fns: HashMap<&str, (bool, Option<&ast::Expr>)> = HashMap::new();
        for decl in &program.decls {
            if let ast::Decl::Fn { name, attrs, body, .. } = decl {
                let is_bounded = attrs.iter().any(|a| a.name.as_str() == "bounded");
                fns.insert(name.as_str(), (is_bounded, body.as_ref()));
            }
        }
        if !fns.values().any(|(b, _)| *b) {
            return;
        }
        let mut diags: Vec<Diagnostic> = Vec::new();
        for decl in &program.decls {
            let ast::Decl::Fn { name, attrs, effect, body: Some(body), span, .. } = decl else {
                continue;
            };
            if !attrs.iter().any(|a| a.name.as_str() == "bounded") {
                continue;
            }
            // ALS-B6 / E073: the reachable call graph must be a DAG — a DFS
            // from this fn over user-fn edges that revisits it is recursion
            if user_call_cycle(name.as_str(), &fns) {
                diags.push(bounded_err(
                    "recursion is not admissible in a @bounded function",
                    "the reachable call graph must be a DAG — replace the recursion with a counted loop",
                    "E073",
                    name.as_str(),
                    *span,
                ));
            }
            let mut cx = BoundedCx {
                checker: self,
                diags: &mut diags,
                fns: &fns,
                fn_name: name.as_str(),
                fn_span: *span,
                is_effect: effect.unwrap_or(false),
                consts: Vec::new(),
                loop_depth: 0,
            };
            cx.walk_expr(body);
        }
        for mut d in diags {
            if d.file.is_none() {
                d.file = self.source_file.clone();
            }
            self.diagnostics.push(d);
        }
    }
}

/// Pure stdlib modules a @bounded fn may call FIRST-ORDER members of (ALS-B7).
const BOUNDED_PURE_MODULES: &[&str] = &[
    "int", "int8", "int16", "int32", "int64", "uint8", "uint16", "uint32", "uint64", "float",
    "float32", "string", "list", "map", "set", "tuple", "math", "option", "result", "value",
    "json", "bytes", "regex", "matrix", "bool", "char", "hex", "base64", "url", "html",
];

/// Effect / host-reaching modules E076 rejects outright (ALS-B9); `io` is
/// special-cased to its print family, the bare `println` builtins are allowed.
const BOUNDED_DENIED_MODULES: &[&str] = &[
    "env", "fs", "http", "net", "process", "random", "zlib", "datetime", "args", "mem",
    "testing", "fan", "compute", "duration", "log", "time",
];

/// Run-time-length heap constructors (ALS-B8): fn -> the size-argument slots
/// that must be compile-time constants.
const BOUNDED_SIZED_CTORS: &[(&str, &[usize])] = &[
    ("string.repeat", &[1]),
    ("list.with_capacity", &[0]),
    ("list.range", &[0, 1]),
    ("list.repeat", &[1]),
    ("bytes.new", &[0]),
    ("bytes.repeat", &[1]),
    ("string.pad_start", &[1]),
    ("string.pad_end", &[1]),
];

/// In-loop allocating calls (ALS-B4).
const BOUNDED_ALLOC_CALLS: &[&str] = &[
    "list.push", "string.push", "bytes.push", "list.concat", "string.concat", "list.append",
    "map.insert", "set.insert", "bytes.append",
];

fn bounded_err(
    msg: &str,
    hint: &str,
    code: &'static str,
    fn_name: &str,
    span: Option<BSpan>,
) -> Diagnostic {
    let d = Diagnostic::error(
        msg.to_string(),
        hint.to_string(),
        format!("@bounded fn {}()", fn_name),
    );
    // spelled as literals so the diagnostic-coverage scanner sees each code
    let mut d = match code {
        "E070" => d.with_code("E070"),
        "E071" => d.with_code("E071"),
        "E072" => d.with_code("E072"),
        "E073" => d.with_code("E073"),
        "E074" => d.with_code("E074"),
        "E075" => d.with_code("E075"),
        "E076" => d.with_code("E076"),
        "E077" => d.with_code("E077"),
        _ => d.with_code("E078"),
    };
    if let Some(s) = span {
        d.line = Some(s.line);
        d.col = Some(s.col);
        d.end_col = Some(s.end_col);
    }
    d
}

/// DFS over USER-fn call edges: does any path from `start` revisit `start`?
fn user_call_cycle<'a>(
    start: &str,
    fns: &std::collections::HashMap<&'a str, (bool, Option<&'a ast::Expr>)>,
) -> bool {
    fn edges<'a>(
        body: &'a ast::Expr,
        fns: &std::collections::HashMap<&'a str, (bool, Option<&'a ast::Expr>)>,
        out: &mut Vec<&'a str>,
    ) {
        ast::visit_expr(body, &mut |e| {
            if let ast::ExprKind::Call { callee, .. } = &e.kind {
                if let ast::ExprKind::Ident { name } = &callee.kind {
                    if let Some((k, _)) = fns.get_key_value(name.as_str()) {
                        out.push(k);
                    }
                }
            }
        });
    }
    let mut stack = vec![start];
    let mut seen: Vec<&str> = Vec::new();
    let mut first = true;
    while let Some(f) = stack.pop() {
        if !first && f == start {
            return true;
        }
        first = false;
        if seen.contains(&f) {
            continue;
        }
        seen.push(f);
        if let Some((_, Some(body))) = fns.get(f) {
            let mut es = Vec::new();
            edges(body, fns, &mut es);
            for e in es {
                if e == start {
                    return true;
                }
                stack.push(e);
            }
        }
    }
    false
}

struct BoundedCx<'a, 'c> {
    checker: &'c Checker,
    diags: &'a mut Vec<Diagnostic>,
    fns: &'a std::collections::HashMap<&'a str, (bool, Option<&'a ast::Expr>)>,
    fn_name: &'a str,
    fn_span: Option<BSpan>,
    is_effect: bool,
    /// let-bound names currently known to be integer literals (ALS-B3's
    /// "constant-foldable from a literal"); shadowing/mutation removes them
    consts: Vec<String>,
    loop_depth: u32,
}

impl BoundedCx<'_, '_> {
    fn err(&mut self, msg: &str, hint: &str, code: &'static str, span: Option<BSpan>) {
        self.diags.push(bounded_err(
            msg,
            hint,
            code,
            self.fn_name,
            span.or(self.fn_span),
        ));
    }

    fn is_const_int(&self, e: &ast::Expr) -> bool {
        match &e.kind {
            ast::ExprKind::Int { .. } => true,
            ast::ExprKind::Unary { op, operand } if op.as_str() == "-" => self.is_const_int(operand),
            ast::ExprKind::Ident { name } => self.consts.iter().any(|c| c == name.as_str()),
            _ => false,
        }
    }

    fn expr_is_float(&self, e: &ast::Expr) -> bool {
        matches!(
            self.checker.type_map.get(&e.id),
            Some(Ty::Float | Ty::Float64 | Ty::Float32)
        )
    }

    fn expr_is_heap(&self, e: &ast::Expr) -> bool {
        matches!(
            self.checker.type_map.get(&e.id),
            Some(Ty::String)
                | Some(Ty::Applied(
                    TypeConstructorId::List | TypeConstructorId::Map | TypeConstructorId::Set,
                    _
                ))
        )
    }

    fn walk_stmts(&mut self, stmts: &[ast::Stmt]) {
        let const_mark = self.consts.len();
        for st in stmts {
            match st {
                ast::Stmt::Let { name, value, .. } => {
                    self.walk_expr(value);
                    self.consts.retain(|c| c != name.as_str());
                    if self.is_const_int(value) {
                        self.consts.push(name.as_str().to_string());
                    }
                }
                ast::Stmt::Var { name, value, .. } => {
                    self.walk_expr(value);
                    self.consts.retain(|c| c != name.as_str());
                }
                ast::Stmt::LetDestructure { value, .. } => self.walk_expr(value),
                ast::Stmt::Assign { name, value, span } => {
                    self.consts.retain(|c| c != name.as_str());
                    // ALS-B4: growing a heap binding inside the loop is a
                    // per-iteration allocation whichever way it is spelled
                    if self.loop_depth > 0 && self.expr_is_heap(value) {
                        if let ast::ExprKind::Binary { op, .. } = &value.kind {
                            if op.as_str() == "+" {
                                self.err(
                                    "allocation inside a counted loop is not admissible in a @bounded function",
                                    "allocate outside the loop and keep the loop body allocation-free",
                                    "E071",
                                    *span,
                                );
                            }
                        }
                    }
                    self.walk_expr(value);
                }
                ast::Stmt::IndexAssign { index, value, .. } => {
                    self.walk_expr(index);
                    self.walk_expr(value);
                }
                ast::Stmt::FieldAssign { value, .. } => self.walk_expr(value),
                ast::Stmt::Guard { cond, else_, span } => {
                    // ALS-B11 / E078: a guard inside a counted loop body is an
                    // early exit past the iteration bound
                    if self.loop_depth > 0 {
                        self.err(
                            "a guard inside a counted loop is not admissible in a @bounded function",
                            "move the guard outside the loop and keep each iteration total",
                            "E078",
                            *span,
                        );
                    }
                    self.walk_expr(cond);
                    self.walk_expr(else_);
                }
                ast::Stmt::GuardLet { scrutinee, else_, span, .. } => {
                    if self.loop_depth > 0 {
                        self.err(
                            "a guard inside a counted loop is not admissible in a @bounded function",
                            "move the guard outside the loop and keep each iteration total",
                            "E078",
                            *span,
                        );
                    }
                    self.walk_expr(scrutinee);
                    self.walk_expr(else_);
                }
                ast::Stmt::Expr { expr, .. } => self.walk_expr(expr),
                ast::Stmt::Comment { .. } | ast::Stmt::Error { .. } => {}
            }
        }
        self.consts.truncate(const_mark);
    }

    fn walk_expr(&mut self, e: &ast::Expr) {
        match &e.kind {
            ast::ExprKind::While { cond, body } => {
                // ALS-B3 / E070: `while` has no static trip count
                self.err(
                    "a `while` loop is not admissible in a @bounded function",
                    "use a counted range — `for i in a..<b` with compile-time-constant bounds (counted range)",
                    "E070",
                    e.span,
                );
                self.walk_expr(cond);
                self.loop_depth += 1;
                self.walk_stmts(body);
                self.loop_depth -= 1;
            }
            ast::ExprKind::ForIn { iterable, body, .. } => {
                let counted = match &iterable.kind {
                    ast::ExprKind::Range { start, end, .. } => {
                        self.is_const_int(start) && self.is_const_int(end)
                    }
                    _ => false,
                };
                if !counted {
                    self.err(
                        "a loop without a compile-time trip count is not admissible in a @bounded function",
                        "iterate a counted range with compile-time-constant bounds (counted range)",
                        "E070",
                        e.span,
                    );
                }
                self.walk_expr(iterable);
                self.loop_depth += 1;
                self.walk_stmts(body);
                self.loop_depth -= 1;
            }
            ast::ExprKind::Break | ast::ExprKind::Continue => {
                // ALS-B5 / E072
                self.err(
                    "`break`/`continue` is not admissible in a @bounded function",
                    "a counted loop has a single exit — restructure without early exits (single exit)",
                    "E072",
                    e.span,
                );
            }
            ast::ExprKind::Unwrap { expr } => {
                // ALS-B11 / E078: `!` inside a counted loop body
                if self.loop_depth > 0 {
                    self.err(
                        "`!` propagation inside a counted loop is not admissible in a @bounded function",
                        "propagate outside the loop; inside it, default with `??` instead",
                        "E078",
                        e.span,
                    );
                }
                self.walk_expr(expr);
            }
            ast::ExprKind::Lambda { body, .. } => {
                // ALS-B7 / E074 (and B4 in a loop): a closure is an indirect
                // callee the profile's static call graph cannot follow
                let code: &'static str = if self.loop_depth > 0 { "E071" } else { "E074" };
                if code == "E071" {
                    self.err(
                        "creating a closure inside a counted loop is not admissible in a @bounded function",
                        "allocate outside the loop and keep the loop body allocation-free",
                        "E071",
                        e.span,
                    );
                } else {
                    self.err(
                        "creating a closure is not admissible in a @bounded function",
                        "call @bounded functions or first-order pure stdlib members directly (@bounded callee)",
                        "E074",
                        e.span,
                    );
                }
                self.walk_expr(body);
            }
            ast::ExprKind::List { elements } => {
                if self.loop_depth > 0 {
                    self.err(
                        "constructing a list inside a counted loop is not admissible in a @bounded function",
                        "allocate outside the loop and keep the loop body allocation-free",
                        "E071",
                        e.span,
                    );
                }
                for el in elements {
                    self.walk_expr(el);
                }
            }
            ast::ExprKind::MapLiteral { entries } => {
                if self.loop_depth > 0 {
                    self.err(
                        "constructing a map inside a counted loop is not admissible in a @bounded function",
                        "allocate outside the loop and keep the loop body allocation-free",
                        "E071",
                        e.span,
                    );
                }
                for (k, v) in entries {
                    self.walk_expr(k);
                    self.walk_expr(v);
                }
            }
            ast::ExprKind::InterpolatedString { .. } => {
                if self.loop_depth > 0 {
                    self.err(
                        "building a string inside a counted loop is not admissible in a @bounded function",
                        "allocate outside the loop and keep the loop body allocation-free",
                        "E071",
                        e.span,
                    );
                }
                self.walk_children(e);
            }
            ast::ExprKind::Binary { op, left, right } => {
                let o = op.as_str();
                // ALS-B10 / E077: Float operators (arithmetic and comparison)
                let arith_or_cmp = matches!(
                    o,
                    "+" | "-" | "*" | "/" | "%" | "^" | "<" | "<=" | ">" | ">=" | "==" | "!="
                );
                if arith_or_cmp && (self.expr_is_float(left) || self.expr_is_float(right)) {
                    self.err(
                        "a Float operation is not admissible in a @bounded function",
                        "keep to Int arithmetic — Float values may be held and passed, not computed on",
                        "E077",
                        e.span,
                    );
                }
                // ALS-B4 / E071: heap concatenation inside a counted loop
                if self.loop_depth > 0
                    && o == "+"
                    && (self.expr_is_heap(left) || self.expr_is_heap(right))
                {
                    self.err(
                        "heap concatenation inside a counted loop is not admissible in a @bounded function",
                        "allocate outside the loop and keep the loop body allocation-free",
                        "E071",
                        e.span,
                    );
                }
                self.walk_expr(left);
                self.walk_expr(right);
            }
            ast::ExprKind::Unary { op, operand } => {
                if op.as_str() == "-" && self.expr_is_float(operand) {
                    self.err(
                        "a Float operation is not admissible in a @bounded function",
                        "keep to Int arithmetic — Float values may be held and passed, not computed on",
                        "E077",
                        e.span,
                    );
                }
                self.walk_expr(operand);
            }
            ast::ExprKind::Call { callee, args, .. } => {
                self.check_call(e, callee, args);
                for a in args {
                    self.walk_expr(a);
                }
                // walk the callee only when it is not a plain name (a lambda
                // callee must be flagged by its own arm)
                if !matches!(
                    &callee.kind,
                    ast::ExprKind::Ident { .. } | ast::ExprKind::Member { .. }
                ) {
                    self.walk_expr(callee);
                }
            }
            // fan.* forms never enter the profile (ALS-B9's scheduling clause)
            ast::ExprKind::Fan { .. }
            | ast::ExprKind::FanBounded { .. }
            | ast::ExprKind::FanRace { .. }
            | ast::ExprKind::FanRaceMap { .. }
            | ast::ExprKind::FanTimeout { .. }
            | ast::ExprKind::FanSettle { .. } => {
                self.err(
                    "`fan.*` scheduling is not admissible in a @bounded function",
                    "the profile's declared capability is standard output only",
                    "E076",
                    e.span,
                );
            }
            ast::ExprKind::Block { stmts, expr } => {
                self.walk_stmts(stmts);
                if let Some(x) = expr {
                    self.walk_expr(x);
                }
            }
            _ => self.walk_children(e),
        }
    }

    /// manual recursion for the container arms with no bounded-specific rule
    /// (ast::visit_expr recurses whole subtrees, which would double-count)
    fn walk_children(&mut self, e: &ast::Expr) {
        match &e.kind {
            ast::ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond);
                self.walk_expr(then);
                self.walk_expr(else_);
            }
            ast::ExprKind::Match { subject, arms } => {
                self.walk_expr(subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.walk_expr(g);
                    }
                    self.walk_expr(&arm.body);
                }
            }
            ast::ExprKind::Member { object, .. } => self.walk_expr(object),
            ast::ExprKind::IndexAccess { object, index } => {
                self.walk_expr(object);
                self.walk_expr(index);
            }
            ast::ExprKind::Try { expr }
            | ast::ExprKind::ToOption { expr }
            | ast::ExprKind::Paren { expr } => self.walk_expr(expr),
            ast::ExprKind::UnwrapOr { expr, fallback } => {
                self.walk_expr(expr);
                self.walk_expr(fallback);
            }
            ast::ExprKind::Range { start, end, .. } => {
                self.walk_expr(start);
                self.walk_expr(end);
            }
            ast::ExprKind::Tuple { elements } => {
                for el in elements {
                    self.walk_expr(el);
                }
            }
            ast::ExprKind::Record { fields, .. } => {
                for f in fields {
                    self.walk_expr(&f.value);
                }
            }
            _ => {}
        }
    }

    fn check_call(&mut self, call: &ast::Expr, callee: &ast::Expr, args: &[ast::Expr]) {
        match &callee.kind {
            ast::ExprKind::Ident { name } => {
                let n = name.as_str();
                if let Some((is_bounded, _)) = self.fns.get(n) {
                    if !is_bounded {
                        // ALS-B7 / E074
                        self.err(
                            "calling a function outside the profile is not admissible in a @bounded function",
                            "mark the callee @bounded, or use a first-order pure stdlib member (@bounded callee)",
                            "E074",
                            call.span,
                        );
                    }
                    return;
                }
                // bare builtins: the print family is B9's one capability
                if n.starts_with("print") || n.starts_with("eprint") {
                    if !self.is_effect {
                        // a pure fn cannot reach it anyway; E006 owns that
                    }
                    return;
                }
                if n == "assert" || n == "assert_eq" {
                    return; // test-position machinery, not a profile callee
                }
                // unknown bare callee: not a @bounded fn, not admissible
                self.err(
                    "calling a function outside the profile is not admissible in a @bounded function",
                    "mark the callee @bounded, or use a first-order pure stdlib member (@bounded callee)",
                    "E074",
                    call.span,
                );
            }
            ast::ExprKind::Member { object, field } => {
                let ast::ExprKind::Ident { name: module } = &object.kind else {
                    // a computed receiver is an indirect callee
                    self.err(
                        "an indirect call is not admissible in a @bounded function",
                        "call @bounded functions or first-order pure stdlib members directly (@bounded callee)",
                        "E074",
                        call.span,
                    );
                    return;
                };
                let m0 = module.as_str();
                let resolved = self
                    .checker
                    .env
                    .import_table
                    .resolve(m0)
                    .map(|s| s.as_str().to_string())
                    .unwrap_or_else(|| m0.to_string());
                let m = resolved.as_str();
                let f = field.as_str();
                if m == "io" {
                    if !(f.starts_with("print") || f.starts_with("eprint")) {
                        self.err(
                            "an effect outside the declared capability is not admissible in a @bounded function",
                            "the profile's declared capability is standard output only",
                            "E076",
                            call.span,
                        );
                    }
                    return;
                }
                if BOUNDED_DENIED_MODULES.contains(&m) {
                    // ALS-B9 / E076
                    self.err(
                        "an effect outside the declared capability is not admissible in a @bounded function",
                        "the profile's declared capability is standard output only",
                        "E076",
                        call.span,
                    );
                    return;
                }
                if !BOUNDED_PURE_MODULES.contains(&m) {
                    self.err(
                        "calling a function outside the profile is not admissible in a @bounded function",
                        "mark the callee @bounded, or use a first-order pure stdlib member (@bounded callee)",
                        "E074",
                        call.span,
                    );
                    return;
                }
                let full = format!("{m}.{f}");
                // ALS-B7 / E074: higher-order stdlib members take function
                // values — indirect callees the profile cannot follow
                let higher_order = crate::stdlib::lookup_sig(m, f)
                    .map(|sig| sig.params.iter().any(|(_, t)| matches!(t, Ty::Fn { .. })))
                    .unwrap_or(false)
                    || args
                        .iter()
                        .any(|a| matches!(a.kind, ast::ExprKind::Lambda { .. }));
                if higher_order {
                    self.err(
                        "a higher-order call is not admissible in a @bounded function",
                        "iterate with a counted loop and first-order calls (@bounded callee)",
                        "E074",
                        call.span,
                    );
                    return;
                }
                // ALS-B8 / E075: run-time-length heap construction
                if let Some((_, slots)) = BOUNDED_SIZED_CTORS.iter().find(|(n, _)| *n == full) {
                    for slot in slots.iter() {
                        if let Some(a) = args.get(*slot) {
                            if !self.is_const_int(a) {
                                self.err(
                                    "run-time-length heap construction is not admissible in a @bounded function",
                                    "give every constructed length a compile-time size",
                                    "E075",
                                    call.span,
                                );
                                return;
                            }
                        }
                    }
                }
                // ALS-B4 / E071: allocating calls inside a counted loop
                if self.loop_depth > 0 && BOUNDED_ALLOC_CALLS.contains(&full.as_str()) {
                    self.err(
                        "an allocating call inside a counted loop is not admissible in a @bounded function",
                        "allocate outside the loop and keep the loop body allocation-free",
                        "E071",
                        call.span,
                    );
                    return;
                }
                // ALS-B10 / E077: a stdlib call computing on Float arguments
                if args.iter().any(|a| self.expr_is_float(a)) && m != "float" {
                    // float.to_string / float.sign etc. OBSERVE a Float —
                    // computing modules (math.*) reject it
                    if m == "math" {
                        self.err(
                            "a Float operation is not admissible in a @bounded function",
                            "keep to Int arithmetic — Float values may be held and passed, not computed on",
                            "E077",
                            call.span,
                        );
                    }
                }
            }
            _ => {
                self.err(
                    "an indirect call is not admissible in a @bounded function",
                    "call @bounded functions or first-order pure stdlib members directly (@bounded callee)",
                    "E074",
                    call.span,
                );
            }
        }
    }
}
