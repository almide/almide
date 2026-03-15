/// RustIR → Rust source code renderer.
///
/// Input:    &Program (RustIR)
/// Output:   String
/// Owns:     formatting, indentation, syntax rules
/// Does NOT: ANY codegen decisions — pure pattern match → string
///
/// Every decision was already made during IR → RustIR lowering.

use super::rust_ir::*;

const IND: &str = "    ";

pub fn program(p: &Program) -> String {
    let mut o = String::new();
    for l in &p.prelude { o.push_str(l); o.push('\n'); }
    if !p.prelude.is_empty() { o.push('\n'); }
    if !p.runtime.is_empty() { o.push_str(&p.runtime); o.push('\n'); }
    for s in &p.structs { struct_def(&mut o, s, 0); o.push('\n'); }
    for e in &p.enums { enum_def(&mut o, e, 0); o.push('\n'); }
    for f in &p.functions { function(&mut o, f, 0); o.push('\n'); }
    for f in &p.tests { function(&mut o, f, 0); o.push('\n'); }
    if let Some(m) = &p.main { function(&mut o, m, 0); o.push('\n'); }
    o
}

fn function(o: &mut String, f: &Function, d: usize) {
    let ind = IND.repeat(d);
    for a in &f.attrs { o.push_str(&ind); o.push_str(a); o.push('\n'); }
    o.push_str(&ind);
    if f.is_pub { o.push_str("pub "); }
    o.push_str("fn "); o.push_str(&f.name);
    if !f.generics.is_empty() { o.push('<'); o.push_str(&f.generics.join(", ")); o.push('>'); }
    o.push('(');
    for (i, p) in f.params.iter().enumerate() {
        if i > 0 { o.push_str(", "); }
        if p.mutable { o.push_str("mut "); }
        o.push_str(&p.name); o.push_str(": "); ty(o, &p.ty);
    }
    o.push_str(") -> "); ty(o, &f.ret); o.push_str(" {\n");
    for s in &f.body { stmt(o, s, d + 1); }
    if let Some(t) = &f.tail { o.push_str(&IND.repeat(d + 1)); expr(o, t, d + 1); o.push('\n'); }
    o.push_str(&ind); o.push_str("}\n");
}

fn struct_def(o: &mut String, s: &StructDef, d: usize) {
    let ind = IND.repeat(d);
    if !s.derives.is_empty() { o.push_str(&ind); o.push_str(&format!("#[derive({})]\n", s.derives.join(", "))); }
    o.push_str(&ind);
    if s.is_pub { o.push_str("pub "); }
    o.push_str("struct "); o.push_str(&s.name);
    if !s.generics.is_empty() { o.push_str(&format!("<{}>", s.generics.join(", "))); }
    o.push_str(" {\n");
    for (n, t) in &s.fields { o.push_str(&IND.repeat(d + 1)); o.push_str("pub "); o.push_str(n); o.push_str(": "); ty(o, t); o.push_str(",\n"); }
    o.push_str(&ind); o.push_str("}\n");
}

fn enum_def(o: &mut String, e: &EnumDef, d: usize) {
    let ind = IND.repeat(d);
    if !e.derives.is_empty() { o.push_str(&ind); o.push_str(&format!("#[derive({})]\n", e.derives.join(", "))); }
    o.push_str(&ind);
    if e.is_pub { o.push_str("pub "); }
    o.push_str("enum "); o.push_str(&e.name);
    if !e.generics.is_empty() { o.push_str(&format!("<{}>", e.generics.join(", "))); }
    o.push_str(" {\n");
    for v in &e.variants {
        o.push_str(&IND.repeat(d + 1)); o.push_str(&v.name);
        match &v.kind {
            VariantKind::Unit => {}
            VariantKind::Tuple(tys) => { o.push('('); for (i, t) in tys.iter().enumerate() { if i > 0 { o.push_str(", "); } ty(o, t); } o.push(')'); }
            VariantKind::Struct(fs) => { o.push_str(" { "); for (i, (n, t)) in fs.iter().enumerate() { if i > 0 { o.push_str(", "); } o.push_str(n); o.push_str(": "); ty(o, t); } o.push_str(" }"); }
        }
        o.push_str(",\n");
    }
    o.push_str(&ind); o.push_str("}\n");
}

fn expr(o: &mut String, e: &Expr, d: usize) {
    match e {
        Expr::Int(v) => { o.push_str(&format!("{}i64", v)); }
        Expr::Float(v) => { o.push_str(&format!("{:?}f64", v)); }
        Expr::Str(s) => { o.push_str(&format!("{:?}.to_string()", s)); }
        Expr::Bool(v) => { o.push_str(if *v { "true" } else { "false" }); }
        Expr::Unit => { o.push_str("()"); }
        Expr::Var(n) => { o.push_str(n); }

        Expr::BinOp { op, left, right } => { o.push('('); expr(o, left, d); o.push(' '); o.push_str(op); o.push(' '); expr(o, right, d); o.push(')'); }
        Expr::UnOp { op, operand } => { o.push_str(op); expr(o, operand, d); }

        Expr::Call { func, args } => { o.push_str(func); o.push('('); comma_exprs(o, args, d); o.push(')'); }
        Expr::MethodCall { recv, method, args } => { expr(o, recv, d); o.push('.'); o.push_str(method); o.push('('); comma_exprs(o, args, d); o.push(')'); }
        Expr::Macro { name, args } => { o.push_str(name); o.push_str("!("); comma_exprs(o, args, d); o.push(')'); }

        Expr::If { cond, then, else_ } => {
            o.push_str("if "); expr(o, cond, d); o.push_str(" { "); expr(o, then, d); o.push_str(" }");
            if let Some(e) = else_ { o.push_str(" else { "); expr(o, e, d); o.push_str(" }"); }
        }
        Expr::Match { subject, arms } => {
            o.push_str("match "); expr(o, subject, d); o.push_str(" {\n");
            for arm in arms {
                o.push_str(&IND.repeat(d + 1)); pat(o, &arm.pat);
                if let Some(g) = &arm.guard { o.push_str(" if "); expr(o, g, d + 1); }
                o.push_str(" => "); expr(o, &arm.body, d + 1); o.push_str(",\n");
            }
            o.push_str(&IND.repeat(d)); o.push('}');
        }
        Expr::Block { stmts, tail } => {
            o.push_str("{\n");
            for s in stmts { stmt(o, s, d + 1); }
            if let Some(t) = tail { o.push_str(&IND.repeat(d + 1)); expr(o, t, d + 1); o.push('\n'); }
            o.push_str(&IND.repeat(d)); o.push('}');
        }
        Expr::For { var, iter, body } => {
            o.push_str("for "); o.push_str(var); o.push_str(" in "); expr(o, iter, d); o.push_str(" {\n");
            for s in body { stmt(o, s, d + 1); }
            o.push_str(&IND.repeat(d)); o.push('}');
        }
        Expr::While { cond, body } => {
            o.push_str("while "); expr(o, cond, d); o.push_str(" {\n");
            for s in body { stmt(o, s, d + 1); }
            o.push_str(&IND.repeat(d)); o.push('}');
        }
        Expr::Loop { label, body } => {
            if let Some(l) = label { o.push_str(&format!("'{}: ", l)); }
            o.push_str("loop {\n");
            for s in body { stmt(o, s, d + 1); }
            o.push_str(&IND.repeat(d)); o.push('}');
        }
        Expr::Break => { o.push_str("break"); }
        Expr::Continue { label } => { o.push_str("continue"); if let Some(l) = label { o.push_str(&format!(" '{}", l)); } }
        Expr::Return(v) => { o.push_str("return"); if let Some(e) = v { o.push(' '); expr(o, e, d); } }

        Expr::Clone(e) => { expr(o, e, d); o.push_str(".clone()"); }
        Expr::Borrow(e) => { o.push('&'); expr(o, e, d); }
        Expr::Try(e) => { expr(o, e, d); o.push('?'); }
        Expr::Ok(e) => { o.push_str("Ok("); expr(o, e, d); o.push(')'); }
        Expr::Err(e) => { o.push_str("Err("); expr(o, e, d); o.push(')'); }
        Expr::Some(e) => { o.push_str("Some("); expr(o, e, d); o.push(')'); }
        Expr::None => { o.push_str("None"); }

        Expr::Vec(elems) => { o.push_str("vec!["); comma_exprs(o, elems, d); o.push(']'); }
        Expr::HashMap(entries) => {
            o.push_str("HashMap::from([");
            for (i, (k, v)) in entries.iter().enumerate() { if i > 0 { o.push_str(", "); } o.push('('); expr(o, k, d); o.push_str(", "); expr(o, v, d); o.push(')'); }
            o.push_str("])");
        }
        Expr::Tuple(elems) => { o.push('('); comma_exprs(o, elems, d); o.push(')'); }
        Expr::Range { start, end, inclusive, elem_ty } => {
            o.push('('); expr(o, start, d);
            o.push_str(if *inclusive { "..=" } else { ".." });
            expr(o, end, d); o.push_str(").collect::<Vec<"); ty(o, elem_ty); o.push_str(">>()");
        }

        Expr::Field(e, f) => { expr(o, e, d); o.push('.'); o.push_str(f); }
        Expr::Index(e, i) => { expr(o, e, d); o.push_str("[("); expr(o, i, d); o.push_str(") as usize].clone()"); }
        Expr::TupleIdx(e, i) => { expr(o, e, d); o.push('.'); o.push_str(&i.to_string()); }

        Expr::Struct { name, fields } => {
            o.push_str(name); o.push_str(" { ");
            for (i, (n, v)) in fields.iter().enumerate() { if i > 0 { o.push_str(", "); } o.push_str(n); o.push_str(": "); expr(o, v, d); }
            o.push_str(" }");
        }
        Expr::StructUpdate { base, fields } => {
            o.push_str("{ ");
            for (n, v) in fields { o.push_str(n); o.push_str(": "); expr(o, v, d); o.push_str(", "); }
            o.push_str(".."); expr(o, base, d); o.push_str(" }");
        }

        Expr::Closure { params, body } => {
            o.push_str("move |"); o.push_str(&params.join(", ")); o.push_str("| "); expr(o, body, d);
        }
        Expr::Format { template, args } => {
            o.push_str("format!("); o.push_str(template);
            for a in args { o.push_str(", "); expr(o, a, d); }
            o.push(')');
        }
        Expr::Raw(code) => { o.push_str(code); }
    }
}

fn stmt(o: &mut String, s: &Stmt, d: usize) {
    let ind = IND.repeat(d);
    o.push_str(&ind);
    match s {
        Stmt::Let { name, ty: t, mutable, value } => {
            o.push_str("let "); if *mutable { o.push_str("mut "); }
            o.push_str(name);
            if let Some(t) = t { o.push_str(": "); ty(o, t); }
            o.push_str(" = "); expr(o, value, d); o.push_str(";\n");
        }
        Stmt::LetPattern { pattern, value } => {
            o.push_str("let "); pat(o, pattern); o.push_str(" = "); expr(o, value, d); o.push_str(";\n");
        }
        Stmt::Assign { target, value } => { o.push_str(target); o.push_str(" = "); expr(o, value, d); o.push_str(";\n"); }
        Stmt::FieldAssign { target, field, value } => { o.push_str(target); o.push('.'); o.push_str(field); o.push_str(" = "); expr(o, value, d); o.push_str(";\n"); }
        Stmt::IndexAssign { target, index, value } => { o.push_str(target); o.push('['); expr(o, index, d); o.push_str("] = "); expr(o, value, d); o.push_str(";\n"); }
        Stmt::Expr(e) => { expr(o, e, d); o.push_str(";\n"); }
    }
}

fn pat(o: &mut String, p: &Pattern) {
    match p {
        Pattern::Wild => o.push('_'),
        Pattern::Var(n) => o.push_str(n),
        Pattern::Lit(e) => match e {
            Expr::Str(s) => o.push_str(&format!("{:?}", s)),
            _ => expr(o, e, 0),
        },
        Pattern::Ctor { name, args } => {
            o.push_str(name);
            if !args.is_empty() { o.push('('); for (i, a) in args.iter().enumerate() { if i > 0 { o.push_str(", "); } pat(o, a); } o.push(')'); }
        }
        Pattern::Struct { name, fields, rest } => {
            o.push_str(name); o.push_str(" { ");
            for (i, (n, p)) in fields.iter().enumerate() { if i > 0 { o.push_str(", "); } o.push_str(n); if let Some(p) = p { o.push_str(": "); pat(o, p); } }
            if *rest { if !fields.is_empty() { o.push_str(", "); } o.push_str(".."); }
            o.push_str(" }");
        }
        Pattern::Tuple(elems) => { o.push('('); for (i, e) in elems.iter().enumerate() { if i > 0 { o.push_str(", "); } pat(o, e); } o.push(')'); }
    }
}

pub fn render_type(o: &mut String, t: &Type) { ty(o, t); }

fn ty(o: &mut String, t: &Type) {
    match t {
        Type::I64 => o.push_str("i64"),
        Type::F64 => o.push_str("f64"),
        Type::Bool => o.push_str("bool"),
        Type::Str => o.push_str("String"),
        Type::Unit => o.push_str("()"),
        Type::Vec(inner) => { o.push_str("Vec<"); ty(o, inner); o.push('>'); }
        Type::HashMap(k, v) => { o.push_str("HashMap<"); ty(o, k); o.push_str(", "); ty(o, v); o.push('>'); }
        Type::Option(inner) => { o.push_str("Option<"); ty(o, inner); o.push('>'); }
        Type::Result(ok, err) => { o.push_str("Result<"); ty(o, ok); o.push_str(", "); ty(o, err); o.push('>'); }
        Type::Tuple(elems) => { o.push('('); for (i, t) in elems.iter().enumerate() { if i > 0 { o.push_str(", "); } ty(o, t); } o.push(')'); }
        Type::Named(n) => o.push_str(n),
        Type::Generic(n, args) => { o.push_str(n); o.push('<'); for (i, a) in args.iter().enumerate() { if i > 0 { o.push_str(", "); } ty(o, a); } o.push('>'); }
        Type::Ref(inner) => { o.push('&'); ty(o, inner); }
        Type::RefStr => o.push_str("&str"),
        Type::Slice(inner) => { o.push_str("&["); ty(o, inner); o.push(']'); }
        Type::Fn(params, ret) => { o.push_str("impl Fn("); for (i, p) in params.iter().enumerate() { if i > 0 { o.push_str(", "); } ty(o, p); } o.push_str(") -> "); ty(o, ret); o.push_str(" + Clone"); }
        Type::Infer => o.push('_'),
    }
}

fn comma_exprs(o: &mut String, exprs: &[Expr], d: usize) {
    for (i, e) in exprs.iter().enumerate() { if i > 0 { o.push_str(", "); } expr(o, e, d); }
}

/// Render a type to string (utility).
pub fn ty_str(t: &Type) -> String { let mut o = String::new(); ty(&mut o, t); o }
/// Render an expression to string (utility).
pub fn expr_str(e: &Expr) -> String { let mut o = String::new(); expr(&mut o, e, 0); o }
