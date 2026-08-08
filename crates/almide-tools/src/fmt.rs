/// Almide code formatter: AST → formatted Almide source code.
///
/// Owns:     indentation, spacing, line breaks, comment preservation
/// Does NOT: parsing, type checking

use std::fmt::Write;
use almide_lang::ast::*;
use almide_base::intern::Sym;

// Escapes a raw string for emission inside a double-quoted literal. `Decl::Test`
// names are stored (post-parse) as the raw, already-unescaped description text —
// unlike `ExprKind::String` (see fmt_expr.rs), which fmt_expr escapes correctly,
// this had no escaping at all, so a test name containing a literal `"` (e.g.
// `test "decide_pick(\"big\") ..."`) round-tripped through fmt into a broken,
// prematurely-closed string literal — not idempotent, and not even valid on
// the first pass. Same rule set as fmt_expr's ExprKind::String double-quote arm.
pub(crate) fn escape_dquoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out
}

fn join_syms(syms: &[Sym], sep: &str) -> String {
    syms.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(sep)
}

/// Infallible write to String — suppresses unwrap() on write!/writeln!
macro_rules! w {
    ($dst:expr, $($arg:tt)*) => {{ let _ = write!($dst, $($arg)*); }};
}
macro_rules! wln {
    ($dst:expr, $($arg:tt)*) => {{ let _ = writeln!($dst, $($arg)*); }};
    ($dst:expr) => {{ let _ = writeln!($dst); }};
}

const INDENT: &str = "  ";

fn ind(depth: usize) -> String { INDENT.repeat(depth) }

fn is_short(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Int { .. } | ExprKind::Float { .. } | ExprKind::Bool { .. }
        | ExprKind::Unit | ExprKind::None | ExprKind::Hole | ExprKind::Placeholder
        | ExprKind::Ident { .. } | ExprKind::TypeName { .. } => true,
        ExprKind::String { value, .. } => value.len() < 40,
        ExprKind::Some { expr, .. } | ExprKind::Ok { expr, .. } | ExprKind::Err { expr, .. }
        | ExprKind::Paren { expr, .. } => is_short(expr),
        ExprKind::Tuple { elements, .. } => elements.len() <= 4 && elements.iter().all(is_short),
        ExprKind::Call { args, .. } => args.len() <= 2 && args.iter().all(is_short),
        ExprKind::IndexAccess { object, index, .. } => is_short(object) && is_short(index),
        ExprKind::Binary { left, right, .. } => is_short(left) && is_short(right),
        ExprKind::Unary { operand, .. } => is_short(operand),
        _ => false,
    }
}

fn comma_sep<T>(out: &mut String, items: &[T], f: impl Fn(&mut String, &T)) {
    for (i, item) in items.iter().enumerate() {
        if i > 0 { out.push_str(", "); }
        f(out, item);
    }
}

/// Auto-manage imports: add missing stdlib/dependency imports, remove unused ones.
/// `dep_names`: dependency names from almide.toml (empty if no project file).
/// `dep_submodules`: map of short_name → full dotted path for dependency submodules.
/// Returns messages describing changes made.
/// Token-level module-reference SUPERSET: every identifier immediately
/// followed by `.` in the source. Total by construction — unlike the AST
/// walk below, there is no traversal to grow holes in. Drives REMOVAL
/// decisions only: a false KEEP (a local var that shadows a module name)
/// is harmless, while a false REMOVE silently broke real programs twice
/// (type-position-only imports; a match-subject `json.parse` missed by a
/// wildcard arm). Additions keep using the precise AST walk.
fn token_module_refs(source: &str) -> std::collections::HashMap<String, std::collections::HashSet<String>> {
    use almide_lang::lexer::{Lexer, TokenType};
    // Trivia is dropped before windowing: a method chain may break the line
    // before its `.` (#1091), so `json\n  .parse(s)` must still register as a
    // reference to `json` — otherwise the REMOVE side would delete the very
    // import that call needs.
    let tokens: Vec<_> = Lexer::tokenize(source)
        .into_iter()
        .filter(|t| !matches!(t.token_type, TokenType::Newline | TokenType::Comment))
        .collect();
    let mut refs: std::collections::HashMap<String, std::collections::HashSet<String>> = Default::default();
    for w in tokens.windows(3) {
        if matches!(w[0].token_type, TokenType::Ident | TokenType::TypeName)
            && matches!(w[1].token_type, TokenType::Dot)
        {
            let fields = refs.entry(w[0].value.clone()).or_default();
            if matches!(w[2].token_type, TokenType::Ident | TokenType::TypeName) {
                fields.insert(w[2].value.clone());
            }
        }
    }
    refs
}

/// ADD-side precision gate: only auto-import a stdlib module when at least
/// one `name.field` usage names a function that module actually DEFINES —
/// a LOCAL variable that happens to share a stdlib module's name (`let path
/// = ...; path.starts_with(..)`) must not get a spurious `import path`
/// injected over it (which re-routes the call to the module and breaks the
/// build). Verified against the bundled stdlib source; modules without
/// bundled source stay on the old behavior.
fn stdlib_module_defines_any(module: &str, fields: Option<&std::collections::HashSet<String>>) -> bool {
    let Some(fields) = fields else { return false };
    match almide_lang::stdlib_info::bundled_source(module) {
        Some(src) => fields.iter().any(|f| src.contains(&format!("fn {}(", f))),
        None => true,
    }
}

/// The `(display_name, path_segments)` of an import `name` still needs — a
/// Tier-2 stdlib module that actually DEFINES one of the referenced items, a
/// declared dependency, or a dependency SUBMODULE (`python` →
/// `bindgen.bindings.python`). `None` means nothing to add for this name.
fn import_to_add(
    name: &str,
    token_refs: &std::collections::HashMap<String, std::collections::HashSet<String>>,
    dep_set: &std::collections::HashSet<&str>,
    dep_submodules: &std::collections::HashMap<String, String>,
) -> Option<(String, Vec<String>)> {
    if almide_lang::stdlib_info::is_any_stdlib(name) {
        return stdlib_module_defines_any(name, token_refs.get(name))
            .then(|| (name.to_string(), vec![name.to_string()]));
    }
    if dep_set.contains(name) {
        return Some((name.to_string(), vec![name.to_string()]));
    }
    let full_path = dep_submodules.get(name)?;
    Some((full_path.clone(), full_path.split('.').map(String::from).collect()))
}

/// Should this import declaration survive the unused-import sweep?
///
/// `_`-prefixed and `self` imports are always kept (they are deliberate), and
/// liveness consults the token-level SUPERSET as well as the AST walk: deleting
/// a live import destroys the program, so recall beats precision here.
fn import_is_live(
    d: &Decl,
    used: &std::collections::HashSet<String>,
    token_refs: &std::collections::HashMap<String, std::collections::HashSet<String>>,
) -> bool {
    let Decl::Import { path, alias, .. } = d else { return true };
    let name = alias
        .as_ref()
        .map(|a| a.to_string())
        .unwrap_or_else(|| path.last().map(|s| s.to_string()).unwrap_or_default());
    if name.starts_with('_') || path.first().map(|s| s.as_str()) == Some("self") {
        return true;
    }
    used.contains(&name) || token_refs.contains_key(&name)
}

/// #1129: `comment_map` is walked POSITIONALLY (module?, imports…, decls…), so
/// dropping an import must drop its comment slot too — otherwise every later
/// declaration reads its PREDECESSOR's comments and the labels silently attach
/// to the wrong fn (the one artifact the compiler cannot reconstruct, #1090's
/// principle). The dropped import's OWN leading comments belong to whatever now
/// occupies that position — prepend them, never discard.
fn carry_dropped_comment_slots(comment_map: &mut Vec<Vec<String>>, dropped_slots: Vec<usize>) {
    for slot in dropped_slots.into_iter().rev() {
        if slot >= comment_map.len() {
            continue;
        }
        let carried = comment_map.remove(slot);
        if carried.is_empty() {
            continue;
        }
        match comment_map.get_mut(slot) {
            Some(next) => {
                let mut merged = carried;
                merged.extend(std::mem::take(next));
                *next = merged;
            }
            None => comment_map.push(carried),
        }
    }
}

pub fn auto_imports(program: &mut Program, source: &str, dep_names: &[String], dep_submodules: &std::collections::HashMap<String, String>) -> Vec<String> {
    use std::collections::HashSet;
    let mut messages = Vec::new();

    // Collect existing import names (canonical paths)
    let existing: HashSet<String> = program.imports.iter()
        .filter_map(|d| match d {
            Decl::Import { path, .. } =>
                Some(path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".")),
            _ => None,
        })
        .collect();

    // Collect module identifiers used in member access patterns (module.func)
    let mut used = HashSet::new();
    for decl in &program.decls {
        collect_module_refs_decl(decl, &mut used);
    }
    let token_refs = token_module_refs(source);

    // Also check auto-imported stdlib (Tier 1) — these don't need explicit import
    let auto_imported: HashSet<&str> = almide_lang::stdlib_info::AUTO_IMPORT_BUNDLED.iter().copied().collect();
    // Tier 1 hardcoded stdlib modules that don't need import (matches types/env.rs)
    let tier1: HashSet<&str> = ["string", "list", "int", "float", "bytes", "matrix", "map", "set",
        "value", "option", "result"].iter().copied().collect();

    let dep_set: HashSet<&str> = dep_names.iter().map(|s| s.as_str()).collect();

    let mut to_add: Vec<(String, Vec<String>)> = used
        .iter()
        .filter(|name| {
            !existing.contains(name.as_str())
                && !auto_imported.contains(name.as_str())
                && !tier1.contains(name.as_str())
        })
        .filter_map(|name| {
            import_to_add(name, &token_refs, &dep_set, dep_submodules)
        })
        .collect();
    to_add.sort_by(|a, b| a.0.cmp(&b.0));
    for (display, segments) in to_add {
        let path: Vec<Sym> = segments.iter().map(|s| almide_base::intern::sym(s)).collect();
        program.imports.push(Decl::Import { path, names: None, alias: None, span: None });
        messages.push(format!("Added `import {}`", display));
    }

    // Remove unused imports (keep _ prefixed, self imports, and auto-imported).
    // Removal consults the token-level SUPERSET, not the AST walk: deleting a
    // live import destroys the program, so recall beats precision here.
    let before_len = program.imports.len();
    // #1129: `comment_map` is walked POSITIONALLY (module?, imports…, decls…),
    // so dropping an import must drop its comment slot too — otherwise every
    // later declaration reads its PREDECESSOR's comments and the labels
    // silently attach to the wrong fn (the one artifact the compiler cannot
    // reconstruct, #1090's principle).
    let module_slots = usize::from(matches!(program.decls.first(), Some(Decl::Module { .. })));
    let mut import_idx = 0usize;
    let mut dropped_slots: Vec<usize> = Vec::new();
    program.imports.retain(|d| {
        let slot = module_slots + import_idx;
        import_idx += 1;
        let keep = import_is_live(d, &used, &token_refs);
        if !keep {
            dropped_slots.push(slot);
        }
        keep
    });
    carry_dropped_comment_slots(&mut program.comment_map, dropped_slots);
    let removed = before_len - program.imports.len();
    if removed > 0 {
        messages.push(format!("Removed {} unused import(s)", removed));
    }

    messages
}

fn collect_module_refs_decl(decl: &Decl, used: &mut std::collections::HashSet<String>) {
    match decl {
        Decl::Fn { params, return_type, body, .. } => {
            for p in params { collect_module_refs_type(&p.ty, used); }
            collect_module_refs_type(return_type, used);
            if let Some(body) = body { collect_module_refs_expr(body, used); }
        }
        Decl::Test { body, .. } => collect_module_refs_expr(body, used),
        Decl::TopLet { ty, value, .. } => {
            if let Some(te) = ty { collect_module_refs_type(te, used); }
            collect_module_refs_expr(value, used);
        }
        Decl::Type { ty, .. } => collect_module_refs_type(ty, used),
        _ => {}
    }
}

/// Type-position module references (`varlib.Policy` in a signature, variant
/// payload, record field, alias target, or annotation) count as usages too —
/// without this walk, an import used ONLY in type position was deleted as
/// "unused", silently breaking the file.
fn collect_module_refs_type(te: &TypeExpr, used: &mut std::collections::HashSet<String>) {
    match te {
        TypeExpr::Simple { name } => insert_type_name_prefix(name.as_str(), used),
        TypeExpr::Generic { name, args } => {
            // `!` and `?` are the pseudo-generic effect/Option spellings, not
            // module-qualified names.
            if name.as_str() != "!" && name.as_str() != "?" {
                insert_type_name_prefix(name.as_str(), used);
            }
            collect_module_refs_types(args, used);
        }
        TypeExpr::Record { fields } | TypeExpr::OpenRecord { fields } => {
            collect_module_refs_field_types(fields, used)
        }
        TypeExpr::Fn { params, ret, is_effect: _ } => {
            collect_module_refs_types(params, used);
            collect_module_refs_type(ret, used);
        }
        TypeExpr::Tuple { elements } | TypeExpr::Union { members: elements } => {
            collect_module_refs_types(elements, used)
        }
        TypeExpr::Variant { cases } => {
            for c in cases {
                collect_module_refs_variant_case(c, used);
            }
        }
        TypeExpr::ConstLit { .. } => {}
    }
}

/// [`collect_module_refs_type`] over a sequence of types.
fn collect_module_refs_types(tes: &[TypeExpr], used: &mut std::collections::HashSet<String>) {
    for te in tes {
        collect_module_refs_type(te, used);
    }
}

/// [`collect_module_refs_type`] over a record's field types.
fn collect_module_refs_field_types(
    fields: &[FieldType],
    used: &mut std::collections::HashSet<String>,
) {
    for f in fields {
        collect_module_refs_type(&f.ty, used);
    }
}

fn insert_type_name_prefix(name: &str, used: &mut std::collections::HashSet<String>) {
    if let Some((prefix, _)) = name.rsplit_once('.') {
        used.insert(prefix.to_string());
        // Submodule path (`a.b.Type`): the import binds the LAST segment.
        if let Some((_, last)) = prefix.rsplit_once('.') {
            used.insert(last.to_string());
        }
    }
}

fn collect_module_refs_variant_case(c: &VariantCase, used: &mut std::collections::HashSet<String>) {
    match c {
        VariantCase::Unit { .. } => {}
        VariantCase::Tuple { fields, .. } => {
            for f in fields { collect_module_refs_type(f, used); }
        }
        VariantCase::Record { fields, .. } => {
            for f in fields { collect_module_refs_type(&f.ty, used); }
        }
    }
}

/// Collect the module names an expression references, for `auto_imports`.
///
/// Split into a CHILD-LIST arm set and a SCOPED arm set. The first group's arms
/// only name their sub-expressions, so they return `Vec<&Expr>` and the single
/// driver below does the recursion — the arms carry no bodies at all, which is
/// where the complexity was. The second group needs more than recursion (a
/// module name to record, statement bodies, match arms with guards) and keeps
/// its own helpers.
///
/// The `_ => {}` fallthrough is deliberate and pre-existing: an expression form
/// with no sub-expressions references no module. But note what a MISSING arm
/// costs here — a dropped import, i.e. code that no longer compiles after
/// `almide fmt`. Any new `ExprKind` with children must be added to one group.
fn collect_module_refs_expr(expr: &Expr, used: &mut std::collections::HashSet<String>) {
    if collect_module_refs_scoped(expr, used) {
        return;
    }
    for child in module_ref_children(expr) {
        collect_module_refs_expr(child, used);
    }
}

/// Every sub-expression of a form whose only contribution is its children.
fn module_ref_children(expr: &Expr) -> Vec<&Expr> {
    match &expr.kind {
        ExprKind::Call { callee, args, .. } => {
            let mut v: Vec<&Expr> = vec![callee];
            v.extend(args.iter());
            v
        }
        ExprKind::Binary { left, right, .. }
        | ExprKind::Pipe { left, right, .. } => vec![left, right],
        ExprKind::If { cond, then, else_, .. } => vec![cond, then, else_],
        ExprKind::List { elements, .. } | ExprKind::Tuple { elements, .. } => {
            elements.iter().collect()
        }
        ExprKind::Record { fields, .. } => fields.iter().map(|f| &f.value).collect(),
        ExprKind::IndexAccess { object, index, .. } => vec![object, index],
        ExprKind::Unary { operand, .. } => vec![operand],
        ExprKind::Unwrap { expr, .. }
        | ExprKind::Try { expr, .. }
        | ExprKind::ToOption { expr, .. } => vec![expr],
        ExprKind::UnwrapOr { expr, fallback, .. } => vec![expr, fallback],
        _ => Vec::new(),
    }
}

/// The forms that contribute more than their child expressions: a module name to
/// record, a statement body, or match arms with guards. Returns `true` when it
/// handled the expression (so the child-list driver must not also walk it).
fn collect_module_refs_scoped(expr: &Expr, used: &mut std::collections::HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::Member { .. } => collect_module_refs_member(expr, used),
        ExprKind::Match { .. } => collect_module_refs_match(expr, used),
        ExprKind::InterpolatedString { .. } => collect_module_refs_istring(expr, used),
        ExprKind::Lambda { body, .. } => collect_module_refs_expr(body, used),
        ExprKind::Block { stmts, .. } => {
            for s in stmts { collect_module_refs_stmt(s, used); }
        }
        ExprKind::ForIn { iterable, body, .. } => {
            collect_module_refs_expr(iterable, used);
            for s in body { collect_module_refs_stmt(s, used); }
        }
        ExprKind::While { cond, body, .. } => {
            collect_module_refs_expr(cond, used);
            for s in body { collect_module_refs_stmt(s, used); }
        }
        _ => return false,
    }
    true
}

fn collect_module_refs_member(expr: &Expr, used: &mut std::collections::HashSet<String>) {
    let ExprKind::Member { object, .. } = &expr.kind else { unreachable!() };
    if let ExprKind::Ident { name, .. } = &object.kind {
        used.insert(name.to_string());
    }
    collect_module_refs_expr(object, used);
}

fn collect_module_refs_match(expr: &Expr, used: &mut std::collections::HashSet<String>) {
    let ExprKind::Match { subject, arms, .. } = &expr.kind else { unreachable!() };
    collect_module_refs_expr(subject, used);
    for arm in arms {
        collect_module_refs_expr(&arm.body, used);
        if let Some(g) = &arm.guard { collect_module_refs_expr(g, used); }
    }
}

fn collect_module_refs_istring(expr: &Expr, used: &mut std::collections::HashSet<String>) {
    let ExprKind::InterpolatedString { parts, .. } = &expr.kind else { unreachable!() };
    for p in parts {
        if let StringPart::Expr { expr } = p { collect_module_refs_expr(expr, used); }
    }
}

fn collect_module_refs_stmt(stmt: &Stmt, used: &mut std::collections::HashSet<String>) {
    match stmt {
        Stmt::Let { ty, value, .. } | Stmt::Var { ty, value, .. } => {
            if let Some(te) = ty { collect_module_refs_type(te, used); }
            collect_module_refs_expr(&value, used);
        }
        Stmt::Assign { value, .. } => collect_module_refs_expr(value, used),
        Stmt::Expr { expr, .. } => collect_module_refs_expr(expr, used),
        Stmt::Guard { cond, else_, .. } => {
            collect_module_refs_expr(cond, used);
            collect_module_refs_expr(else_, used);
        }
        Stmt::GuardLet { scrutinee, else_, .. } => {
            collect_module_refs_expr(scrutinee, used);
            collect_module_refs_expr(else_, used);
        }
        _ => {}
    }
}

pub fn format_program(program: &Program) -> String {
    let mut out = String::new();
    let cm = &program.comment_map;
    let mut ci = 0;
    let emit_comments = |out: &mut String, idx: &mut usize| {
        if let Some(comments) = cm.get(*idx) {
            for c in comments { wln!(out, "{c}"); }
        }
        *idx += 1;
    };
    for imp in &program.imports {
        if !out.is_empty() && ci == 0 { out.push('\n'); }
        emit_comments(&mut out, &mut ci);
        fmt_decl(&mut out, imp, 0);
        out.push('\n');
    }
    for decl in &program.decls {
        // The blank line SEPARATES declarations — so it belongs before every one except the
        // first thing in the file. Emitting it unconditionally opened every import-less file
        // with a blank line (#919: the leading-blank diff on every such spec fixture), which
        // is also not idempotent-looking output for a file that starts with a doc comment.
        if !out.is_empty() {
            out.push('\n');
        }
        emit_comments(&mut out, &mut ci);
        fmt_decl(&mut out, decl, 0);
        out.push('\n');
    }
    if let Some(comments) = cm.get(ci) {
        if !comments.is_empty() {
            out.push('\n');
            for c in comments { wln!(out, "{c}"); }
        }
    }
    out
}

/// Render a generic `@name(args)` attribute back to source. Mirrors
/// the parser's accepted shapes: bare `@name`, positional args, and
/// `name=value` named args. String values are re-quoted with `"`;
/// the parser never records the raw source quote style.
fn format_attribute(attr: &Attribute) -> String {
    let mut out = String::new();
    out.push('@');
    out.push_str(&attr.name);
    if attr.args.is_empty() {
        return out;
    }
    out.push('(');
    for (i, arg) in attr.args.iter().enumerate() {
        if i > 0 { out.push_str(", "); }
        if let Some(n) = &arg.name {
            out.push_str(n);
            out.push('=');
        }
        push_attr_value(&mut out, &arg.value);
    }
    out.push(')');
    out
}

/// One attribute argument value. Extracted from `format_attribute` — the string
/// case's escape table nested inside the argument loop was the whole cost.
fn push_attr_value(out: &mut String, value: &AttrValue) {
    match value {
        AttrValue::String { value } => {
            out.push('"');
            for ch in value.chars() {
                match ch {
                    '\\' => out.push_str("\\\\"),
                    '"' => out.push_str("\\\""),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        AttrValue::Int { value } => out.push_str(&value.to_string()),
        AttrValue::Bool { value } => out.push_str(if *value { "true" } else { "false" }),
        AttrValue::Ident { name } => out.push_str(name),
    }
}

fn fmt_vis(out: &mut String, vis: &Visibility) {
    match vis {
        Visibility::Local => out.push_str("local "),
        Visibility::Mod => out.push_str("mod "),
        Visibility::Public => {}
    }
}

fn fmt_generics(out: &mut String, params: &[GenericParam]) {
    out.push('[');
    comma_sep(out, params, |out, gp| {
        out.push_str(&gp.name);
        if let Some(ref sb) = gp.structural_bound {
            out.push_str(": "); fmt_type(out, sb, 0);
        } else if let Some(ref bounds) = gp.bounds {
            if !bounds.is_empty() { w!(out, ": {}", join_syms(bounds, " + ")); }
        }
    });
    out.push(']');
}

fn maybe_generics(out: &mut String, generics: &Option<Vec<GenericParam>>) {
    if let Some(gps) = generics { if !gps.is_empty() { fmt_generics(out, gps); } }
}

/// A `self` param typed `Self` is the parser's sugar for bare `self` — the
/// parser can never actually produce a *written-out* `self: Self` (that's a
/// syntax error, `self` always consumes the whole param). Printing `p.name: p.ty`
/// unconditionally here would emit exactly that unparseable text and break
/// `fmt`'s idempotence the moment a source file uses the sugar.
fn is_bare_self_param(p: &Param) -> bool {
    p.name.as_str() == "self" && matches!(&p.ty, TypeExpr::Simple { name } if name.as_str() == "Self")
}

/// `[vis] type Name[generics][: deriving] = Ty`.
fn fmt_decl_type(out: &mut String, decl: &Decl, depth: usize) {
    let Decl::Type { name, ty, deriving, visibility, generics, .. } = decl else { unreachable!() };
    out.push_str(&ind(depth));
    fmt_vis(out, visibility);
    w!(out, "type {name}");
    maybe_generics(out, generics);
    if let Some(d) = deriving {
        if !d.is_empty() {
            w!(out, ": {}", join_syms(d, ", "));
        }
    }
    out.push_str(" = ");
    fmt_type(out, ty, depth);
}

/// `[vis] let|var name[: Ty] = expr`.
fn fmt_decl_top_let(out: &mut String, decl: &Decl, depth: usize) {
    let Decl::TopLet { name, ty, value, visibility, mutable, .. } = decl else { unreachable!() };
    out.push_str(&ind(depth));
    fmt_vis(out, visibility);
    w!(out, "{} {name}", if *mutable { "var" } else { "let" });
    if let Some(te) = ty {
        out.push_str(": ");
        fmt_type(out, te, depth);
    }
    out.push_str(" = ");
    fmt_expr(out, value, depth);
}

fn fmt_decl(out: &mut String, decl: &Decl, depth: usize) {
    let i = ind(depth);
    match decl {
        Decl::Module { path, .. } => { w!(out, "{i}module {}", join_syms(path, ".")); }
        Decl::Import { path, names, alias, .. } => {
            w!(out, "{i}import {}", join_syms(path, "."));
            if let Some(n) = names { w!(out, ".{{{}}}", join_syms(n, ", ")); }
            if let Some(a) = alias { w!(out, " as {a}"); }
        }
        Decl::Strict { mode, .. } => w!(out, "{i}strict \"{mode}\""),
        Decl::Type { .. } => fmt_decl_type(out, decl, depth),
        Decl::TopLet { .. } => fmt_decl_top_let(out, decl, depth),
        Decl::Fn { .. } => fmt_decl_fn(out, decl, depth),
        Decl::Test { .. } => fmt_decl_test(out, decl, depth),
        Decl::TestWhereDef { .. } => {} // test where defs don't need formatting (internal)
        Decl::Protocol { .. } => fmt_decl_protocol(out, decl, depth),
    }
}

fn fmt_decl_fn(out: &mut String, decl: &Decl, depth: usize) {
    let Decl::Fn { name, effect, visibility, params, return_type, body, extern_attrs, export_attrs, attrs, generics, .. } = decl else { unreachable!() };
    let i = ind(depth);
    for a in extern_attrs { wln!(out, "{i}@extern({}, \"{}\", \"{}\")", a.target, escape_dquoted(a.module.as_str()), escape_dquoted(a.function.as_str())); }
    for a in export_attrs { wln!(out, "{i}@export({}, \"{}\")", a.target, escape_dquoted(a.symbol.as_str())); }
    for a in attrs { wln!(out, "{i}{}", format_attribute(a)); }
    out.push_str(&i); fmt_vis(out, visibility);
    if matches!(effect, Some(true)) { out.push_str("effect "); }
    w!(out, "fn {name}");
    maybe_generics(out, generics);
    out.push('(');
    comma_sep(out, params, |out, p| {
        // `mut` is semantic (mutable-borrow param) — dropping it
        // turned every in-place mutator call into E032.
        if p.is_mut { out.push_str("mut "); }
        if is_bare_self_param(p) {
            out.push_str("self");
        } else {
            w!(out, "{}: ", p.name); fmt_type(out, &p.ty, depth);
        }
        if let Some(ref d) = p.default { out.push_str(" = "); fmt_expr(out, d, depth); }
    });
    out.push_str(") -> "); fmt_type(out, return_type, depth);
    if let Some(b) = body { out.push_str(" = "); fmt_expr(out, b, depth); }
}

fn fmt_decl_test(out: &mut String, decl: &Decl, depth: usize) {
    let Decl::Test { name, body, where_clauses, .. } = decl else { unreachable!() };
    let i = ind(depth);
    // `where` clauses are the test's data — dropping them deleted
    // the bindings the body reads (E003 after formatting).
    w!(out, "{i}test \"{}\"", escape_dquoted(name));
    let cases: Vec<&TestWhere> = where_clauses.iter()
        .filter(|wc| matches!(wc, TestWhere::Case { .. })).collect();
    let binds: Vec<&TestWhere> = where_clauses.iter()
        .filter(|wc| !matches!(wc, TestWhere::Case { .. })).collect();
    for wc in &binds {
        out.push('\n');
        w!(out, "{i}  ");
        fmt_test_where(out, wc, depth);
    }
    if !cases.is_empty() {
        out.push_str(" where [\n");
        for c in &cases {
            w!(out, "{i}  ");
            fmt_test_where(out, c, depth);
            out.push_str(",\n");
        }
        w!(out, "{i}]");
    }
    if !binds.is_empty() { out.push('\n'); w!(out, "{i}"); } else { out.push(' '); }
    fmt_expr(out, body, depth);
}

fn fmt_decl_protocol(out: &mut String, decl: &Decl, depth: usize) {
    let Decl::Protocol { name, methods, .. } = decl else { unreachable!() };
    let i = ind(depth);
    wln!(out, "{i}protocol {name} {{");
    let inner = "  ".repeat(depth + 1);
    for m in methods {
        let effect = if m.effect { "effect " } else { "" };
        let mut params_str = String::new();
        for (j, p) in m.params.iter().enumerate() {
            if j > 0 { params_str.push_str(", "); }
            if is_bare_self_param(p) {
                params_str.push_str("self");
            } else {
                params_str.push_str(&p.name);
                params_str.push_str(": ");
                fmt_type(&mut params_str, &p.ty, 0);
            }
        }
        let mut ret_str = String::new();
        fmt_type(&mut ret_str, &m.return_type, 0);
        wln!(out, "{inner}{effect}fn {name}({params_str}) -> {ret_str}", name = m.name);
    }
    w!(out, "{i}}}");
}

/// ADR-0010 D3: `T?` is the canonical Option spelling — the pseudo-generic `?`
/// prints back as written, and a written `Option[T]` NORMALIZES to the same
/// shorthand. The inner takes parens whenever the bare rendering would not
/// re-parse under the atom-binding rule: fn types (`((A) -> B)?`), nested Option
/// (`(Int?)?` — `T??` would lex as the `??` operator), records/variants. A tuple
/// already renders parenthesized.
fn fmt_option_shorthand(out: &mut String, inner: &TypeExpr, depth: usize) {
    let bare_atom = match inner {
        TypeExpr::Simple { .. } | TypeExpr::Tuple { .. } => true,
        TypeExpr::Generic { name, args } => {
            !((name.as_str() == "?" || name.as_str() == "Option") && args.len() == 1)
                && name.as_str() != "!"
        }
        _ => false,
    };
    if bare_atom {
        fmt_type(out, inner, depth);
    } else {
        out.push('(');
        fmt_type(out, inner, depth);
        out.push(')');
    }
    out.push('?');
}

/// A record (or `..`-open record) type.
///
/// A field comment has nowhere to go on one line, so a record that carries one
/// is emitted multi-line. Records without comments keep the single-line shape,
/// so existing sources do not churn (#1090).
fn fmt_record_type(out: &mut String, fields: &[FieldType], open: bool, depth: usize) {
    if fields.iter().any(|f| !f.comments.is_empty()) {
        fmt_record_type_multiline(out, fields, open, depth);
        return;
    }
    out.push_str("{ ");
    comma_sep(out, fields, |out, f| fmt_field_type(out, f, depth));
    match (open, fields.is_empty()) {
        (true, false) => out.push_str(", .. "),
        (true, true) => out.push_str(".. "),
        (false, _) => out.push(' '),
    }
    out.push('}');
}

/// A union type's members, `A | B | C`.
fn fmt_union_members(out: &mut String, members: &[TypeExpr], depth: usize) {
    for (i, m) in members.iter().enumerate() {
        if i > 0 {
            out.push_str(" | ");
        }
        fmt_type(out, m, depth);
    }
}

/// A variant type's cases, with a LEADING `|` on the first case too — the
/// declaration style `type T =\n  | A\n  | B` round-trips only if it is emitted.
fn fmt_variant_cases(out: &mut String, cases: &[VariantCase], depth: usize) {
    for (i, case) in cases.iter().enumerate() {
        out.push_str(if i > 0 { " | " } else { "| " });
        fmt_variant_case(out, case, depth);
    }
}

fn fmt_type(out: &mut String, ty: &TypeExpr, depth: usize) {
    match ty {
        TypeExpr::Simple { name } => out.push_str(name),
        // ADR-0002 Phase 1: the pseudo-generic `!` prints back as the surface
        // spelling `T!` (never `![T]`) — the fmt roundtrip for `-> Int!`.
        TypeExpr::Generic { name, args } if name.as_str() == "!" && args.len() == 1 => {
            fmt_type(out, &args[0], depth);
            out.push('!');
        }
        // ADR-0010 D3: `T?` is the canonical Option spelling — the pseudo-
        // generic `?` prints back as written, and a written `Option[T]`
        // NORMALIZES to the same shorthand. The inner takes parens whenever
        // the bare rendering would not re-parse under the atom-binding rule:
        // fn types (`((A) -> B)?`), nested Option (`(Int?)?` — `T??` would
        // lex as the `??` operator), records/variants. A tuple already
        // renders parenthesized.
        TypeExpr::Generic { name, args }
            if (name.as_str() == "?" || name.as_str() == "Option") && args.len() == 1 =>
        {
            fmt_option_shorthand(out, &args[0], depth)
        }
        TypeExpr::Generic { name, args } => {
            out.push_str(name); out.push('[');
            comma_sep(out, args, |out, a| fmt_type(out, a, depth));
            out.push(']');
        }
        TypeExpr::Record { fields } => fmt_record_type(out, fields, false, depth),
        TypeExpr::OpenRecord { fields } => fmt_record_type(out, fields, true, depth),
        TypeExpr::Fn { params, ret, is_effect } => {
            if *is_effect { out.push_str("effect "); }
            out.push_str("fn(");
            comma_sep(out, params, |out, p| fmt_type(out, p, depth));
            out.push_str(") -> "); fmt_type(out, ret, depth);
        }
        TypeExpr::Tuple { elements } => {
            out.push('('); comma_sep(out, elements, |out, e| fmt_type(out, e, depth)); out.push(')');
        }
        TypeExpr::Union { members } => fmt_union_members(out, members, depth),
        TypeExpr::ConstLit { value } => {
            out.push_str(&value.to_string());
        }
        TypeExpr::Variant { cases } => fmt_variant_cases(out, cases, depth),
    }
}

/// One variant case: `Unit`, `Tuple(Ty, …)`, or `Record { f: Ty, … }`.
/// Extracted from `fmt_type` — the case shapes are a peer set, and nesting them
/// inside the enumerate loop is what pushed that function over the threshold.
fn fmt_variant_case(out: &mut String, case: &VariantCase, depth: usize) {
    match case {
        VariantCase::Unit { name } => out.push_str(name),
        VariantCase::Tuple { name, fields } => {
            out.push_str(name);
            out.push('(');
            comma_sep(out, fields, |out, f| fmt_type(out, f, depth));
            out.push(')');
        }
        VariantCase::Record { name, fields } => {
            w!(out, "{name} {{ ");
            comma_sep(out, fields, |out, f| fmt_field_type(out, f, depth));
            out.push_str(" }");
        }
    }
}


/// A record type whose fields carry comments: one field per line, so each
/// comment can sit above the field it documents.
fn fmt_record_type_multiline(out: &mut String, fields: &[FieldType], open: bool, depth: usize) {
    let inner = ind(depth + 1);
    out.push_str("{\n");
    for f in fields {
        for c in &f.comments {
            wln!(out, "{inner}{c}");
        }
        w!(out, "{inner}");
        fmt_field_type(out, f, depth + 1);
        out.push_str(",\n");
    }
    if open {
        wln!(out, "{inner}..");
    }
    w!(out, "{}}}", ind(depth));
}

/// One record-field declaration: `[@attr…] name [as "alias"]: Ty [= default]`.
/// The formatter must NOT drop the default, the serialization alias, or the
/// attributes — all three are semantic (defaults make fields omissible,
/// aliases name the wire key, attributes drive layout), and silently deleting
/// them broke round-tripped sources.
fn fmt_field_type(out: &mut String, f: &FieldType, depth: usize) {
    for a in &f.attrs {
        w!(out, "{} ", format_attribute(a));
    }
    w!(out, "{}", f.name);
    if let Some(alias) = &f.alias { w!(out, " as \"{}\"", escape_dquoted(alias.as_str())); }
    out.push_str(": ");
    fmt_type(out, &f.ty, depth);
    if let Some(d) = &f.default { out.push_str(" = "); fmt_expr(out, d, depth); }
}

include!("fmt_expr.rs");
include!("fmt_tests.rs");
