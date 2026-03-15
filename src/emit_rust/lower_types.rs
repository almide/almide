/// Type lowering + anonymous record collection.
///
/// Collects all unique anonymous record field-name sets from the IR,
/// generates AlmdRec struct definitions, and converts Ty → rust_ir::Type.

use std::collections::{HashMap, HashSet};
use almide::ir::*;
use almide::types::Ty;
use super::rust_ir::*;

// ── Ty → Type conversion ───────────────────────────────────────

pub fn lower_ty(ty: &Ty) -> Type {
    lower_ty_with(&HashMap::new(), &HashMap::new(), ty)
}

pub fn lower_ty_with(
    anon: &HashMap<Vec<String>, String>,
    named: &HashMap<Vec<String>, String>,
    ty: &Ty,
) -> Type {
    match ty {
        Ty::Int => Type::I64, Ty::Float => Type::F64, Ty::Bool => Type::Bool,
        Ty::String => Type::Str, Ty::Unit => Type::Unit,
        Ty::List(inner) => Type::Vec(Box::new(lower_ty_with(anon, named, inner))),
        Ty::Option(inner) => Type::Option(Box::new(lower_ty_with(anon, named, inner))),
        Ty::Result(ok, err) => Type::Result(Box::new(lower_ty_with(anon, named, ok)), Box::new(lower_ty_with(anon, named, err))),
        Ty::Map(k, v) => Type::HashMap(Box::new(lower_ty_with(anon, named, k)), Box::new(lower_ty_with(anon, named, v))),
        Ty::Tuple(elems) => Type::Tuple(elems.iter().map(|e| lower_ty_with(anon, named, e)).collect()),
        Ty::Named(n, args) if args.is_empty() => Type::Named(n.clone()),
        Ty::Named(n, args) => Type::Generic(n.clone(), args.iter().map(|a| lower_ty_with(anon, named, a)).collect()),
        Ty::Fn { params, ret } => Type::Fn(
            params.iter().map(|p| lower_ty_with(anon, named, p)).collect(),
            Box::new(lower_ty_with(anon, named, ret)),
        ),
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            let mut names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            names.sort();
            if let Some(n) = named.get(&names) { return Type::Named(n.clone()); }
            if let Some(n) = anon.get(&names) {
                // generics をフィールドのソート順で生成（struct 定義と一致させる）
                let mut sorted_fields: Vec<(&String, &Ty)> = fields.iter().map(|(n, t)| (n, t)).collect();
                sorted_fields.sort_by_key(|(n, _)| n.clone());
                let args: Vec<Type> = sorted_fields.iter().map(|(_, t)| lower_ty_with(anon, named, t)).collect();
                return if args.is_empty() { Type::Named(n.clone()) } else { Type::Generic(n.clone(), args) };
            }
            Type::Named("AnonRecord".into())
        }
        Ty::TypeVar(n) => Type::Named(n.clone()),
        Ty::Variant { name, .. } => Type::Named(name.clone()),
        Ty::Unknown | Ty::Union(_) => Type::Infer,
    }
}

pub fn is_copy(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Bool | Ty::Unit)
}

// ── Anonymous record collection ─────────────────────────────────

pub fn collect_anon_records(ir: &IrProgram) -> HashMap<Vec<String>, String> {
    let mut seen: HashSet<Vec<String>> = HashSet::new();
    let named: HashSet<Vec<String>> = ir.type_decls.iter().filter_map(|td| {
        if let IrTypeDeclKind::Record { fields } = &td.kind {
            let mut names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            names.sort();
            Some(names)
        } else { None }
    }).collect();

    for func in &ir.functions {
        for p in &func.params { collect_from_ty(&p.ty, &named, &mut seen); }
        collect_from_ty(&func.ret_ty, &named, &mut seen);
        collect_from_expr(&func.body, &named, &mut seen);
    }
    for tl in &ir.top_lets {
        collect_from_ty(&tl.ty, &named, &mut seen);
        collect_from_expr(&tl.value, &named, &mut seen);
    }

    let mut map = HashMap::new();
    let mut keys: Vec<Vec<String>> = seen.into_iter().collect();
    keys.sort();
    for (i, key) in keys.into_iter().enumerate() {
        map.insert(key, format!("AlmdRec{}", i));
    }
    map
}

fn collect_from_ty(ty: &Ty, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    match ty {
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            let mut names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            names.sort();
            if !named.contains(&names) { seen.insert(names); }
            for (_, t) in fields { collect_from_ty(t, named, seen); }
        }
        Ty::List(inner) | Ty::Option(inner) => collect_from_ty(inner, named, seen),
        Ty::Result(a, b) | Ty::Map(a, b) => { collect_from_ty(a, named, seen); collect_from_ty(b, named, seen); }
        Ty::Tuple(elems) => { for e in elems { collect_from_ty(e, named, seen); } }
        Ty::Fn { params, ret } => { for p in params { collect_from_ty(p, named, seen); } collect_from_ty(ret, named, seen); }
        _ => {}
    }
}

fn collect_from_expr(expr: &IrExpr, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    collect_from_ty(&expr.ty, named, seen);
    match &expr.kind {
        IrExprKind::Record { name: None, fields } => {
            let mut names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            names.sort();
            if !named.contains(&names) { seen.insert(names); }
            for (_, v) in fields { collect_from_expr(v, named, seen); }
        }
        IrExprKind::Record { fields, .. } | IrExprKind::SpreadRecord { fields, .. } => {
            for (_, v) in fields { collect_from_expr(v, named, seen); }
            if let IrExprKind::SpreadRecord { base, .. } = &expr.kind { collect_from_expr(base, named, seen); }
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { collect_from_stmt(s, named, seen); }
            if let Some(e) = expr { collect_from_expr(e, named, seen); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_from_expr(cond, named, seen); collect_from_expr(then, named, seen); collect_from_expr(else_, named, seen);
        }
        IrExprKind::Match { subject, arms } => {
            collect_from_expr(subject, named, seen);
            for arm in arms { if let Some(g) = &arm.guard { collect_from_expr(g, named, seen); } collect_from_expr(&arm.body, named, seen); }
        }
        IrExprKind::Call { args, target, .. } => {
            if let CallTarget::Method { object, .. } | CallTarget::Computed { callee: object } = target { collect_from_expr(object, named, seen); }
            for a in args { collect_from_expr(a, named, seen); }
        }
        IrExprKind::BinOp { left, right, .. } => { collect_from_expr(left, named, seen); collect_from_expr(right, named, seen); }
        IrExprKind::UnOp { operand, .. } => collect_from_expr(operand, named, seen),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => { for e in elements { collect_from_expr(e, named, seen); } }
        IrExprKind::MapLiteral { entries } => { for (k, v) in entries { collect_from_expr(k, named, seen); collect_from_expr(v, named, seen); } }
        IrExprKind::Lambda { body, .. } => collect_from_expr(body, named, seen),
        IrExprKind::ForIn { iterable, body, .. } => { collect_from_expr(iterable, named, seen); for s in body { collect_from_stmt(s, named, seen); } }
        IrExprKind::While { cond, body } => { collect_from_expr(cond, named, seen); for s in body { collect_from_stmt(s, named, seen); } }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => collect_from_expr(object, named, seen),
        IrExprKind::IndexAccess { object, index } => { collect_from_expr(object, named, seen); collect_from_expr(index, named, seen); }
        IrExprKind::Range { start, end, .. } => { collect_from_expr(start, named, seen); collect_from_expr(end, named, seen); }
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr } | IrExprKind::Await { expr } => collect_from_expr(expr, named, seen),
        IrExprKind::StringInterp { parts } => { for p in parts { if let IrStringPart::Expr { expr } = p { collect_from_expr(expr, named, seen); } } }
        _ => {}
    }
}

fn collect_from_stmt(stmt: &IrStmt, named: &HashSet<Vec<String>>, seen: &mut HashSet<Vec<String>>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. } | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => collect_from_expr(value, named, seen),
        IrStmtKind::IndexAssign { index, value, .. } => { collect_from_expr(index, named, seen); collect_from_expr(value, named, seen); }
        IrStmtKind::Guard { cond, else_ } => { collect_from_expr(cond, named, seen); collect_from_expr(else_, named, seen); }
        IrStmtKind::Expr { expr } => collect_from_expr(expr, named, seen),
        IrStmtKind::Comment { .. } => {}
    }
}

pub fn generate_anon_structs(anon: &HashMap<Vec<String>, String>) -> Vec<StructDef> {
    let mut result: Vec<StructDef> = anon.iter().map(|(field_names, struct_name)| {
        let generics: Vec<String> = (0..field_names.len())
            .map(|i| format!("T{}: Clone + std::fmt::Debug + PartialEq", i))
            .collect();
        let fields: Vec<(String, Type)> = field_names.iter().enumerate()
            .map(|(i, name)| (name.clone(), Type::Named(format!("T{}", i))))
            .collect();
        StructDef {
            name: struct_name.clone(), fields, generics,
            derives: vec!["Debug".into(), "Clone".into(), "PartialEq".into()],
            is_pub: false,
        }
    }).collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

/// Build named record field sets → type name map.
pub fn build_named_records(ir: &IrProgram) -> HashMap<Vec<String>, String> {
    let mut map = HashMap::new();
    for td in &ir.type_decls {
        if let IrTypeDeclKind::Record { fields } = &td.kind {
            let mut names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            names.sort();
            map.insert(names, td.name.clone());
        }
    }
    map
}

// ── TCO detection ───────────────────────────────────────────────

pub fn has_tail_self_call(fn_name: &str, expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Named { name }, .. } if name == fn_name => true,
        IrExprKind::If { then, else_, .. } => has_tail_self_call(fn_name, then) || has_tail_self_call(fn_name, else_),
        IrExprKind::Match { arms, .. } => arms.iter().any(|arm| has_tail_self_call(fn_name, &arm.body)),
        IrExprKind::Block { expr: Some(e), .. } => has_tail_self_call(fn_name, e),
        _ => false,
    }
}
