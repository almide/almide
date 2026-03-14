/// IR → RustIR lowering pass.
///
/// All codegen decisions are made here:
/// - auto-? insertion
/// - clone/borrow insertion
/// - Ok wrapping for effect functions
/// - mut determination
/// - type annotations
///
/// The output is a clean RustIR tree that the Render pass can emit without decisions.

use almide::ir::*;
use almide::types::Ty;
use super::rust_ir::*;

/// Check if a function body has self-recursive tail calls (for TCO).
fn has_tail_self_call(fn_name: &str, expr: &IrExpr) -> bool {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Named { name }, .. } if name == fn_name => true,
        IrExprKind::If { then, else_, .. } => {
            has_tail_self_call(fn_name, then) || has_tail_self_call(fn_name, else_)
        }
        IrExprKind::Match { arms, .. } => {
            arms.iter().any(|arm| has_tail_self_call(fn_name, &arm.body))
        }
        IrExprKind::Block { expr: Some(e), .. } => has_tail_self_call(fn_name, e),
        _ => false,
    }
}

/// Collect all unique anonymous record field-name sets from the IR program.
/// Returns a map: sorted field names → AlmdRec{N} struct name.
/// Skips field sets that match a named record type declaration.
fn collect_anon_records(ir: &IrProgram) -> std::collections::HashMap<Vec<String>, String> {
    let mut seen: std::collections::HashSet<Vec<String>> = std::collections::HashSet::new();

    // Build named record field sets to exclude
    let mut named_record_fields: std::collections::HashSet<Vec<String>> = std::collections::HashSet::new();
    for td in &ir.type_decls {
        if let IrTypeDeclKind::Record { fields } = &td.kind {
            let mut names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            names.sort();
            named_record_fields.insert(names);
        }
    }

    // Walk all expressions and types to find anonymous records
    for func in &ir.functions {
        // Check param types for Record/OpenRecord
        for p in &func.params {
            collect_anon_from_ty(&p.ty, &named_record_fields, &mut seen);
        }
        // Check return type
        collect_anon_from_ty(&func.ret_ty, &named_record_fields, &mut seen);
        // Walk function body
        collect_anon_from_expr(&func.body, &named_record_fields, &mut seen);
    }
    for tl in &ir.top_lets {
        collect_anon_from_ty(&tl.ty, &named_record_fields, &mut seen);
        collect_anon_from_expr(&tl.value, &named_record_fields, &mut seen);
    }

    // Assign names: AlmdRec0, AlmdRec1, ...
    let mut map = std::collections::HashMap::new();
    let mut sorted_keys: Vec<Vec<String>> = seen.into_iter().collect();
    sorted_keys.sort(); // deterministic ordering
    for (i, key) in sorted_keys.into_iter().enumerate() {
        map.insert(key, format!("AlmdRec{}", i));
    }
    map
}

/// Collect anonymous record field sets from a type.
fn collect_anon_from_ty(
    ty: &Ty,
    named: &std::collections::HashSet<Vec<String>>,
    seen: &mut std::collections::HashSet<Vec<String>>,
) {
    match ty {
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            let mut names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            names.sort();
            if !named.contains(&names) {
                seen.insert(names);
            }
            // Recurse into field types
            for (_, ft) in fields {
                collect_anon_from_ty(ft, named, seen);
            }
        }
        Ty::List(inner) | Ty::Option(inner) => collect_anon_from_ty(inner, named, seen),
        Ty::Result(ok, err) | Ty::Map(ok, err) => {
            collect_anon_from_ty(ok, named, seen);
            collect_anon_from_ty(err, named, seen);
        }
        Ty::Tuple(elems) => {
            for e in elems { collect_anon_from_ty(e, named, seen); }
        }
        Ty::Fn { params, ret } => {
            for p in params { collect_anon_from_ty(p, named, seen); }
            collect_anon_from_ty(ret, named, seen);
        }
        Ty::Named(_, args) => {
            for a in args { collect_anon_from_ty(a, named, seen); }
        }
        _ => {}
    }
}

/// Collect anonymous record field sets from an expression tree.
fn collect_anon_from_expr(
    expr: &IrExpr,
    named: &std::collections::HashSet<Vec<String>>,
    seen: &mut std::collections::HashSet<Vec<String>>,
) {
    // Check the expression's type
    collect_anon_from_ty(&expr.ty, named, seen);

    match &expr.kind {
        IrExprKind::Record { name: None, fields } => {
            let mut names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            names.sort();
            if !named.contains(&names) {
                seen.insert(names);
            }
            for (_, v) in fields {
                collect_anon_from_expr(v, named, seen);
            }
        }
        IrExprKind::Record { name: Some(_), fields } => {
            for (_, v) in fields {
                collect_anon_from_expr(v, named, seen);
            }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            collect_anon_from_expr(base, named, seen);
            for (_, v) in fields {
                collect_anon_from_expr(v, named, seen);
            }
        }
        IrExprKind::Block { stmts, expr } | IrExprKind::DoBlock { stmts, expr } => {
            for s in stmts { collect_anon_from_stmt(s, named, seen); }
            if let Some(e) = expr { collect_anon_from_expr(e, named, seen); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_anon_from_expr(cond, named, seen);
            collect_anon_from_expr(then, named, seen);
            collect_anon_from_expr(else_, named, seen);
        }
        IrExprKind::Match { subject, arms } => {
            collect_anon_from_expr(subject, named, seen);
            for arm in arms {
                if let Some(g) = &arm.guard { collect_anon_from_expr(g, named, seen); }
                collect_anon_from_expr(&arm.body, named, seen);
            }
        }
        IrExprKind::Call { args, target, .. } => {
            if let CallTarget::Method { object, .. } = target {
                collect_anon_from_expr(object, named, seen);
            }
            if let CallTarget::Computed { callee } = target {
                collect_anon_from_expr(callee, named, seen);
            }
            for a in args { collect_anon_from_expr(a, named, seen); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            collect_anon_from_expr(left, named, seen);
            collect_anon_from_expr(right, named, seen);
        }
        IrExprKind::UnOp { operand, .. } => collect_anon_from_expr(operand, named, seen),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { collect_anon_from_expr(e, named, seen); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                collect_anon_from_expr(k, named, seen);
                collect_anon_from_expr(v, named, seen);
            }
        }
        IrExprKind::Lambda { body, .. } => collect_anon_from_expr(body, named, seen),
        IrExprKind::ForIn { iterable, body, .. } => {
            collect_anon_from_expr(iterable, named, seen);
            for s in body { collect_anon_from_stmt(s, named, seen); }
        }
        IrExprKind::While { cond, body } => {
            collect_anon_from_expr(cond, named, seen);
            for s in body { collect_anon_from_stmt(s, named, seen); }
        }
        IrExprKind::Member { object, .. } => collect_anon_from_expr(object, named, seen),
        IrExprKind::IndexAccess { object, index } => {
            collect_anon_from_expr(object, named, seen);
            collect_anon_from_expr(index, named, seen);
        }
        IrExprKind::TupleIndex { object, .. } => collect_anon_from_expr(object, named, seen),
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Await { expr } => collect_anon_from_expr(expr, named, seen),
        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr } = part {
                    collect_anon_from_expr(expr, named, seen);
                }
            }
        }
        IrExprKind::Range { start, end, .. } => {
            collect_anon_from_expr(start, named, seen);
            collect_anon_from_expr(end, named, seen);
        }
        _ => {} // Literals, Var, Unit, Break, Continue, OptionNone, EmptyMap, Hole, Todo
    }
}

/// Collect anonymous record field sets from a statement.
fn collect_anon_from_stmt(
    stmt: &IrStmt,
    named: &std::collections::HashSet<Vec<String>>,
    seen: &mut std::collections::HashSet<Vec<String>>,
) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => collect_anon_from_expr(value, named, seen),
        IrStmtKind::Assign { value, .. } => collect_anon_from_expr(value, named, seen),
        IrStmtKind::IndexAssign { index, value, .. } => {
            collect_anon_from_expr(index, named, seen);
            collect_anon_from_expr(value, named, seen);
        }
        IrStmtKind::FieldAssign { value, .. } => collect_anon_from_expr(value, named, seen),
        IrStmtKind::Expr { expr } => collect_anon_from_expr(expr, named, seen),
        IrStmtKind::Guard { cond, else_ } => {
            collect_anon_from_expr(cond, named, seen);
            collect_anon_from_expr(else_, named, seen);
        }
        IrStmtKind::BindDestructure { value, .. } => collect_anon_from_expr(value, named, seen),
        IrStmtKind::Comment { .. } => {}
    }
}

/// Generate RustStruct entries for anonymous record types.
/// Each gets generic type params T0, T1, ... for its fields.
fn generate_anon_record_structs(
    anon_records: &std::collections::HashMap<Vec<String>, String>,
) -> Vec<RustStruct> {
    let mut result: Vec<RustStruct> = anon_records.iter().map(|(field_names, struct_name)| {
        let generics: Vec<String> = (0..field_names.len())
            .map(|i| format!("T{}: Clone + std::fmt::Debug + PartialEq", i))
            .collect();
        let fields: Vec<(String, RustType)> = field_names.iter().enumerate()
            .map(|(i, name)| (name.clone(), RustType::Named(format!("T{}", i))))
            .collect();
        RustStruct {
            name: struct_name.clone(),
            fields,
            generics,
            derives: vec!["Debug".into(), "Clone".into(), "PartialEq".into()],
            is_pub: false,
        }
    }).collect();
    result.sort_by(|a, b| a.name.cmp(&b.name)); // deterministic order
    result
}

/// Lower an entire IrProgram to a RustProgram.
pub fn lower_program(ir: &IrProgram) -> RustProgram {
    // Collect effect/result function names
    let effect_fns: Vec<String> = ir.functions.iter()
        .filter(|f| f.is_effect && !f.is_test)
        .map(|f| f.name.clone())
        .collect();
    let result_fns: Vec<String> = ir.functions.iter()
        .filter(|f| {
            matches!(&f.ret_ty, Ty::Result(_, _))
        })
        .map(|f| f.name.clone())
        .collect();

    // Collect anonymous record field-name sets and assign struct names
    let anon_records = collect_anon_records(ir);

    // Build named record field sets → type name map (for lower_type lookups)
    let mut named_record_types: std::collections::HashMap<Vec<String>, String> = std::collections::HashMap::new();
    for td in &ir.type_decls {
        if let IrTypeDeclKind::Record { fields } = &td.kind {
            let mut names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            names.sort();
            named_record_types.insert(names, td.name.clone());
        }
    }

    // Lower type declarations
    let mut structs = Vec::new();
    let mut enums = Vec::new();
    let mut type_aliases = Vec::new();
    for td in &ir.type_decls {
        match &td.kind {
            IrTypeDeclKind::Record { fields } => {
                structs.push(RustStruct {
                    name: td.name.clone(),
                    fields: fields.iter().map(|f| (f.name.clone(), lower_type(&f.ty))).collect(),
                    generics: td.generics.as_ref()
                        .map(|gs| gs.iter().map(|g| format!("{}: Clone + std::fmt::Debug + PartialEq", g.name)).collect())
                        .unwrap_or_default(),
                    derives: vec!["Debug".into(), "Clone".into(), "PartialEq".into()],
                    is_pub: matches!(td.visibility, IrVisibility::Public),
                });
            }
            IrTypeDeclKind::Variant { cases, .. } => {
                let variants: Vec<RustVariant> = cases.iter().map(|c| {
                    let kind = match &c.kind {
                        IrVariantKind::Unit => RustVariantKind::Unit,
                        IrVariantKind::Tuple { fields } => {
                            RustVariantKind::Tuple(fields.iter().map(lower_type).collect())
                        }
                        IrVariantKind::Record { fields } => {
                            RustVariantKind::Struct(fields.iter().map(|f| (f.name.clone(), lower_type(&f.ty))).collect())
                        }
                    };
                    RustVariant { name: c.name.clone(), kind }
                }).collect();
                enums.push(RustEnum {
                    name: td.name.clone(),
                    variants,
                    generics: td.generics.as_ref()
                        .map(|gs| gs.iter().map(|g| format!("{}: Clone + std::fmt::Debug + PartialEq", g.name)).collect())
                        .unwrap_or_default(),
                    derives: vec!["Debug".into(), "Clone".into(), "PartialEq".into()],
                    is_pub: matches!(td.visibility, IrVisibility::Public),
                });
            }
            IrTypeDeclKind::Alias { target } => {
                type_aliases.push(RustTypeAlias {
                    name: td.name.clone(),
                    ty: lower_type(target),
                    is_pub: matches!(td.visibility, IrVisibility::Public),
                });
            }
        }
    }

    // Run borrow analysis
    let empty_module_irs = std::collections::HashMap::new();
    let borrow_info = super::borrow::analyze_program(ir, &empty_module_irs);

    // Build variant constructor → enum name map
    let mut variant_ctors: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for td in &ir.type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
            for case in cases {
                variant_ctors.insert(case.name.clone(), td.name.clone());
            }
        }
    }

    // Collect open record shape aliases
    let mut open_record_aliases: std::collections::HashMap<String, Vec<(String, Ty)>> = std::collections::HashMap::new();
    for td in &ir.type_decls {
        if let IrTypeDeclKind::Alias { target } = &td.kind {
            if let Ty::OpenRecord { fields } = target {
                open_record_aliases.insert(td.name.clone(), fields.clone());
            }
        }
    }

    // Build open record params: fn_name → [(param_idx, struct_name, field_infos)]
    let mut open_record_params: std::collections::HashMap<String, Vec<(usize, String, Vec<super::OpenFieldInfo>)>> = std::collections::HashMap::new();
    for func in &ir.functions {
        let mut fn_open_recs: Vec<(usize, String, Vec<super::OpenFieldInfo>)> = Vec::new();
        for (i, p) in func.params.iter().enumerate() {
            let fields_opt = match &p.ty {
                Ty::OpenRecord { fields } => Some(fields.clone()),
                Ty::Named(alias_name, _) => open_record_aliases.get(alias_name).cloned(),
                _ => None,
            };
            if let Some(fields) = fields_opt {
                let field_infos: Vec<super::OpenFieldInfo> = fields.iter().map(|(n, _)| {
                    super::OpenFieldInfo { name: n.clone(), nested: None }
                }).collect();
                let mut field_names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                field_names.sort();
                let struct_name = anon_records.get(&field_names)
                    .cloned()
                    .unwrap_or_else(|| format!("AlmdRec{}", anon_records.len()));
                fn_open_recs.push((i, struct_name, field_infos));
            }
        }
        if !fn_open_recs.is_empty() {
            open_record_params.insert(func.name.clone(), fn_open_recs);
        }
    }

    // Lower functions
    let mut functions = Vec::new();
    let mut test_functions = Vec::new();
    for func in &ir.functions {
        let single_use = compute_single_use(&ir.var_table, &func.params);
        // Track which params are borrowed based on borrow analysis
        let mut borrowed_params = std::collections::HashSet::new();
        for (i, p) in func.params.iter().enumerate() {
            if borrow_info.param_ownership(&func.name, i) == super::borrow::ParamOwnership::Borrow {
                borrowed_params.insert(p.name.clone());
            }
        }
        let ctx = LowerCtx {
            var_table: &ir.var_table,
            in_effect: func.is_effect,
            in_test: func.is_test,
            in_do_block: false,
            effect_fns: &effect_fns,
            result_fns: &result_fns,
            current_fn_name: Some(func.name.clone()),
            variant_ctors: &variant_ctors,
            single_use_vars: single_use,
            borrow_info: &borrow_info,
            borrowed_params: borrowed_params.clone(),
            anon_records: &anon_records,
            named_record_types: &named_record_types,
            open_record_params: &open_record_params,
        };

        let lty = |ty: &Ty| lower_type_with_records(ty, &anon_records, &named_record_types);

        // TCO detection (before params so we can set mutable)
        let use_tco = has_tail_self_call(&func.name, &func.body);

        let ret_ty = if func.is_test {
            RustType::Unit
        } else if func.is_effect {
            match &func.ret_ty {
                Ty::Result(_, _) => lty(&func.ret_ty),
                Ty::Unit => RustType::Result(Box::new(RustType::Unit), Box::new(RustType::String)),
                _ => RustType::Result(Box::new(lty(&func.ret_ty)), Box::new(RustType::String)),
            }
        } else {
            lty(&func.ret_ty)
        };

        let params: Vec<RustParam> = func.params.iter().enumerate().map(|(i, p)| {
            let ty = if borrow_info.param_ownership(&func.name, i) == super::borrow::ParamOwnership::Borrow {
                // Borrowed param: String → &str, Vec<T> → &[T]
                match &p.ty {
                    Ty::String => RustType::RefStr,
                    Ty::List(inner) => RustType::Slice(Box::new(lty(inner))),
                    _ => lty(&p.ty),
                }
            } else {
                lty(&p.ty)
            };
            RustParam {
                name: crate::emit_common::sanitize(&p.name),
                ty,
                mutable: use_tco,
            }
        }).collect();

        let body_expr = if use_tco {
            // Lower body with TCO: wrap in loop, replace tail calls with param reassignment + continue
            let param_names: Vec<String> = func.params.iter().map(|p| p.name.clone()).collect();
            let tco_body = ctx.lower_tco_expr(&func.body, &func.name, &param_names);
            RustExpr::Loop {
                label: Some("_tco".to_string()),
                body: vec![RustStmt::Expr(tco_body)],
            }
        } else {
            ctx.lower_expr(&func.body)
        };

        // Wrap body: effect functions need Ok wrapping
        let (body_stmts, tail_expr) = match body_expr {
            RustExpr::Block { stmts, expr } => {
                if func.is_effect {
                    let tail = expr.map(|e| {
                        // Wrap in Ok if not already a Result
                        match e.as_ref() {
                            RustExpr::ResultOk(_) | RustExpr::ResultErr(_) => *e,
                            _ => RustExpr::ResultOk(e),
                        }
                    }).unwrap_or(RustExpr::ResultOk(Box::new(RustExpr::Unit)));
                    (stmts, Some(tail))
                } else {
                    (stmts, expr.map(|e| *e))
                }
            }
            other => {
                if func.is_effect {
                    let wrapped = match &other {
                        RustExpr::ResultOk(_) | RustExpr::ResultErr(_) => other,
                        _ => RustExpr::ResultOk(Box::new(other)),
                    };
                    (vec![], Some(wrapped))
                } else {
                    (vec![], Some(other))
                }
            }
        };

        let fn_name = if func.name == "main" {
            "almide_main".to_string()
        } else if func.is_test {
            // Sanitize test name: replace spaces/special chars with underscores
            let sanitized = func.name.replace(|c: char| !c.is_alphanumeric() && c != '_', "_");
            format!("test_{}", sanitized)
        } else {
            crate::emit_common::sanitize(&func.name)
        };

        // Generics: emit type params with Clone + Debug + PartialEq bounds
        let generics: Vec<String> = func.generics.as_ref()
            .map(|gs| gs.iter()
                .filter(|g| g.structural_bound.is_none()) // structural bounds are monomorphized, not generic
                .map(|g| format!("{}: Clone + std::fmt::Debug + PartialEq + PartialOrd", g.name))
                .collect())
            .unwrap_or_default();

        let rust_fn = RustFunction {
            name: fn_name,
            generics,
            params,
            ret_ty,
            body: body_stmts,
            tail_expr,
            attrs: if func.is_test { vec!["#[test]".to_string()] } else { vec![] },
            is_pub: !func.is_test,
            is_async: func.is_async,
        };

        if func.is_test {
            test_functions.push(rust_fn);
        } else {
            functions.push(rust_fn);
        }
    }

    // Lower top-level lets as constants
    let consts: Vec<RustConst> = ir.top_lets.iter().map(|tl| {
        let ctx = LowerCtx {
            var_table: &ir.var_table,
            in_effect: false,
            in_test: false,
            in_do_block: false,
            effect_fns: &effect_fns,
            result_fns: &result_fns,
            current_fn_name: None,
            variant_ctors: &variant_ctors,
            single_use_vars: std::collections::HashSet::new(),
            borrow_info: &borrow_info,
            borrowed_params: std::collections::HashSet::new(),
            anon_records: &anon_records,
            named_record_types: &named_record_types,
            open_record_params: &open_record_params,
        };
        let info = ir.var_table.get(tl.var);
        RustConst {
            name: crate::emit_common::sanitize(&info.name).to_uppercase(),
            ty: lower_type_with_records(&tl.ty, &anon_records, &named_record_types),
            value: ctx.lower_expr(&tl.value),
            is_pub: true,
        }
    }).collect();

    // Build runtime string
    let mut runtime = String::new();
    runtime.push_str("trait AlmideConcat<Rhs> { type Output; fn concat(self, rhs: Rhs) -> Self::Output; }\n");
    runtime.push_str("impl AlmideConcat<String> for String { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }\n");
    runtime.push_str("impl AlmideConcat<&str> for String { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }\n");
    runtime.push_str("impl AlmideConcat<String> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }\n");
    runtime.push_str("impl AlmideConcat<&str> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }\n");
    runtime.push_str("impl<T: Clone> AlmideConcat<Vec<T>> for Vec<T> { type Output = Vec<T>; #[inline(always)] fn concat(self, rhs: Vec<T>) -> Vec<T> { let mut r = self; r.extend(rhs); r } }\n");
    runtime.push_str("macro_rules! almide_eq { ($a:expr, $b:expr) => { ($a) == ($b) }; }\n");
    runtime.push_str("macro_rules! almide_ne { ($a:expr, $b:expr) => { ($a) != ($b) }; }\n");
    runtime.push_str("\n");
    // Embedded runtime files
    runtime.push_str(super::IO_RUNTIME);
    runtime.push('\n');
    runtime.push_str(super::JSON_RUNTIME);
    runtime.push('\n');
    runtime.push_str(super::HTTP_RUNTIME);
    runtime.push('\n');
    runtime.push_str(super::TIME_RUNTIME);
    runtime.push('\n');
    runtime.push_str(super::REGEX_RUNTIME);
    runtime.push('\n');
    runtime.push_str(super::PLATFORM_RUNTIME);
    runtime.push('\n');
    runtime.push_str(super::COLLECTION_RUNTIME);
    runtime.push('\n');
    runtime.push_str(super::CORE_RUNTIME);
    runtime.push('\n');
    runtime.push_str(super::DATETIME_RUNTIME);
    runtime.push('\n');

    // Main wrapper for executables
    let main_wrapper = if let Some(main_fn) = ir.functions.iter().find(|f| f.name == "main") {
        let has_args = !main_fn.params.is_empty();
        let is_effect = main_fn.is_effect;
        let mut body = Vec::new();
        if has_args {
            body.push(RustStmt::Let {
                name: "args".to_string(),
                ty: Some(RustType::Vec(Box::new(RustType::String))),
                mutable: false,
                value: RustExpr::Raw("std::env::args().skip(1).collect()".to_string()),
            });
        }
        let call_args = if has_args {
            vec![RustExpr::Var("args".to_string())]
        } else {
            vec![]
        };
        let call = RustExpr::Call { func: "almide_main".to_string(), args: call_args };
        if is_effect {
            body.push(RustStmt::Expr(RustExpr::Block {
                stmts: vec![RustStmt::Expr(RustExpr::If {
                    cond: Box::new(RustExpr::MethodCall {
                        receiver: Box::new(call),
                        method: "is_err".to_string(),
                        args: vec![],
                    }),
                    then: Box::new(RustExpr::Block {
                        stmts: vec![RustStmt::Expr(RustExpr::Raw("std::process::exit(1)".to_string()))],
                        expr: None,
                    }),
                    else_: None,
                })],
                expr: None,
            }));
        } else {
            body.push(RustStmt::Expr(call));
        }
        Some(RustFunction {
            name: "main".to_string(),
            generics: vec![],
            params: vec![],
            ret_ty: RustType::Unit,
            body,
            tail_expr: None,
            attrs: vec![],
            is_pub: false,
            is_async: false,
        })
    } else {
        None
    };

    // Generate anonymous record structs and prepend them (before user-defined structs)
    let mut anon_structs = generate_anon_record_structs(&anon_records);
    anon_structs.extend(structs);
    let structs = anon_structs;

    RustProgram {
        prelude: vec![
            "#![allow(unused_parens, unused_variables, dead_code, unused_imports, unused_mut, unused_must_use)]".to_string(),
            String::new(),
            "use std::collections::HashMap;".to_string(),
        ],
        macros: vec![],
        structs,
        enums,
        type_aliases,
        consts,
        impls: vec![],
        functions,
        test_functions,
        main_wrapper,
        runtime,
    }
}

/// Context for the IR → RustIR lowering pass.
pub struct LowerCtx<'a> {
    pub var_table: &'a VarTable,
    pub in_effect: bool,
    pub in_test: bool,
    #[allow(dead_code)]
    pub in_do_block: bool,
    pub effect_fns: &'a [String],
    pub result_fns: &'a [String],
    #[allow(dead_code)]
    pub current_fn_name: Option<String>,
    /// Maps variant constructor name → enum type name (e.g., "Red" → "Color")
    pub variant_ctors: &'a std::collections::HashMap<String, String>,
    /// Variables used only once — safe to move instead of clone
    pub single_use_vars: std::collections::HashSet<VarId>,
    /// Borrow inference results
    pub borrow_info: &'a super::borrow::BorrowInfo,
    /// Parameters that are currently borrowed (&str, &[T])
    pub borrowed_params: std::collections::HashSet<String>,
    /// Anonymous record field names → generated struct name (e.g., ["breed","name"] → "AlmdRec0")
    pub anon_records: &'a std::collections::HashMap<Vec<String>, String>,
    /// Named record field names → declared type name (e.g., ["x","y"] → "Point")
    pub named_record_types: &'a std::collections::HashMap<Vec<String>, String>,
    /// Open record params: fn_name → [(param_idx, struct_name, field_infos)]
    pub open_record_params: &'a std::collections::HashMap<String, Vec<(usize, String, Vec<super::OpenFieldInfo>)>>,
}

impl<'a> LowerCtx<'a> {
    /// Lower a type using the anonymous record maps.
    fn lower_ty(&self, ty: &Ty) -> RustType {
        lower_type_with_records(ty, self.anon_records, self.named_record_types)
    }

    /// Lower an IR expression to RustIR.
    pub fn lower_expr(&self, expr: &IrExpr) -> RustExpr {
        match &expr.kind {
            // ── Literals ──
            IrExprKind::LitInt { value } => RustExpr::IntLit(*value),
            IrExprKind::LitFloat { value } => RustExpr::FloatLit(*value),
            IrExprKind::LitStr { value } => RustExpr::StringLit(value.clone()),
            IrExprKind::LitBool { value } => RustExpr::BoolLit(*value),
            IrExprKind::Unit => RustExpr::Unit,

            // ── Variables ──
            IrExprKind::Var { id } => {
                let info = self.var_table.get(*id);
                let name = crate::emit_common::sanitize(&info.name);
                RustExpr::Var(name)
            }

            // ── Operators ──
            IrExprKind::BinOp { op, left, right } => {
                let rust_op = match op {
                    BinOp::AddInt | BinOp::AddFloat => RustBinOp::Add,
                    BinOp::SubInt | BinOp::SubFloat => RustBinOp::Sub,
                    BinOp::MulInt | BinOp::MulFloat => RustBinOp::Mul,
                    BinOp::DivInt | BinOp::DivFloat => RustBinOp::Div,
                    BinOp::ModInt | BinOp::ModFloat => RustBinOp::Mod,
                    BinOp::Eq => RustBinOp::Eq,
                    BinOp::Neq => RustBinOp::Neq,
                    BinOp::Lt => RustBinOp::Lt,
                    BinOp::Gt => RustBinOp::Gt,
                    BinOp::Lte => RustBinOp::Lte,
                    BinOp::Gte => RustBinOp::Gte,
                    BinOp::And => RustBinOp::And,
                    BinOp::Or => RustBinOp::Or,
                    BinOp::XorInt => RustBinOp::BitXor,
                    // String/List concat and pow handled specially
                    BinOp::ConcatStr => {
                        return RustExpr::Call {
                            func: "AlmideConcat::concat".to_string(),
                            args: vec![self.lower_arg(left), self.lower_arg(right)],
                        };
                    }
                    BinOp::ConcatList => {
                        return RustExpr::Call {
                            func: "AlmideConcat::concat".to_string(),
                            args: vec![self.lower_arg(left), self.lower_arg(right)],
                        };
                    }
                    BinOp::PowFloat => {
                        return RustExpr::MethodCall {
                            receiver: Box::new(self.lower_expr(left)),
                            method: "powf".to_string(),
                            args: vec![self.lower_expr(right)],
                        };
                    }
                };
                RustExpr::BinOp {
                    op: rust_op,
                    left: Box::new(self.lower_expr(left)),
                    right: Box::new(self.lower_expr(right)),
                }
            }

            IrExprKind::UnOp { op, operand } => {
                let rust_op = match op {
                    UnOp::Not => RustUnOp::Not,
                    UnOp::NegInt | UnOp::NegFloat => RustUnOp::Neg,
                };
                RustExpr::UnOp {
                    op: rust_op,
                    operand: Box::new(self.lower_expr(operand)),
                }
            }

            // ── Control flow ──
            IrExprKind::If { cond, then, else_ } => RustExpr::If {
                cond: Box::new(self.lower_expr(cond)),
                then: Box::new(self.lower_expr(then)),
                else_: Some(Box::new(self.lower_expr(else_))),
            },

            IrExprKind::Match { subject, arms } => RustExpr::Match {
                subject: Box::new(self.lower_expr(subject)),
                arms: arms.iter().map(|arm| RustMatchArm {
                    pattern: self.lower_pattern(&arm.pattern),
                    guard: arm.guard.as_ref().map(|g| self.lower_expr(g)),
                    body: self.lower_expr(&arm.body),
                }).collect(),
            },

            IrExprKind::Block { stmts, expr } => {
                let rust_stmts: Vec<RustStmt> = stmts.iter().map(|s| self.lower_stmt(s)).collect();
                let tail = expr.as_ref().map(|e| Box::new(self.lower_expr(e)));
                RustExpr::Block { stmts: rust_stmts, expr: tail }
            }

            IrExprKind::Break => RustExpr::Break,
            IrExprKind::Continue => RustExpr::Continue { label: None },

            // ── Result / Option ──
            IrExprKind::ResultOk { expr } => RustExpr::ResultOk(Box::new(self.lower_expr(expr))),
            IrExprKind::ResultErr { expr } => RustExpr::ResultErr(Box::new(self.lower_expr(expr))),
            IrExprKind::OptionSome { expr } => RustExpr::OptionSome(Box::new(self.lower_expr(expr))),
            IrExprKind::OptionNone => RustExpr::OptionNone,
            IrExprKind::Try { expr } => RustExpr::TryOp(Box::new(self.lower_expr(expr))),

            // ── Collections ──
            IrExprKind::List { elements } => {
                // List elements: clone vars (they're moved into the Vec)
                RustExpr::Vec(elements.iter().map(|e| self.lower_arg(e)).collect())
            }
            IrExprKind::Tuple { elements } => {
                RustExpr::Tuple(elements.iter().map(|e| self.lower_expr(e)).collect())
            }
            IrExprKind::EmptyMap => RustExpr::Raw("HashMap::new()".to_string()),
            IrExprKind::MapLiteral { entries } => {
                RustExpr::HashMap(entries.iter().map(|(k, v)| {
                    (self.lower_expr(k), self.lower_expr(v))
                }).collect())
            }

            // ── Access ──
            IrExprKind::Member { object, field } => {
                let obj = self.lower_expr(object);
                let field_expr = RustExpr::Field(Box::new(obj), field.clone());
                // Clone non-Copy field accesses
                if is_copy_ty(&expr.ty) {
                    field_expr
                } else {
                    RustExpr::Clone(Box::new(field_expr))
                }
            }
            IrExprKind::TupleIndex { object, index } => {
                RustExpr::TupleIndex(Box::new(self.lower_expr(object)), *index)
            }
            IrExprKind::IndexAccess { object, index } => {
                RustExpr::Index(Box::new(self.lower_expr(object)), Box::new(self.lower_expr(index)))
            }

            // ── Lambda ──
            IrExprKind::Lambda { params, body } => {
                let ps = params.iter().map(|(var, _ty)| {
                    RustParam {
                        name: self.var_table.get(*var).name.clone(),
                        ty: RustType::Infer,
                        mutable: false,
                    }
                }).collect();
                RustExpr::Closure {
                    params: ps,
                    body: Box::new(self.lower_expr(body)),
                }
            }

            // ── Strings ──
            IrExprKind::StringInterp { parts } => {
                let mut template = String::new();
                let mut args = Vec::new();
                for part in parts {
                    match part {
                        IrStringPart::Lit { value } => {
                            for c in value.chars() {
                                match c {
                                    '{' => template.push_str("{{"),
                                    '}' => template.push_str("}}"),
                                    '"' => template.push_str("\\\""),
                                    '\\' => template.push_str("\\\\"),
                                    _ => template.push(c),
                                }
                            }
                        }
                        IrStringPart::Expr { expr: e } => {
                            let use_debug = needs_debug_format(&e.ty);
                            if use_debug {
                                template.push_str("{:?}");
                            } else {
                                template.push_str("{}");
                            }
                            args.push(self.lower_expr(e));
                        }
                    }
                }
                if args.is_empty() {
                    RustExpr::StringLit(template)
                } else {
                    RustExpr::Format { template: format!("\"{}\"", template), args }
                }
            }

            // ── Records ──
            IrExprKind::Record { name, fields } => {
                let struct_name = if let Some(n) = name {
                    // Qualify variant record constructors: Shape::Circle { ... }
                    if let Some(enum_name) = self.variant_ctors.get(n) {
                        format!("{}::{}", enum_name, n)
                    } else {
                        n.clone()
                    }
                } else {
                    // Look up anonymous record struct name from field names
                    let mut field_names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                    field_names.sort();
                    self.anon_records.get(&field_names)
                        .cloned()
                        .unwrap_or_else(|| "AnonRecord".to_string())
                };
                RustExpr::StructInit {
                    name: struct_name,
                    fields: fields.iter().map(|(n, v)| (n.clone(), self.lower_arg(v))).collect(),
                }
            }
            IrExprKind::SpreadRecord { base, fields } => {
                RustExpr::StructUpdate {
                    base: Box::new(self.lower_expr(base)),
                    fields: fields.iter().map(|(n, v)| (n.clone(), self.lower_expr(v))).collect(),
                }
            }

            // ── Range ──
            IrExprKind::Range { start, end, inclusive } => {
                let elem_ty = match &expr.ty {
                    Ty::List(inner) => self.lower_ty(inner),
                    _ => self.lower_ty(&start.ty),
                };
                RustExpr::Range {
                    start: Box::new(self.lower_expr(start)),
                    end: Box::new(self.lower_expr(end)),
                    inclusive: *inclusive,
                    elem_ty,
                }
            }

            // ── Calls ──
            IrExprKind::Call { target, args, .. } => {
                match target {
                    CallTarget::Named { name } => {
                        self.lower_named_call(name, args)
                    }
                    CallTarget::Module { module, func } => {
                        // Render args as strings for the generated stdlib dispatch
                        let args_str: Vec<String> = args.iter().map(|a| {
                            let rust = self.lower_expr(a);
                            super::render::render_expr_to_string(&rust)
                        }).collect();
                        // Lambda inlining: extract param names and body from IR lambda
                        let var_table = self.var_table;
                        let this = &*self;
                        let inline_lambda = |idx: usize, _arity: usize| -> (Vec<String>, String) {
                            if let Some(arg) = args.get(idx) {
                                if let IrExprKind::Lambda { params, body } = &arg.kind {
                                    let names: Vec<String> = params.iter()
                                        .map(|(var, _)| var_table.get(*var).name.clone())
                                        .collect();
                                    let body_rust = this.lower_expr(body);
                                    let body_str = super::render::render_expr_to_string(&body_rust);
                                    return (names, body_str);
                                }
                            }
                            (vec!["_".to_string()], String::new())
                        };
                        if let Some(code) = almide::generated::emit_rust_calls::gen_generated_call(
                            module, func, &args_str, self.in_effect, &inline_lambda,
                        ) {
                            RustExpr::Raw(code)
                        } else {
                            RustExpr::Call {
                                func: format!("almide_rt_{}_{}", module.replace('.', "_"), func),
                                args: args.iter().map(|a| self.lower_expr(a)).collect(),
                            }
                        }
                    }
                    CallTarget::Method { object, method } => {
                        let obj = self.lower_expr(object);
                        let rest: Vec<RustExpr> = args.iter().map(|a| self.lower_expr(a)).collect();
                        // UFCS: receiver is first arg
                        let mut all_args = vec![obj];
                        all_args.extend(rest);
                        RustExpr::Call {
                            func: crate::emit_common::sanitize(method),
                            args: all_args,
                        }
                    }
                    CallTarget::Computed { callee } => {
                        let callee_expr = self.lower_expr(callee);
                        let arg_exprs: Vec<RustExpr> = args.iter().map(|a| self.lower_expr(a)).collect();
                        RustExpr::Raw(format!("({})({})",
                            super::render::render_expr_to_string(&callee_expr),
                            arg_exprs.iter().map(|a| super::render::render_expr_to_string(a)).collect::<Vec<_>>().join(", ")
                        ))
                    }
                }
            }

            // Remaining
            IrExprKind::DoBlock { stmts, expr } => {
                let has_guard = stmts.iter().any(|s| matches!(&s.kind, IrStmtKind::Guard { .. }));
                if has_guard {
                    // Do-block with guard → loop { if !cond { break/return }; body; }
                    let mut loop_stmts: Vec<RustStmt> = Vec::new();
                    for s in stmts {
                        loop_stmts.push(self.lower_do_block_stmt(s));
                    }
                    if let Some(e) = expr {
                        loop_stmts.push(RustStmt::Expr(self.lower_expr(e)));
                    }
                    RustExpr::Block {
                        stmts: vec![RustStmt::Expr(RustExpr::Loop { label: None, body: loop_stmts })],
                        expr: None,
                    }
                } else if self.in_effect && expr.is_some() {
                    // Do-block without guard in effect fn → block with Ok(final_expr)
                    let rust_stmts: Vec<RustStmt> = stmts.iter().map(|s| self.lower_stmt(s)).collect();
                    let tail = expr.as_ref().map(|e| Box::new(RustExpr::ResultOk(Box::new(self.lower_expr(e)))));
                    RustExpr::Block { stmts: rust_stmts, expr: tail }
                } else {
                    let rust_stmts: Vec<RustStmt> = stmts.iter().map(|s| self.lower_stmt(s)).collect();
                    let tail = expr.as_ref().map(|e| Box::new(self.lower_expr(e)));
                    RustExpr::Block { stmts: rust_stmts, expr: tail }
                }
            }
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                let binding = if let Some(tuple_vars) = var_tuple {
                    // Tuple destructuring: for (a, b) in ...
                    let names: Vec<String> = tuple_vars.iter()
                        .map(|v| self.var_table.get(*v).name.clone())
                        .collect();
                    format!("({})", names.join(", "))
                } else {
                    self.var_table.get(*var).name.clone()
                };
                // Clone iterable for iteration (Rust consumes iterators)
                let iter_expr = if let IrExprKind::Range { .. } = &iterable.kind {
                    self.lower_expr(iterable) // Range → native Rust range, no clone
                } else {
                    RustExpr::Clone(Box::new(self.lower_expr(iterable)))
                };
                RustExpr::For {
                    var: binding,
                    iter: Box::new(iter_expr),
                    body: body.iter().map(|s| self.lower_stmt(s)).collect(),
                }
            }
            IrExprKind::While { cond, body } => RustExpr::While {
                cond: Box::new(self.lower_expr(cond)),
                body: body.iter().map(|s| self.lower_stmt(s)).collect(),
            },
            IrExprKind::Await { expr } => self.lower_expr(expr), // TODO: async support
            IrExprKind::Hole | IrExprKind::Todo { .. } => {
                RustExpr::Raw("todo!()".to_string())
            }
        }
    }

    /// Lower an expression used as a function argument — adds clone for multi-use vars.
    fn lower_arg(&self, expr: &IrExpr) -> RustExpr {
        if let IrExprKind::Var { id } = &expr.kind {
            let info = self.var_table.get(*id);
            let name = crate::emit_common::sanitize(&info.name);
            if is_copy_ty(&info.ty) {
                return RustExpr::Var(name);
            }
            // Borrowed param: needs .to_owned() / .to_string() / .to_vec()
            if self.borrowed_params.contains(&info.name) {
                return RustExpr::ToOwned(Box::new(RustExpr::Var(name)));
            }
            if self.single_use_vars.contains(id) {
                return RustExpr::Var(name);
            }
            return RustExpr::Clone(Box::new(RustExpr::Var(name)));
        }
        self.lower_expr(expr)
    }

    /// Lower a named function call (builtins + user functions).
    fn lower_named_call(&self, name: &str, args: &[IrExpr]) -> RustExpr {
        match name {
            "println" => {
                let arg = self.lower_expr(&args[0]);
                RustExpr::MacroCall { name: "println".into(), args: vec![RustExpr::Raw("\"{}\"".into()), arg] }
            }
            "eprintln" => {
                let arg = self.lower_expr(&args[0]);
                RustExpr::MacroCall { name: "eprintln".into(), args: vec![RustExpr::Raw("\"{}\"".into()), arg] }
            }
            "assert_eq" => {
                let mut a = self.lower_expr(&args[0]);
                let mut b = self.lower_expr(&args[1]);
                // Empty list needs type annotation in assert_eq context
                if matches!(&args[0].kind, IrExprKind::List { elements } if elements.is_empty()) {
                    if let Ty::List(inner) = &args[1].ty {
                        let elem = super::render::render_type_to_string(&self.lower_ty(inner));
                        a = RustExpr::Raw(format!("Vec::<{}>::new()", elem));
                    }
                }
                if matches!(&args[1].kind, IrExprKind::List { elements } if elements.is_empty()) {
                    if let Ty::List(inner) = &args[0].ty {
                        let elem = super::render::render_type_to_string(&self.lower_ty(inner));
                        b = RustExpr::Raw(format!("Vec::<{}>::new()", elem));
                    }
                }
                RustExpr::MacroCall { name: "assert_eq".into(), args: vec![a, b] }
            }
            "assert_ne" => {
                let a = self.lower_expr(&args[0]);
                let b = self.lower_expr(&args[1]);
                RustExpr::MacroCall { name: "assert_ne".into(), args: vec![a, b] }
            }
            "assert" => {
                let a = self.lower_expr(&args[0]);
                RustExpr::MacroCall { name: "assert".into(), args: vec![a] }
            }
            "unwrap_or" if args.len() == 2 => {
                let a = self.lower_expr(&args[0]);
                let b = self.lower_expr(&args[1]);
                RustExpr::MethodCall { receiver: Box::new(a), method: "unwrap_or".into(), args: vec![b] }
            }
            _ => {
                // Check if this is a variant constructor
                if let Some(enum_name) = self.variant_ctors.get(name) {
                    let qualified = format!("{}::{}", enum_name, name);
                    if args.is_empty() {
                        // Unit variant
                        return RustExpr::Var(qualified);
                    }
                    return RustExpr::Call {
                        func: qualified,
                        args: args.iter().map(|a| self.lower_expr(a)).collect(),
                    };
                }
                // Regular function call — apply borrow and open record projection
                let callee_name = crate::emit_common::sanitize(name);
                let open_recs = self.open_record_params.get(name).cloned();
                let call = RustExpr::Call {
                    func: callee_name.clone(),
                    args: args.iter().enumerate().map(|(i, a)| {
                        // Check if this arg needs open record projection
                        if let Some(ref recs) = open_recs {
                            if let Some((_, struct_name, field_infos)) = recs.iter().find(|(idx, _, _)| *idx == i) {
                                return self.gen_open_record_projection(struct_name, field_infos, a);
                            }
                        }
                        if self.borrow_info.param_ownership(name, i) == super::borrow::ParamOwnership::Borrow {
                            let expr = self.lower_expr(a);
                            RustExpr::Borrow(Box::new(expr))
                        } else {
                            self.lower_arg(a)
                        }
                    }).collect(),
                };
                // Auto-? for effect function calls inside effect context
                if self.in_effect && !self.in_test
                    && (self.effect_fns.contains(&name.to_string()) || self.result_fns.contains(&name.to_string()))
                {
                    RustExpr::TryOp(Box::new(call))
                } else {
                    call
                }
            }
        }
    }

    /// Generate open record projection: extract required fields from argument into AlmdRec struct.
    fn gen_open_record_projection(&self, struct_name: &str, field_infos: &[super::OpenFieldInfo], arg: &IrExpr) -> RustExpr {
        let arg_expr = self.lower_expr(arg);
        let fields: Vec<(String, RustExpr)> = field_infos.iter().map(|fi| {
            if let Some((nested_struct, nested_infos)) = &fi.nested {
                // Nested open record: recursively project
                let inner = RustExpr::Field(Box::new(arg_expr.clone()), fi.name.clone());
                let nested_fields: Vec<(String, RustExpr)> = nested_infos.iter().map(|nfi| {
                    (nfi.name.clone(), RustExpr::Clone(Box::new(RustExpr::Field(Box::new(inner.clone()), nfi.name.clone()))))
                }).collect();
                (fi.name.clone(), RustExpr::StructInit { name: nested_struct.clone(), fields: nested_fields })
            } else {
                (fi.name.clone(), RustExpr::Clone(Box::new(RustExpr::Field(Box::new(arg_expr.clone()), fi.name.clone()))))
            }
        }).collect();
        RustExpr::StructInit { name: struct_name.to_string(), fields }
    }

    /// Lower an IR statement to RustIR.
    pub fn lower_stmt(&self, stmt: &IrStmt) -> RustStmt {
        match &stmt.kind {
            IrStmtKind::Bind { var, mutability, ty, value } => {
                let info = self.var_table.get(*var);
                // Add type annotation for values that Rust can't infer
                let needs_annotation = matches!(&value.kind,
                    IrExprKind::List { elements } if elements.is_empty()
                ) || matches!(&value.kind, IrExprKind::EmptyMap)
                  || matches!(&value.kind, IrExprKind::OptionNone)
                  // Ok/Err only when ty is a clean Result (no Unknown inside)
                  || (matches!(&value.kind, IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. })
                      && matches!(ty, Ty::Result(_, _)) && !ty.contains_unknown())
                  || (matches!(&value.kind, IrExprKind::OptionSome { .. })
                      && matches!(ty, Ty::Option(inner) if matches!(inner.as_ref(), Ty::Result(_, _)) && !inner.contains_unknown()));
                let type_ann = if needs_annotation && !matches!(ty, Ty::Unknown) {
                    Some(self.lower_ty(ty))
                } else {
                    None
                };
                RustStmt::Let {
                    name: crate::emit_common::sanitize(&info.name),
                    ty: type_ann,
                    mutable: matches!(mutability, Mutability::Var),
                    value: self.lower_expr(value),
                }
            }
            IrStmtKind::Assign { var, value } => {
                let name = crate::emit_common::sanitize(&self.var_table.get(*var).name);
                RustStmt::Assign { target: name, value: self.lower_expr(value) }
            }
            IrStmtKind::IndexAssign { target, index, value } => {
                let name = crate::emit_common::sanitize(&self.var_table.get(*target).name);
                RustStmt::IndexAssign {
                    target: name,
                    index: self.lower_expr(index),
                    value: self.lower_expr(value),
                }
            }
            IrStmtKind::FieldAssign { target, field, value } => {
                let name = crate::emit_common::sanitize(&self.var_table.get(*target).name);
                RustStmt::FieldAssign {
                    target: name,
                    field: field.clone(),
                    value: self.lower_expr(value),
                }
            }
            IrStmtKind::Expr { expr } => RustStmt::Expr(self.lower_expr(expr)),
            IrStmtKind::Guard { cond, else_ } => {
                // Guard → if !cond { return else_; }
                RustStmt::Expr(RustExpr::If {
                    cond: Box::new(RustExpr::UnOp {
                        op: RustUnOp::Not,
                        operand: Box::new(self.lower_expr(cond)),
                    }),
                    then: Box::new(RustExpr::Return(Some(Box::new(self.lower_expr(else_))))),
                    else_: None,
                })
            }
            IrStmtKind::BindDestructure { pattern, value } => {
                RustStmt::LetPattern {
                    pattern: self.lower_pattern(pattern),
                    value: self.lower_expr(value),
                }
            }
            IrStmtKind::Comment { text } => RustStmt::Comment(text.clone()),
        }
    }

    /// Lower a statement inside a do-block (guard ↦ break/continue/return instead of return-wrap)
    fn lower_do_block_stmt(&self, stmt: &IrStmt) -> RustStmt {
        match &stmt.kind {
            IrStmtKind::Guard { cond, else_ } => {
                let else_expr = match &else_.kind {
                    IrExprKind::Break => RustExpr::Break,
                    IrExprKind::Continue => RustExpr::Continue { label: None },
                    IrExprKind::Unit => RustExpr::Break,
                    IrExprKind::ResultOk { expr } if matches!(&expr.kind, IrExprKind::Unit) => RustExpr::Break,
                    IrExprKind::ResultErr { .. } => {
                        if self.in_effect {
                            RustExpr::Return(Some(Box::new(self.lower_expr(else_))))
                        } else {
                            RustExpr::Return(Some(Box::new(self.lower_expr(else_))))
                        }
                    }
                    _ => RustExpr::Return(Some(Box::new(self.lower_expr(else_)))),
                };
                RustStmt::Expr(RustExpr::If {
                    cond: Box::new(RustExpr::UnOp {
                        op: RustUnOp::Not,
                        operand: Box::new(self.lower_expr(cond)),
                    }),
                    then: Box::new(else_expr),
                    else_: None,
                })
            }
            _ => self.lower_stmt(stmt),
        }
    }

    /// Lower an expression in TCO context — tail self-calls become param reassignment + continue.
    pub fn lower_tco_expr(&self, expr: &IrExpr, fn_name: &str, params: &[String]) -> RustExpr {
        match &expr.kind {
            // Tail self-call → reassign params + continue
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if name == fn_name => {
                let mut stmts = Vec::new();
                for (i, arg) in args.iter().enumerate() {
                    let tmp = self.lower_arg(arg);
                    stmts.push(RustStmt::Let {
                        name: format!("_tco_tmp_{}", i),
                        ty: None,
                        mutable: false,
                        value: tmp,
                    });
                }
                for (i, param) in params.iter().enumerate() {
                    stmts.push(RustStmt::Assign {
                        target: param.clone(),
                        value: RustExpr::Var(format!("_tco_tmp_{}", i)),
                    });
                }
                stmts.push(RustStmt::Expr(RustExpr::Continue { label: Some("_tco".to_string()) }));
                RustExpr::Block { stmts, expr: None }
            }
            IrExprKind::If { cond, then, else_ } => {
                RustExpr::If {
                    cond: Box::new(self.lower_expr(cond)),
                    then: Box::new(self.lower_tco_expr(then, fn_name, params)),
                    else_: Some(Box::new(self.lower_tco_expr(else_, fn_name, params))),
                }
            }
            IrExprKind::Match { subject, arms } => {
                RustExpr::Match {
                    subject: Box::new(self.lower_expr(subject)),
                    arms: arms.iter().map(|arm| RustMatchArm {
                        pattern: self.lower_pattern(&arm.pattern),
                        guard: arm.guard.as_ref().map(|g| self.lower_expr(g)),
                        body: self.lower_tco_expr(&arm.body, fn_name, params),
                    }).collect(),
                }
            }
            IrExprKind::Block { stmts, expr: final_expr } => {
                let rust_stmts: Vec<RustStmt> = stmts.iter().map(|s| self.lower_stmt(s)).collect();
                let tail = final_expr.as_ref().map(|e| {
                    Box::new(self.lower_tco_expr(e, fn_name, params))
                });
                RustExpr::Block { stmts: rust_stmts, expr: tail }
            }
            // Non-tail: return the value
            _ => {
                let e = self.lower_expr(expr);
                RustExpr::Return(Some(Box::new(e)))
            }
        }
    }

    /// Lower an IR pattern to RustIR.
    pub fn lower_pattern(&self, pat: &IrPattern) -> RustPattern {
        match pat {
            IrPattern::Wildcard => RustPattern::Wildcard,
            IrPattern::Bind { var } => {
                RustPattern::Var(self.var_table.get(*var).name.clone())
            }
            IrPattern::Literal { expr } => RustPattern::Literal(self.lower_expr(expr)),
            IrPattern::Constructor { name, args } => {
                let qualified = if let Some(enum_name) = self.variant_ctors.get(name) {
                    format!("{}::{}", enum_name, name)
                } else {
                    name.clone()
                };
                RustPattern::Constructor {
                    name: qualified,
                    args: args.iter().map(|a| self.lower_pattern(a)).collect(),
                }
            }
            IrPattern::RecordPattern { name, fields, rest } => RustPattern::Struct {
                name: self.variant_ctors.get(name)
                    .map(|enum_name| format!("{}::{}", enum_name, name))
                    .unwrap_or_else(|| name.clone()),
                fields: fields.iter().map(|f| {
                    (f.name.clone(), f.pattern.as_ref().map(|p| self.lower_pattern(p)))
                }).collect(),
                rest: *rest,
            },
            IrPattern::Tuple { elements } => {
                RustPattern::Tuple(elements.iter().map(|e| self.lower_pattern(e)).collect())
            }
            IrPattern::Some { inner } => RustPattern::Constructor {
                name: "Some".to_string(),
                args: vec![self.lower_pattern(inner)],
            },
            IrPattern::None => RustPattern::Var("None".to_string()),
            IrPattern::Ok { inner } => RustPattern::Constructor {
                name: "Ok".to_string(),
                args: vec![self.lower_pattern(inner)],
            },
            IrPattern::Err { inner } => RustPattern::Constructor {
                name: "Err".to_string(),
                args: vec![self.lower_pattern(inner)],
            },
        }
    }
}

/// Check if a type is Copy (no clone needed).
fn is_copy_ty(ty: &Ty) -> bool {
    match ty {
        Ty::Int | Ty::Float | Ty::Bool | Ty::Unit => true,
        Ty::Option(inner) => is_copy_ty(inner),
        Ty::Tuple(elems) => elems.iter().all(is_copy_ty),
        _ => false,
    }
}

/// Compute the set of single-use variables from the VarTable use counts.
fn compute_single_use(var_table: &VarTable, params: &[IrParam]) -> std::collections::HashSet<VarId> {
    let mut result = std::collections::HashSet::new();
    // Exclude params — they shouldn't be moved
    let param_ids: std::collections::HashSet<VarId> = params.iter().map(|p| p.var).collect();
    for i in 0..var_table.len() {
        let id = VarId(i as u32);
        if param_ids.contains(&id) { continue; }
        if var_table.use_count(id) <= 1 {
            result.insert(id);
        }
    }
    result
}

/// Lower an Almide Ty to RustType (without anonymous record resolution).
/// For code that doesn't have access to anon_records maps (e.g., type decl lowering
/// where the fields are already known from the declaration).
pub fn lower_type(ty: &Ty) -> RustType {
    lower_type_with_records(ty, &std::collections::HashMap::new(), &std::collections::HashMap::new())
}

/// Lower an Almide Ty to RustType, resolving anonymous records via the provided maps.
pub fn lower_type_with_records(
    ty: &Ty,
    anon_records: &std::collections::HashMap<Vec<String>, String>,
    named_record_types: &std::collections::HashMap<Vec<String>, String>,
) -> RustType {
    match ty {
        Ty::Int => RustType::I64,
        Ty::Float => RustType::F64,
        Ty::Bool => RustType::Bool,
        Ty::String => RustType::String,
        Ty::Unit => RustType::Unit,
        Ty::List(inner) => RustType::Vec(Box::new(lower_type_with_records(inner, anon_records, named_record_types))),
        Ty::Option(inner) => RustType::Option(Box::new(lower_type_with_records(inner, anon_records, named_record_types))),
        Ty::Result(ok, err) => RustType::Result(
            Box::new(lower_type_with_records(ok, anon_records, named_record_types)),
            Box::new(lower_type_with_records(err, anon_records, named_record_types)),
        ),
        Ty::Map(k, v) => RustType::HashMap(
            Box::new(lower_type_with_records(k, anon_records, named_record_types)),
            Box::new(lower_type_with_records(v, anon_records, named_record_types)),
        ),
        Ty::Tuple(elems) => RustType::Tuple(elems.iter().map(|e| lower_type_with_records(e, anon_records, named_record_types)).collect()),
        Ty::Named(name, args) if args.is_empty() => RustType::Named(name.clone()),
        Ty::Named(name, args) => RustType::Generic(name.clone(), args.iter().map(|a| lower_type_with_records(a, anon_records, named_record_types)).collect()),
        Ty::Fn { params, ret } => RustType::Fn(
            params.iter().map(|p| lower_type_with_records(p, anon_records, named_record_types)).collect(),
            Box::new(lower_type_with_records(ret, anon_records, named_record_types)),
        ),
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            let mut field_names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            field_names.sort();
            // Check named record types first
            if let Some(name) = named_record_types.get(&field_names) {
                return RustType::Named(name.clone());
            }
            // Check anonymous record structs
            if let Some(name) = anon_records.get(&field_names) {
                // Build type args from concrete field types
                let type_args: Vec<RustType> = fields.iter().map(|(_, t)| {
                    lower_type_with_records(t, anon_records, named_record_types)
                }).collect();
                if type_args.is_empty() {
                    RustType::Named(name.clone())
                } else {
                    RustType::Generic(name.clone(), type_args)
                }
            } else {
                RustType::Named("AnonRecord".to_string())
            }
        }
        Ty::TypeVar(name) => RustType::Named(name.clone()),
        Ty::Union(members) => {
            // Generate canonical enum name from member types
            let names: Vec<String> = members.iter().map(|m| m.display().replace(" ", "")).collect();
            RustType::Named(format!("AlmideUnion_{}", names.join("_")))
        }
        Ty::Variant { name, .. } => RustType::Named(name.clone()),
        Ty::Unknown => RustType::Infer,
    }
}

/// Check if a type needs Debug formatting in string interpolation.
fn needs_debug_format(ty: &Ty) -> bool {
    match ty {
        Ty::List(_) | Ty::Option(_) | Ty::Result(_, _) |
        Ty::Map(_, _) | Ty::Tuple(_) | Ty::Record { .. } | Ty::OpenRecord { .. } |
        Ty::Variant { .. } => true,
        Ty::Named(name, _) => !matches!(name.as_str(), "String" | "Int" | "Float" | "Bool"),
        _ => false,
    }
}
