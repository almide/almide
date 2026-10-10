// E008 (ADR-0020 §3, #2697): the checker side of the concurrent-var-reach
// rule. The analysis is `crate::concurrent_reach`; this file supplies the
// program's types and import table, and turns each finding into the §3.4
// diagnostic. It runs once per program after inference (the entry program and
// every module alike, through `validate_after_solve`).

impl Checker {
    pub(crate) fn check_concurrent_var_reach(&mut self, program: &ast::Program) {
        use crate::concurrent_reach::{Analyzer, World};
        let module = self.current_module_prefix.as_deref().map(sym);
        let (findings, closure) = {
            let env = &self.env;
            let aliases = &env.import_table.aliases;
            let direct = &env.import_table.direct;
            // No site: `run` records nothing — said without building the
            // analyzer's tables (#3509).
            if !crate::concurrent_reach::may_have_sites(program, aliases, direct, &env.concurrent_summaries) {
                return;
            }
            let type_map = &self.type_map;
            let arg_is_fn = |e: &ast::Expr| -> Option<bool> {
                type_map.get(&e.id).map(|t| crate::concurrent_reach_types::ty_is_fn_valued(env, t))
            };
            let type_is_fn = |te: &ast::TypeExpr| crate::concurrent_reach_types::type_expr_is_fn_valued(env, te);
            let w = World {
                module,
                aliases,
                direct,
                ext: &env.concurrent_summaries,
                arg_is_fn: &arg_is_fn,
                type_is_fn: &type_is_fn,
            };
            Analyzer::new(program, w).run(program)
        };
        for f in findings {
            let d = concurrent_reach_diagnostic(&f, self.source_file.clone());
            self.diagnostics.push(d);
        }
        for f in closure {
            let d = served_app_closure_diagnostic(&f, self.source_file.clone());
            self.diagnostics.push(d);
        }
    }

    /// The interface facts of this program's fns (`almide compile`): each
    /// fn's concurrent parameters and whether its body reaches a `var`.
    pub fn concurrent_fn_facts(&self, program: &ast::Program) -> Vec<(String, Vec<String>, Option<String>)> {
        use crate::concurrent_reach::{Analyzer, World};
        let module = self.current_module_prefix.as_deref().map(sym);
        let aliases = self.env.import_table.aliases.clone();
        let direct = self.env.import_table.direct.clone();
        let env = &self.env;
        let type_map = &self.type_map;
        let arg_is_fn = |e: &ast::Expr| -> Option<bool> {
            type_map.get(&e.id).map(|t| crate::concurrent_reach_types::ty_is_fn_valued(env, t))
        };
        let type_is_fn = |te: &ast::TypeExpr| crate::concurrent_reach_types::type_expr_is_fn_valued(env, te);
        let w = World {
            module,
            aliases: &aliases,
            direct: &direct,
            ext: &env.concurrent_summaries,
            arg_is_fn: &arg_is_fn,
            type_is_fn: &type_is_fn,
        };
        let mut a = Analyzer::new(program, w);
        a.infer_slots(program);
        let summaries = a.summaries();
        let mut out = Vec::new();
        for d in &program.decls {
            let ast::Decl::Fn { name, params, .. } = d else { continue };
            let Some(s) = summaries.get(name) else { continue };
            let concurrent: Vec<String> = s
                .slots
                .iter()
                .filter_map(|(i, _)| params.get(*i).map(|p| p.name.to_string()))
                .collect();
            let reaches = s.reach.as_ref().map(|w| format!("{}{}", w.var, witness_path(w)));
            if !concurrent.is_empty() || reaches.is_some() {
                out.push((name.to_string(), concurrent, reaches));
            }
        }
        out
    }
}

/// The §3.4 text for one finding.
fn concurrent_reach_diagnostic(
    f: &crate::concurrent_reach::Finding,
    file: Option<String>,
) -> Diagnostic {
    use crate::concurrent_reach::SlotKind;
    let w = &f.witness;
    let var = w.var.as_str();
    let never_written = !w.maybe_assigned;
    let let_fix = format!("`{var}` is never assigned: declare it with `let` instead of `var`.");
    let through = witness_path(w);
    let (message, hint, context) = match &f.kind {
        SlotKind::FanBlock { surface } => (
            format!("cannot capture mutable variable '{var}' inside {surface} block"),
            "Use a `let` binding instead of `var` for values shared across fan expressions".to_string(),
            format!("{surface} block{through}"),
        ),
        SlotKind::FanCallback { surface } => {
            let what = match &f.wrapper {
                Some(w) if w != surface => format!("this argument to `{w}`"),
                _ => format!("this {surface} callback"),
            };
            let base = "return each element's contribution and combine the results after the fan, e.g. `fan.map(xs, f)! |> list.sum`, or bind a `let` copy of the value before the fan";
            (
                format!("{what} can reach `{var}`, a `var` — fan bodies run concurrently"),
                if never_written { format!("{let_fix} Otherwise {base}") } else { base.to_string() },
                format!("the callback passed to {}{through}", f.wrapper.as_deref().unwrap_or(surface)),
            )
        }
        SlotKind::Handler { surface } => {
            let base = "a handler cannot share mutable state between requests. For a value fixed before serving, use a top-level `let` instead of `var`, or bind a `let` copy before the handler";
            (
                format!("this handler can reach `{var}`, a `var` — handlers run concurrently"),
                if never_written { format!("{let_fix} Otherwise: {base}") } else { base.to_string() },
                format!("the handler passed to {}{through}", f.wrapper.as_deref().unwrap_or(surface)),
            )
        }
    };
    let mut d = Diagnostic::error(message, hint, context).with_code("E008");
    d.file = file;
    if let Some(s) = f.span {
        d.line = Some(s.line);
        d.col = Some(s.col);
        d.end_col = Some(s.end_col);
    }
    d
}

/// E095 (ADR-0020 §5.2): the app passed to `http.serve` reads a local.
fn served_app_closure_diagnostic(f: &crate::concurrent_reach::ClosureFinding, file: Option<String>) -> Diagnostic {
    let name = f.name.as_str();
    let (whose, theirs) = match f.owner.map(|o| o.to_string()) {
        Some(o) if o == "main" => ("main".to_string(), "main's locals".to_string()),
        Some(o) => (format!("fn `{o}`"), format!("the locals of `{o}`")),
        None => ("the enclosing block".to_string(), "its locals".to_string()),
    };
    let message = format!("the app passed to {} captures `{name}`, a local of {whose}", f.surface);
    let hint = format!(
        "the app runs in several instances — one per worker natively, one per request on a wasi:http host — \
         and {theirs} do not exist there. Make `{name}` a top-level `let`, or compute it inside the handler."
    );
    let context = match &f.wrapper {
        Some(w) => format!("the app passed to {w}, which serves it with {}", f.surface),
        None => format!("the app passed to {}", f.surface),
    };
    let mut d = Diagnostic::error(message, hint, context).with_code("E095");
    d.file = file;
    if let Some(s) = f.span {
        d.line = Some(s.line);
        d.col = Some(s.col);
        d.end_col = Some(s.end_col);
    }
    d
}

/// " (reached through fn `bump` at line 4)" — empty when the site names a var
/// of this program itself.
fn witness_path(w: &crate::concurrent_reach::Witness) -> String {
    if w.path.is_empty() {
        return match w.var_module {
            Some(m) => format!(" (`{}` is a `var` of module {m})", w.var),
            None => String::new(),
        };
    }
    let hops: Vec<String> = w
        .path
        .iter()
        .map(|h| match (h.module, h.line) {
            (Some(m), _) if !h.fn_name.contains('.') => format!("fn `{}` in module {m}", h.fn_name),
            (_, Some(l)) => format!("fn `{}` at line {l}", h.fn_name),
            _ => format!("fn `{}`", h.fn_name),
        })
        .collect();
    format!(" (reached through {})", hops.join(" → "))
}
