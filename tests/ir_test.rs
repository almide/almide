use almide::ir::*;
use almide::types::Ty;
use almide::ast::Span;

// ---- VarTable ----

#[test]
fn var_table_alloc_and_get() {
    let mut vt = VarTable::new();
    let id = vt.alloc("x".into(), Ty::Int, Mutability::Let, None);
    assert_eq!(id, VarId(0));
    let info = vt.get(id);
    assert_eq!(info.name, "x");
    assert_eq!(info.ty, Ty::Int);
    assert_eq!(info.mutability, Mutability::Let);
    assert!(info.span.is_none());
}

#[test]
fn var_table_multiple_vars() {
    let mut vt = VarTable::new();
    let id0 = vt.alloc("a".into(), Ty::Int, Mutability::Let, None);
    let id1 = vt.alloc("b".into(), Ty::String, Mutability::Var, Some(Span { line: 5, col: 3, end_col: 4 }));
    let id2 = vt.alloc("c".into(), Ty::Bool, Mutability::Let, None);
    assert_eq!(id0, VarId(0));
    assert_eq!(id1, VarId(1));
    assert_eq!(id2, VarId(2));
    assert_eq!(vt.len(), 3);
    assert_eq!(vt.get(id1).name, "b");
    assert_eq!(vt.get(id1).mutability, Mutability::Var);
    assert!(vt.get(id1).span.is_some());
}

#[test]
fn var_table_len() {
    let mut vt = VarTable::new();
    assert_eq!(vt.len(), 0);
    vt.alloc("x".into(), Ty::Int, Mutability::Let, None);
    assert_eq!(vt.len(), 1);
    vt.alloc("y".into(), Ty::Int, Mutability::Let, None);
    assert_eq!(vt.len(), 2);
}

#[test]
fn var_table_default() {
    let vt = VarTable::default();
    assert_eq!(vt.len(), 0);
}

// ---- VarId ----

#[test]
fn var_id_equality() {
    assert_eq!(VarId(0), VarId(0));
    assert_ne!(VarId(0), VarId(1));
}

#[test]
fn var_id_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(VarId(0));
    set.insert(VarId(1));
    set.insert(VarId(0)); // duplicate
    assert_eq!(set.len(), 2);
}

// ---- Mutability ----

#[test]
fn mutability_equality() {
    assert_eq!(Mutability::Let, Mutability::Let);
    assert_eq!(Mutability::Var, Mutability::Var);
    assert_ne!(Mutability::Let, Mutability::Var);
}

// ---- BinOp / UnOp ----

#[test]
fn binop_equality() {
    assert_eq!(BinOp::AddInt, BinOp::AddInt);
    assert_ne!(BinOp::AddInt, BinOp::AddFloat);
    assert_ne!(BinOp::Eq, BinOp::Neq);
}

#[test]
fn unop_equality() {
    assert_eq!(UnOp::NegInt, UnOp::NegInt);
    assert_ne!(UnOp::NegInt, UnOp::Not);
}

// ---- IrExpr construction ----

#[test]
fn ir_expr_lit_int() {
    let expr = IrExpr {
        kind: IrExprKind::LitInt { value: 42 },
        ty: Ty::Int,
        span: Some(Span { line: 1, col: 1, end_col: 2 }),
    };
    assert_eq!(expr.ty, Ty::Int);
    assert!(matches!(expr.kind, IrExprKind::LitInt { value: 42 }));
}

#[test]
fn ir_expr_lit_float() {
    let expr = IrExpr {
        kind: IrExprKind::LitFloat { value: 3.14 },
        ty: Ty::Float,
        span: None,
    };
    assert_eq!(expr.ty, Ty::Float);
}

#[test]
fn ir_expr_lit_str() {
    let expr = IrExpr {
        kind: IrExprKind::LitStr { value: "hello".into() },
        ty: Ty::String,
        span: None,
    };
    assert_eq!(expr.ty, Ty::String);
}

#[test]
fn ir_expr_lit_bool() {
    let expr = IrExpr {
        kind: IrExprKind::LitBool { value: true },
        ty: Ty::Bool,
        span: None,
    };
    assert_eq!(expr.ty, Ty::Bool);
}

#[test]
fn ir_expr_unit() {
    let expr = IrExpr {
        kind: IrExprKind::Unit,
        ty: Ty::Unit,
        span: None,
    };
    assert_eq!(expr.ty, Ty::Unit);
}

#[test]
fn ir_expr_var() {
    let expr = IrExpr {
        kind: IrExprKind::Var { id: VarId(5) },
        ty: Ty::Int,
        span: None,
    };
    assert!(matches!(expr.kind, IrExprKind::Var { id: VarId(5) }));
}

#[test]
fn ir_expr_binop() {
    let left = Box::new(IrExpr { kind: IrExprKind::LitInt { value: 1 }, ty: Ty::Int, span: None });
    let right = Box::new(IrExpr { kind: IrExprKind::LitInt { value: 2 }, ty: Ty::Int, span: None });
    let expr = IrExpr {
        kind: IrExprKind::BinOp { op: BinOp::AddInt, left, right },
        ty: Ty::Int,
        span: None,
    };
    assert!(matches!(expr.kind, IrExprKind::BinOp { op: BinOp::AddInt, .. }));
}

#[test]
fn ir_expr_unop() {
    let operand = Box::new(IrExpr { kind: IrExprKind::LitBool { value: true }, ty: Ty::Bool, span: None });
    let expr = IrExpr {
        kind: IrExprKind::UnOp { op: UnOp::Not, operand },
        ty: Ty::Bool,
        span: None,
    };
    assert!(matches!(expr.kind, IrExprKind::UnOp { op: UnOp::Not, .. }));
}

// ---- IrPattern ----

#[test]
fn ir_pattern_wildcard() {
    let p = IrPattern::Wildcard;
    assert!(matches!(p, IrPattern::Wildcard));
}

#[test]
fn ir_pattern_bind() {
    let p = IrPattern::Bind { var: VarId(0), ty: Ty::Unknown };
    assert!(matches!(p, IrPattern::Bind { var: VarId(0), ty: Ty::Unknown }));
}

#[test]
fn ir_pattern_constructor() {
    let p = IrPattern::Constructor {
        name: "Some".into(),
        args: vec![IrPattern::Bind { var: VarId(1), ty: Ty::Unknown }],
    };
    if let IrPattern::Constructor { name, args } = &p {
        assert_eq!(name, "Some");
        assert_eq!(args.len(), 1);
    } else {
        panic!("expected Constructor pattern");
    }
}

#[test]
fn ir_pattern_tuple() {
    let p = IrPattern::Tuple {
        elements: vec![
            IrPattern::Bind { var: VarId(0), ty: Ty::Unknown },
            IrPattern::Bind { var: VarId(1), ty: Ty::Unknown },
        ],
    };
    if let IrPattern::Tuple { elements } = &p {
        assert_eq!(elements.len(), 2);
    } else {
        panic!("expected Tuple pattern");
    }
}

#[test]
fn ir_pattern_some_none() {
    let some = IrPattern::Some { inner: Box::new(IrPattern::Bind { var: VarId(0), ty: Ty::Unknown }) };
    assert!(matches!(some, IrPattern::Some { .. }));
    let none = IrPattern::None;
    assert!(matches!(none, IrPattern::None));
}

#[test]
fn ir_pattern_ok_err() {
    let ok = IrPattern::Ok { inner: Box::new(IrPattern::Bind { var: VarId(0), ty: Ty::Unknown }) };
    assert!(matches!(ok, IrPattern::Ok { .. }));
    let err = IrPattern::Err { inner: Box::new(IrPattern::Bind { var: VarId(1), ty: Ty::Unknown }) };
    assert!(matches!(err, IrPattern::Err { .. }));
}

// ---- CallTarget ----

#[test]
fn call_target_named() {
    let t = CallTarget::Named { name: "println".into() };
    if let CallTarget::Named { name } = &t {
        assert_eq!(name, "println");
    }
}

#[test]
fn call_target_module() {
    let t = CallTarget::Module { module: "string".into(), func: "trim".into() };
    if let CallTarget::Module { module, func } = &t {
        assert_eq!(module, "string");
        assert_eq!(func, "trim");
    }
}

// ---- IrStmt ----

#[test]
fn ir_stmt_bind() {
    let stmt = IrStmt {
        kind: IrStmtKind::Bind {
            var: VarId(0),
            mutability: Mutability::Let,
            ty: Ty::Int,
            value: IrExpr { kind: IrExprKind::LitInt { value: 42 }, ty: Ty::Int, span: None },
        },
        span: None,
    };
    assert!(matches!(stmt.kind, IrStmtKind::Bind { .. }));
}

#[test]
fn ir_stmt_assign() {
    let stmt = IrStmt {
        kind: IrStmtKind::Assign {
            var: VarId(0),
            value: IrExpr { kind: IrExprKind::LitInt { value: 1 }, ty: Ty::Int, span: None },
        },
        span: None,
    };
    assert!(matches!(stmt.kind, IrStmtKind::Assign { .. }));
}

#[test]
fn ir_stmt_expr() {
    let stmt = IrStmt {
        kind: IrStmtKind::Expr {
            expr: IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None },
        },
        span: None,
    };
    assert!(matches!(stmt.kind, IrStmtKind::Expr { .. }));
}

#[test]
fn ir_stmt_comment() {
    let stmt = IrStmt {
        kind: IrStmtKind::Comment { text: "// hello".into() },
        span: None,
    };
    assert!(matches!(stmt.kind, IrStmtKind::Comment { .. }));
}

// ---- IrFunction ----

#[test]
fn ir_function_construction() {
    let f = IrFunction {
        name: "add".into(),
        params: vec![
            IrParam { var: VarId(0), ty: Ty::Int, name: "a".into(), borrow: ParamBorrow::Own, open_record: None, default: None },
            IrParam { var: VarId(1), ty: Ty::Int, name: "b".into(), borrow: ParamBorrow::Own, open_record: None, default: None },
        ],
        ret_ty: Ty::Int,
        body: IrExpr { kind: IrExprKind::LitInt { value: 0 }, ty: Ty::Int, span: None },
        is_effect: false,
        is_async: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        visibility: IrVisibility::Public,
        doc: None,
        blank_lines_before: 0,
    };
    assert_eq!(f.name, "add");
    assert_eq!(f.params.len(), 2);
    assert!(!f.is_effect);
    assert!(!f.is_async);
    assert!(!f.is_test);
}

#[test]
fn ir_function_effect() {
    let f = IrFunction {
        name: "main".into(),
        params: vec![],
        ret_ty: Ty::result(Ty::Unit, Ty::String),
        body: IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None },
        is_effect: true,
        is_async: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        visibility: IrVisibility::Public,
        doc: None,
        blank_lines_before: 0,
    };
    assert!(f.is_effect);
}

// ---- IrProgram ----

#[test]
fn ir_program_construction() {
    let prog = IrProgram {
        functions: vec![],
        top_lets: vec![],
        type_decls: vec![],
        var_table: VarTable::new(),
        modules: vec![],
        type_registry: Default::default(),
        effect_fn_names: Default::default(),
        effect_map: Default::default(),
        codegen_annotations: Default::default(),
    };
    assert!(prog.functions.is_empty());
    assert!(prog.top_lets.is_empty());
    assert_eq!(prog.var_table.len(), 0);
}

// ---- IrStringPart ----

#[test]
fn ir_string_part_lit() {
    let part = IrStringPart::Lit { value: "hello".into() };
    assert!(matches!(part, IrStringPart::Lit { value } if value == "hello"));
}

#[test]
fn ir_string_part_expr() {
    let part = IrStringPart::Expr {
        expr: IrExpr { kind: IrExprKind::Var { id: VarId(0) }, ty: Ty::String, span: None },
    };
    assert!(matches!(part, IrStringPart::Expr { .. }));
}

// ---- IrMatchArm ----

#[test]
fn ir_match_arm() {
    let arm = IrMatchArm {
        pattern: IrPattern::Wildcard,
        guard: None,
        body: IrExpr { kind: IrExprKind::LitInt { value: 0 }, ty: Ty::Int, span: None },
    };
    assert!(arm.guard.is_none());
    assert!(matches!(arm.pattern, IrPattern::Wildcard));
}

#[test]
fn ir_match_arm_with_guard() {
    let arm = IrMatchArm {
        pattern: IrPattern::Bind { var: VarId(0), ty: Ty::Unknown },
        guard: Some(IrExpr { kind: IrExprKind::LitBool { value: true }, ty: Ty::Bool, span: None }),
        body: IrExpr { kind: IrExprKind::LitInt { value: 1 }, ty: Ty::Int, span: None },
    };
    assert!(arm.guard.is_some());
}

// ---- IrExprKind variants ----

#[test]
fn ir_expr_list() {
    let expr = IrExpr {
        kind: IrExprKind::List { elements: vec![
            IrExpr { kind: IrExprKind::LitInt { value: 1 }, ty: Ty::Int, span: None },
            IrExpr { kind: IrExprKind::LitInt { value: 2 }, ty: Ty::Int, span: None },
        ]},
        ty: Ty::list(Ty::Int),
        span: None,
    };
    if let IrExprKind::List { elements } = &expr.kind {
        assert_eq!(elements.len(), 2);
    }
}

#[test]
fn ir_expr_tuple() {
    let expr = IrExpr {
        kind: IrExprKind::Tuple { elements: vec![
            IrExpr { kind: IrExprKind::LitInt { value: 1 }, ty: Ty::Int, span: None },
            IrExpr { kind: IrExprKind::LitStr { value: "a".into() }, ty: Ty::String, span: None },
        ]},
        ty: Ty::Tuple(vec![Ty::Int, Ty::String]),
        span: None,
    };
    if let IrExprKind::Tuple { elements } = &expr.kind {
        assert_eq!(elements.len(), 2);
    }
}

#[test]
fn ir_expr_range() {
    let expr = IrExpr {
        kind: IrExprKind::Range {
            start: Box::new(IrExpr { kind: IrExprKind::LitInt { value: 0 }, ty: Ty::Int, span: None }),
            end: Box::new(IrExpr { kind: IrExprKind::LitInt { value: 10 }, ty: Ty::Int, span: None }),
            inclusive: false,
        },
        ty: Ty::list(Ty::Int),
        span: None,
    };
    assert!(matches!(expr.kind, IrExprKind::Range { inclusive: false, .. }));
}

#[test]
fn ir_expr_todo() {
    let expr = IrExpr {
        kind: IrExprKind::Todo { message: "not implemented".into() },
        ty: Ty::Unknown,
        span: None,
    };
    assert!(matches!(expr.kind, IrExprKind::Todo { .. }));
}

#[test]
fn ir_expr_hole() {
    let expr = IrExpr {
        kind: IrExprKind::Hole,
        ty: Ty::Unknown,
        span: None,
    };
    assert!(matches!(expr.kind, IrExprKind::Hole));
}

// ---- collect_unused_var_warnings ----

fn make_program_with_vars(vars: Vec<(&str, Option<Span>, bool)>) -> IrProgram {
    let mut var_table = VarTable::new();
    let mut stmts = Vec::new();
    let mut param_vars = Vec::new();

    for (name, span, is_param) in &vars {
        let id = var_table.alloc(name.to_string().into(), Ty::Int, Mutability::Let, *span);
        if *is_param {
            param_vars.push(id);
        } else {
            stmts.push(IrStmt {
                kind: IrStmtKind::Bind {
                    var: id,
                    mutability: Mutability::Let,
                    ty: Ty::Int,
                    value: IrExpr { kind: IrExprKind::LitInt { value: 0 }, ty: Ty::Int, span: None },
                },
                span: *span,
            });
        }
    }

    let params: Vec<IrParam> = param_vars.iter().map(|&id| {
        let info = var_table.get(id);
        IrParam {
            var: id,
            ty: Ty::Int,
            name: info.name.clone(),
            borrow: ParamBorrow::Own,
            open_record: None,
            default: None,
        }
    }).collect();

    let body = IrExpr {
        kind: IrExprKind::Block { stmts, expr: Some(Box::new(IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None })) },
        ty: Ty::Unit,
        span: None,
    };

    IrProgram {
        functions: vec![IrFunction {
            name: "test_fn".into(),
            params,
            ret_ty: Ty::Unit,
            body,
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
        }],
        top_lets: vec![],
        type_decls: vec![],
        var_table,
        modules: vec![],
        type_registry: Default::default(),
        effect_fn_names: Default::default(),
        effect_map: Default::default(),
        codegen_annotations: Default::default(),
    }
}

#[test]
fn unused_var_warning_basic() {
    let mut prog = make_program_with_vars(vec![
        ("x", Some(Span { line: 3, col: 7, end_col: 8 }), false),
    ]);
    compute_use_counts(&mut prog);
    let warnings = collect_unused_var_warnings(&prog, "test.almd");
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("unused variable 'x'"));
    assert!(warnings[0].hint.contains("_x"));
}

#[test]
fn unused_var_warning_underscore_suppressed() {
    let mut prog = make_program_with_vars(vec![
        ("_x", Some(Span { line: 3, col: 7, end_col: 8 }), false),
    ]);
    compute_use_counts(&mut prog);
    let warnings = collect_unused_var_warnings(&prog, "test.almd");
    assert_eq!(warnings.len(), 0);
}

#[test]
fn unused_var_warning_used_var_no_warning() {
    let mut var_table = VarTable::new();
    let x_id = var_table.alloc("x".into(), Ty::Int, Mutability::Let, Some(Span { line: 2, col: 7, end_col: 8 }));

    let bind_stmt = IrStmt {
        kind: IrStmtKind::Bind {
            var: x_id,
            mutability: Mutability::Let,
            ty: Ty::Int,
            value: IrExpr { kind: IrExprKind::LitInt { value: 42 }, ty: Ty::Int, span: None },
        },
        span: Some(Span { line: 2, col: 7, end_col: 8 }),
    };
    let use_expr = IrExpr {
        kind: IrExprKind::Var { id: x_id },
        ty: Ty::Int,
        span: None,
    };
    let body = IrExpr {
        kind: IrExprKind::Block {
            stmts: vec![bind_stmt],
            expr: Some(Box::new(use_expr)),
        },
        ty: Ty::Int,
        span: None,
    };

    let mut prog = IrProgram {
        functions: vec![IrFunction {
            name: "test_fn".into(),
            params: vec![],
            ret_ty: Ty::Int,
            body,
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
        }],
        top_lets: vec![],
        type_decls: vec![],
        var_table,
        modules: vec![],
        type_registry: Default::default(),
        effect_fn_names: Default::default(),
        effect_map: Default::default(),
        codegen_annotations: Default::default(),
    };
    compute_use_counts(&mut prog);
    let warnings = collect_unused_var_warnings(&prog, "test.almd");
    assert_eq!(warnings.len(), 0);
}

#[test]
fn unused_var_warning_param_excluded() {
    let mut prog = make_program_with_vars(vec![
        ("arg", Some(Span { line: 1, col: 10, end_col: 11 }), true),
    ]);
    compute_use_counts(&mut prog);
    let warnings = collect_unused_var_warnings(&prog, "test.almd");
    assert_eq!(warnings.len(), 0);
}

#[test]
fn unused_var_warning_no_span_excluded() {
    let mut prog = make_program_with_vars(vec![
        ("x", None, false),
    ]);
    compute_use_counts(&mut prog);
    let warnings = collect_unused_var_warnings(&prog, "test.almd");
    assert_eq!(warnings.len(), 0);
}

#[test]
fn unused_var_warning_multiple() {
    let mut prog = make_program_with_vars(vec![
        ("a", Some(Span { line: 2, col: 7, end_col: 8 }), false),
        ("_b", Some(Span { line: 3, col: 7, end_col: 8 }), false),
        ("c", Some(Span { line: 4, col: 7, end_col: 8 }), false),
        ("param", Some(Span { line: 1, col: 10, end_col: 11 }), true),
    ]);
    compute_use_counts(&mut prog);
    let warnings = collect_unused_var_warnings(&prog, "test.almd");
    assert_eq!(warnings.len(), 2);
    let names: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(names.iter().any(|m| m.contains("'a'")));
    assert!(names.iter().any(|m| m.contains("'c'")));
}
