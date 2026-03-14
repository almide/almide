/// RustIR → Rust source code renderer.
///
/// Pure, stateless string generation. No codegen decisions — those are
/// all made during IR → RustIR lowering.

use super::rust_ir::*;

const INDENT: &str = "    ";

/// Render a type to a String (utility for lower_rust interop).
pub fn render_type_to_string(ty: &RustType) -> String {
    let mut out = String::new();
    render_type(&mut out, ty);
    out
}

/// Render a single expression to a String (utility for lower_rust interop).
pub fn render_expr_to_string(expr: &RustExpr) -> String {
    let mut out = String::new();
    render_expr(&mut out, expr, 0);
    out
}

// ── Program ──────────────────────────────────────────────────────

pub fn render_program(prog: &RustProgram) -> String {
    let mut out = String::new();

    // Prelude
    for line in &prog.prelude {
        out.push_str(line);
        out.push('\n');
    }
    if !prog.prelude.is_empty() { out.push('\n'); }

    // Macros
    for m in &prog.macros {
        out.push_str(m);
        out.push('\n');
    }
    if !prog.macros.is_empty() { out.push('\n'); }

    // Runtime
    if !prog.runtime.is_empty() {
        out.push_str(&prog.runtime);
        out.push('\n');
    }

    // Type declarations
    for s in &prog.structs {
        render_struct(&mut out, s, 0);
        out.push('\n');
    }
    for e in &prog.enums {
        render_enum(&mut out, e, 0);
        out.push('\n');
    }
    for a in &prog.type_aliases {
        render_type_alias(&mut out, a, 0);
        out.push('\n');
    }

    // Constants
    for c in &prog.consts {
        render_const(&mut out, c, 0);
        out.push('\n');
    }

    // Impls
    for imp in &prog.impls {
        render_impl(&mut out, imp, 0);
        out.push('\n');
    }

    // Functions
    for f in &prog.functions {
        render_function(&mut out, f, 0);
        out.push('\n');
    }

    // Test functions
    for f in &prog.test_functions {
        render_function(&mut out, f, 0);
        out.push('\n');
    }

    // Main wrapper
    if let Some(m) = &prog.main_wrapper {
        render_function(&mut out, m, 0);
        out.push('\n');
    }

    out
}

// ── Function ─────────────────────────────────────────────────────

fn render_function(out: &mut String, f: &RustFunction, depth: usize) {
    let ind = indent(depth);

    // Attributes
    for attr in &f.attrs {
        out.push_str(&ind);
        out.push_str(attr);
        out.push('\n');
    }

    out.push_str(&ind);
    if f.is_pub { out.push_str("pub "); }
    if f.is_async { out.push_str("async "); }
    out.push_str("fn ");
    out.push_str(&f.name);

    // Generics
    if !f.generics.is_empty() {
        out.push('<');
        out.push_str(&f.generics.join(", "));
        out.push('>');
    }

    // Params
    out.push('(');
    for (i, p) in f.params.iter().enumerate() {
        if i > 0 { out.push_str(", "); }
        if p.mutable { out.push_str("mut "); }
        out.push_str(&p.name);
        out.push_str(": ");
        render_type(out, &p.ty);
    }
    out.push_str(") -> ");
    render_type(out, &f.ret_ty);
    out.push_str(" {\n");

    // Body
    for stmt in &f.body {
        render_stmt(out, stmt, depth + 1);
    }
    if let Some(expr) = &f.tail_expr {
        out.push_str(&indent(depth + 1));
        render_expr(out, expr, depth + 1);
        out.push('\n');
    }

    out.push_str(&ind);
    out.push_str("}\n");
}

// ── Struct / Enum ────────────────────────────────────────────────

fn render_struct(out: &mut String, s: &RustStruct, depth: usize) {
    let ind = indent(depth);
    if !s.derives.is_empty() {
        out.push_str(&ind);
        out.push_str(&format!("#[derive({})]\n", s.derives.join(", ")));
    }
    out.push_str(&ind);
    if s.is_pub { out.push_str("pub "); }
    out.push_str("struct ");
    out.push_str(&s.name);
    if !s.generics.is_empty() {
        out.push_str(&format!("<{}>", s.generics.join(", ")));
    }
    out.push_str(" {\n");
    for (name, ty) in &s.fields {
        out.push_str(&indent(depth + 1));
        out.push_str("pub ");
        out.push_str(name);
        out.push_str(": ");
        render_type(out, ty);
        out.push_str(",\n");
    }
    out.push_str(&ind);
    out.push_str("}\n");
}

fn render_enum(out: &mut String, e: &RustEnum, depth: usize) {
    let ind = indent(depth);
    if !e.derives.is_empty() {
        out.push_str(&ind);
        out.push_str(&format!("#[derive({})]\n", e.derives.join(", ")));
    }
    out.push_str(&ind);
    if e.is_pub { out.push_str("pub "); }
    out.push_str("enum ");
    out.push_str(&e.name);
    if !e.generics.is_empty() {
        out.push_str(&format!("<{}>", e.generics.join(", ")));
    }
    out.push_str(" {\n");
    for v in &e.variants {
        out.push_str(&indent(depth + 1));
        out.push_str(&v.name);
        match &v.kind {
            RustVariantKind::Unit => {}
            RustVariantKind::Tuple(tys) => {
                out.push('(');
                for (i, ty) in tys.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    render_type(out, ty);
                }
                out.push(')');
            }
            RustVariantKind::Struct(fields) => {
                out.push_str(" { ");
                for (i, (name, ty)) in fields.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    out.push_str(name);
                    out.push_str(": ");
                    render_type(out, ty);
                }
                out.push_str(" }");
            }
        }
        out.push_str(",\n");
    }
    out.push_str(&ind);
    out.push_str("}\n");
}

fn render_type_alias(out: &mut String, a: &RustTypeAlias, depth: usize) {
    let ind = indent(depth);
    out.push_str(&ind);
    if a.is_pub { out.push_str("pub "); }
    out.push_str("type ");
    out.push_str(&a.name);
    out.push_str(" = ");
    render_type(out, &a.ty);
    out.push_str(";\n");
}

fn render_const(out: &mut String, c: &RustConst, depth: usize) {
    let ind = indent(depth);
    out.push_str(&ind);
    if c.is_pub { out.push_str("pub "); }
    out.push_str("const ");
    out.push_str(&c.name);
    out.push_str(": ");
    render_type(out, &c.ty);
    out.push_str(" = ");
    render_expr(out, &c.value, depth);
    out.push_str(";\n");
}

fn render_impl(out: &mut String, imp: &RustImpl, depth: usize) {
    let ind = indent(depth);
    out.push_str(&ind);
    if let Some(t) = &imp.trait_name {
        out.push_str(&format!("impl {} for {} {{\n", t, imp.type_name));
    } else {
        out.push_str(&format!("impl {} {{\n", imp.type_name));
    }
    for m in &imp.methods {
        render_function(out, m, depth + 1);
    }
    out.push_str(&ind);
    out.push_str("}\n");
}

// ── Expressions ──────────────────────────────────────────────────

fn render_expr(out: &mut String, expr: &RustExpr, depth: usize) {
    match expr {
        RustExpr::IntLit(v) => out.push_str(&format!("{}i64", v)),
        RustExpr::FloatLit(v) => out.push_str(&format!("{:?}f64", v)),
        RustExpr::StringLit(s) => out.push_str(&format!("{:?}.to_string()", s)),
        RustExpr::BoolLit(v) => out.push_str(if *v { "true" } else { "false" }),
        RustExpr::Unit => out.push_str("()"),

        RustExpr::Var(name) => out.push_str(name),

        RustExpr::BinOp { op, left, right } => {
            out.push('(');
            render_expr(out, left, depth);
            out.push_str(match op {
                RustBinOp::Add => " + ", RustBinOp::Sub => " - ",
                RustBinOp::Mul => " * ", RustBinOp::Div => " / ",
                RustBinOp::Mod => " % ",
                RustBinOp::Eq => " == ", RustBinOp::Neq => " != ",
                RustBinOp::Lt => " < ", RustBinOp::Gt => " > ",
                RustBinOp::Lte => " <= ", RustBinOp::Gte => " >= ",
                RustBinOp::And => " && ", RustBinOp::Or => " || ",
                RustBinOp::BitXor => " ^ ",
            });
            render_expr(out, right, depth);
            out.push(')');
        }
        RustExpr::UnOp { op, operand } => {
            out.push_str(match op {
                RustUnOp::Neg => "-",
                RustUnOp::Not => "!",
            });
            render_expr(out, operand, depth);
        }

        RustExpr::Call { func, args } => {
            out.push_str(func);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                render_expr(out, a, depth);
            }
            out.push(')');
        }
        RustExpr::MethodCall { receiver, method, args } => {
            render_expr(out, receiver, depth);
            out.push('.');
            out.push_str(method);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                render_expr(out, a, depth);
            }
            out.push(')');
        }
        RustExpr::MacroCall { name, args } => {
            out.push_str(name);
            out.push_str("!(");
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                render_expr(out, a, depth);
            }
            out.push(')');
        }

        RustExpr::If { cond, then, else_ } => {
            out.push_str("if ");
            render_expr(out, cond, depth);
            out.push_str(" { ");
            render_expr(out, then, depth);
            out.push_str(" }");
            if let Some(e) = else_ {
                out.push_str(" else { ");
                render_expr(out, e, depth);
                out.push_str(" }");
            }
        }
        RustExpr::Match { subject, arms } => {
            out.push_str("match ");
            render_expr(out, subject, depth);
            out.push_str(" {\n");
            for arm in arms {
                out.push_str(&indent(depth + 1));
                render_pattern(out, &arm.pattern);
                if let Some(g) = &arm.guard {
                    out.push_str(" if ");
                    render_expr(out, g, depth + 1);
                }
                out.push_str(" => ");
                render_expr(out, &arm.body, depth + 1);
                out.push_str(",\n");
            }
            out.push_str(&indent(depth));
            out.push('}');
        }
        RustExpr::Block { stmts, expr } => {
            out.push_str("{\n");
            for s in stmts {
                render_stmt(out, s, depth + 1);
            }
            if let Some(e) = expr {
                out.push_str(&indent(depth + 1));
                render_expr(out, e, depth + 1);
                out.push('\n');
            }
            out.push_str(&indent(depth));
            out.push('}');
        }
        RustExpr::For { var, iter, body } => {
            out.push_str("for ");
            out.push_str(var);
            out.push_str(" in ");
            render_expr(out, iter, depth);
            out.push_str(" {\n");
            for s in body {
                render_stmt(out, s, depth + 1);
            }
            out.push_str(&indent(depth));
            out.push('}');
        }
        RustExpr::While { cond, body } => {
            out.push_str("while ");
            render_expr(out, cond, depth);
            out.push_str(" {\n");
            for s in body {
                render_stmt(out, s, depth + 1);
            }
            out.push_str(&indent(depth));
            out.push('}');
        }
        RustExpr::Loop { label, body } => {
            if let Some(l) = label {
                out.push_str(&format!("'{}: loop {{\n", l));
            } else {
                out.push_str("loop {\n");
            }
            for s in body {
                render_stmt(out, s, depth + 1);
            }
            out.push_str(&indent(depth));
            out.push('}');
        }
        RustExpr::Break => out.push_str("break"),
        RustExpr::Continue { label } => {
            out.push_str("continue");
            if let Some(l) = label {
                out.push_str(&format!(" '{}", l));
            }
        }
        RustExpr::Return(val) => {
            out.push_str("return");
            if let Some(v) = val {
                out.push(' ');
                render_expr(out, v, depth);
            }
        }

        RustExpr::Clone(e) => { render_expr(out, e, depth); out.push_str(".clone()"); }
        RustExpr::ToOwned(e) => { render_expr(out, e, depth); out.push_str(".to_owned()"); }
        RustExpr::Borrow(e) => { out.push('&'); render_expr(out, e, depth); }
        RustExpr::Deref(e) => { out.push('*'); render_expr(out, e, depth); }
        RustExpr::TryOp(e) => { render_expr(out, e, depth); out.push('?'); }
        RustExpr::ResultOk(e) => { out.push_str("Ok("); render_expr(out, e, depth); out.push(')'); }
        RustExpr::ResultErr(e) => { out.push_str("Err("); render_expr(out, e, depth); out.push(')'); }
        RustExpr::OptionSome(e) => { out.push_str("Some("); render_expr(out, e, depth); out.push(')'); }
        RustExpr::OptionNone => out.push_str("None"),

        RustExpr::Vec(elems) => {
            if elems.is_empty() {
                out.push_str("vec![]");
            } else {
                out.push_str("vec![");
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    render_expr(out, e, depth);
                }
                out.push(']');
            }
        }
        RustExpr::HashMap(entries) => {
            out.push_str("HashMap::from([");
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push('(');
                render_expr(out, k, depth);
                out.push_str(", ");
                render_expr(out, v, depth);
                out.push(')');
            }
            out.push_str("])");
        }
        RustExpr::Tuple(elems) => {
            out.push('(');
            for (i, e) in elems.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                render_expr(out, e, depth);
            }
            out.push(')');
        }
        RustExpr::Range { start, end, inclusive, elem_ty } => {
            out.push('(');
            render_expr(out, start, depth);
            out.push_str(if *inclusive { "..=" } else { ".." });
            render_expr(out, end, depth);
            out.push_str(").collect::<Vec<");
            render_type(out, elem_ty);
            out.push_str(">>()");
        }

        RustExpr::Field(e, f) => { render_expr(out, e, depth); out.push('.'); out.push_str(f); }
        RustExpr::Index(e, idx) => {
            render_expr(out, e, depth);
            out.push('[');
            render_expr(out, idx, depth);
            out.push(']');
        }
        RustExpr::TupleIndex(e, i) => {
            render_expr(out, e, depth);
            out.push('.');
            out.push_str(&i.to_string());
        }

        RustExpr::StructInit { name, fields } => {
            out.push_str(name);
            out.push_str(" { ");
            for (i, (n, v)) in fields.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(n);
                out.push_str(": ");
                render_expr(out, v, depth);
            }
            out.push_str(" }");
        }
        RustExpr::StructUpdate { base, fields } => {
            out.push_str("{ ");
            for (n, v) in fields {
                out.push_str(n);
                out.push_str(": ");
                render_expr(out, v, depth);
                out.push_str(", ");
            }
            out.push_str("..");
            render_expr(out, base, depth);
            out.push_str(" }");
        }

        RustExpr::Closure { params, body } => {
            out.push_str("move |");
            for (i, p) in params.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(&p.name);
                if !matches!(p.ty, RustType::Infer) {
                    out.push_str(": ");
                    render_type(out, &p.ty);
                }
            }
            out.push_str("| ");
            render_expr(out, body, depth);
        }

        RustExpr::Format { template, args } => {
            out.push_str("format!(");
            out.push_str(template);
            for a in args {
                out.push_str(", ");
                render_expr(out, a, depth);
            }
            out.push(')');
        }

        RustExpr::Cast { expr, ty } => {
            render_expr(out, expr, depth);
            out.push_str(" as ");
            render_type(out, ty);
        }

        RustExpr::Unsafe(e) => {
            out.push_str("unsafe { ");
            render_expr(out, e, depth);
            out.push_str(" }");
        }

        RustExpr::Raw(code) => out.push_str(code),
    }
}

// ── Statements ───────────────────────────────────────────────────

fn render_stmt(out: &mut String, stmt: &RustStmt, depth: usize) {
    let ind = indent(depth);
    out.push_str(&ind);
    match stmt {
        RustStmt::Let { name, ty, mutable, value } => {
            out.push_str("let ");
            if *mutable { out.push_str("mut "); }
            out.push_str(name);
            if let Some(t) = ty {
                out.push_str(": ");
                render_type(out, t);
            }
            out.push_str(" = ");
            render_expr(out, value, depth);
            out.push_str(";\n");
        }
        RustStmt::LetPattern { pattern, value } => {
            out.push_str("let ");
            render_pattern(out, pattern);
            out.push_str(" = ");
            render_expr(out, value, depth);
            out.push_str(";\n");
        }
        RustStmt::Assign { target, value } => {
            out.push_str(target);
            out.push_str(" = ");
            render_expr(out, value, depth);
            out.push_str(";\n");
        }
        RustStmt::FieldAssign { target, field, value } => {
            out.push_str(target);
            out.push('.');
            out.push_str(field);
            out.push_str(" = ");
            render_expr(out, value, depth);
            out.push_str(";\n");
        }
        RustStmt::IndexAssign { target, index, value } => {
            out.push_str(target);
            out.push('[');
            render_expr(out, index, depth);
            out.push_str("] = ");
            render_expr(out, value, depth);
            out.push_str(";\n");
        }
        RustStmt::Expr(e) => {
            render_expr(out, e, depth);
            out.push_str(";\n");
        }
        RustStmt::Comment(text) => {
            out.push_str("// ");
            out.push_str(text);
            out.push('\n');
        }
    }
}

// ── Patterns ─────────────────────────────────────────────────────

fn render_pattern(out: &mut String, pat: &RustPattern) {
    match pat {
        RustPattern::Wildcard => out.push('_'),
        RustPattern::Var(name) => out.push_str(name),
        RustPattern::Literal(expr) => render_expr(out, expr, 0),
        RustPattern::Constructor { name, args } => {
            out.push_str(name);
            if !args.is_empty() {
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    render_pattern(out, a);
                }
                out.push(')');
            }
        }
        RustPattern::Struct { name, fields, rest } => {
            out.push_str(name);
            out.push_str(" { ");
            for (i, (n, p)) in fields.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(n);
                if let Some(pat) = p {
                    out.push_str(": ");
                    render_pattern(out, pat);
                }
            }
            if *rest {
                if !fields.is_empty() { out.push_str(", "); }
                out.push_str("..");
            }
            out.push_str(" }");
        }
        RustPattern::Tuple(elems) => {
            out.push('(');
            for (i, e) in elems.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                render_pattern(out, e);
            }
            out.push(')');
        }
        RustPattern::Box(inner) => {
            out.push_str("box ");
            render_pattern(out, inner);
        }
        RustPattern::Ref(inner) => {
            out.push_str("ref ");
            render_pattern(out, inner);
        }
        RustPattern::Or(pats) => {
            for (i, p) in pats.iter().enumerate() {
                if i > 0 { out.push_str(" | "); }
                render_pattern(out, p);
            }
        }
    }
}

// ── Types ────────────────────────────────────────────────────────

fn render_type(out: &mut String, ty: &RustType) {
    match ty {
        RustType::I64 => out.push_str("i64"),
        RustType::F64 => out.push_str("f64"),
        RustType::Bool => out.push_str("bool"),
        RustType::String => out.push_str("String"),
        RustType::Unit => out.push_str("()"),
        RustType::Vec(inner) => {
            out.push_str("Vec<");
            render_type(out, inner);
            out.push('>');
        }
        RustType::HashMap(k, v) => {
            out.push_str("HashMap<");
            render_type(out, k);
            out.push_str(", ");
            render_type(out, v);
            out.push('>');
        }
        RustType::Option(inner) => {
            out.push_str("Option<");
            render_type(out, inner);
            out.push('>');
        }
        RustType::Result(ok, err) => {
            out.push_str("Result<");
            render_type(out, ok);
            out.push_str(", ");
            render_type(out, err);
            out.push('>');
        }
        RustType::Tuple(elems) => {
            out.push('(');
            for (i, t) in elems.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                render_type(out, t);
            }
            out.push(')');
        }
        RustType::Named(name) => out.push_str(name),
        RustType::Generic(name, args) => {
            out.push_str(name);
            out.push('<');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                render_type(out, a);
            }
            out.push('>');
        }
        RustType::Ref(inner) => {
            out.push('&');
            render_type(out, inner);
        }
        RustType::RefStr => out.push_str("&str"),
        RustType::Slice(inner) => {
            out.push_str("&[");
            render_type(out, inner);
            out.push(']');
        }
        RustType::Fn(params, ret) => {
            out.push_str("impl Fn(");
            for (i, p) in params.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                render_type(out, p);
            }
            out.push_str(") -> ");
            render_type(out, ret);
            out.push_str(" + Clone");
        }
        RustType::Infer => out.push('_'),
    }
}

// ── Helpers ──────────────────────────────────────────────────────

fn indent(depth: usize) -> String {
    INDENT.repeat(depth)
}
