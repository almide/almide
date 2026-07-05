// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_program(functions: Vec<IrFunction>, var_table: VarTable) -> IrProgram {
        IrProgram {
            functions,
            top_lets: vec![],
            type_decls: vec![],
            var_table,
            def_table: Default::default(),
            modules: vec![],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
            used_stdlib_modules: Default::default(),
        }
    }

    fn lit_int(v: i64) -> IrExpr {
        IrExpr { kind: IrExprKind::LitInt { value: v }, ty: Ty::Int, span: None, def_id: None }
    }

    fn var_expr(id: VarId, ty: Ty) -> IrExpr {
        IrExpr { kind: IrExprKind::Var { id }, ty, span: None, def_id: None }
    }

    fn make_fn(name: &str, body: IrExpr) -> IrFunction {
        IrFunction {
            name: name.into(),
            params: vec![],
            ret_ty: body.ty.clone(),
            body,
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![], attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![],
            module_origin: None,
        }
    }

    #[test]
    fn valid_program_no_errors() {
        let mut vt = VarTable::new();
        let x = vt.alloc("x".into(), Ty::Int, Mutability::Let, None);
        let body = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![IrStmt {
                    kind: IrStmtKind::Bind { var: x, mutability: Mutability::Let, ty: Ty::Int, value: lit_int(1) },
                    span: None,
                }],
                expr: Some(Box::new(var_expr(x, Ty::Int))),
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn detects_var_id_out_of_bounds() {
        let vt = VarTable::new(); // empty table
        let body = var_expr(VarId(99), Ty::Int);
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("VarId(99)"));
    }

    #[test]
    fn assign_checks_var_id_bounds() {
        let vt = VarTable::new(); // empty
        let body = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![IrStmt {
                    kind: IrStmtKind::Assign { var: VarId(99), value: lit_int(2) },
                    span: None,
                }],
                expr: None,
            },
            ty: Ty::Unit,
            span: None, def_id: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("VarId(99)"));
    }

    #[test]
    fn detects_break_outside_loop() {
        let vt = VarTable::new();
        let body = IrExpr { kind: IrExprKind::Break, ty: Ty::Unit, span: None, def_id: None };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("break outside of loop"));
    }

    #[test]
    fn allows_break_inside_loop() {
        let mut vt = VarTable::new();
        let i = vt.alloc("i".into(), Ty::Int, Mutability::Let, None);
        let body = IrExpr {
            kind: IrExprKind::ForIn {
                var: i,
                var_tuple: None,
                iterable: Box::new(IrExpr {
                    kind: IrExprKind::Range {
                        start: Box::new(lit_int(0)),
                        end: Box::new(lit_int(10)),
                        inclusive: false,
                    },
                    ty: Ty::Int, // simplified
                    span: None, def_id: None,
                }),
                body: vec![IrStmt {
                    kind: IrStmtKind::Expr {
                        expr: IrExpr { kind: IrExprKind::Break, ty: Ty::Unit, span: None, def_id: None },
                    },
                    span: None,
                }],
            },
            ty: Ty::Unit,
            span: None, def_id: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert!(errors.is_empty());
    }

    #[test]
    fn detects_binop_type_mismatch() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::AddInt,
                left: Box::new(IrExpr { kind: IrExprKind::LitStr { value: "a".into() }, ty: Ty::String, span: None, def_id: None }),
                right: Box::new(lit_int(1)),
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("AddInt"));
    }

    #[test]
    fn skips_unknown_types_in_binop() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::AddInt,
                left: Box::new(IrExpr { kind: IrExprKind::Hole, ty: Ty::Unknown, span: None, def_id: None }),
                right: Box::new(lit_int(1)),
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert!(errors.is_empty());
    }

    #[test]
    fn detects_continue_outside_loop() {
        let vt = VarTable::new();
        let body = IrExpr { kind: IrExprKind::Continue, ty: Ty::Unit, span: None, def_id: None };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("continue outside of loop"));
    }

    #[test]
    fn verifies_pattern_var_ids() {
        let mut vt = VarTable::new();
        let _x = vt.alloc("x".into(), Ty::Int, Mutability::Let, None);
        // Pattern references VarId(99) which doesn't exist
        let body = IrExpr {
            kind: IrExprKind::Match {
                subject: Box::new(lit_int(1)),
                arms: vec![IrMatchArm {
                    pattern: IrPattern::Bind { var: VarId(99), ty: Ty::Int },
                    guard: None,
                    body: lit_int(2),
                }],
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("VarId(99)"));
    }

    #[test]
    fn verifies_module_functions() {
        let mut main_vt = VarTable::new();
        let _x = main_vt.alloc("x".into(), Ty::Int, Mutability::Let, None);

        let mod_vt = VarTable::new(); // empty
        let mod_body = var_expr(VarId(99), Ty::Int); // out of bounds in module table

        let prog = IrProgram {
            functions: vec![make_fn("main", lit_int(0))],
            top_lets: vec![],
            type_decls: vec![],
            var_table: main_vt,
            def_table: Default::default(),
            modules: vec![IrModule {
                name: "mymod".into(),
                versioned_name: None,
                type_decls: vec![],
                functions: vec![make_fn("helper", mod_body)],
                top_lets: vec![],
                var_table: mod_vt,
                exports: vec![],
                imports: vec![],
            }],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
            used_stdlib_modules: Default::default(),
        };
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].fn_name == "mymod.helper");
    }

    #[test]
    fn detects_unop_type_mismatch() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::UnOp {
                op: UnOp::NegInt,
                operand: Box::new(IrExpr {
                    kind: IrExprKind::LitBool { value: true },
                    ty: Ty::Bool,
                    span: None, def_id: None,
                }),
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("NegInt"));
    }

    #[test]
    fn detects_duplicate_record_fields() {
        let vt = VarTable::new();
        let prog = IrProgram {
            functions: vec![],
            top_lets: vec![],
            type_decls: vec![IrTypeDecl {
                name: "Bad".into(),
                kind: IrTypeDeclKind::Record {
                    fields: vec![
                        IrFieldDecl { name: "x".into(), ty: Ty::Int, default: None, alias: None, attrs: vec![] },
                        IrFieldDecl { name: "x".into(), ty: Ty::String, default: None, alias: None, attrs: vec![] },
                    ],
                },
                deriving: None,
                generics: None,
                visibility: IrVisibility::Public,
                doc: None,
                blank_lines_before: 0,
            }],
            var_table: vt,
            def_table: Default::default(),
            modules: vec![],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
            used_stdlib_modules: Default::default(),
        };
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicate field 'x'"));
    }

    #[test]
    fn detects_duplicate_variant_cases() {
        let vt = VarTable::new();
        let prog = IrProgram {
            functions: vec![],
            top_lets: vec![],
            type_decls: vec![IrTypeDecl {
                name: "Bad".into(),
                kind: IrTypeDeclKind::Variant {
                    cases: vec![
                        IrVariantDecl { name: "A".into(), kind: IrVariantKind::Unit },
                        IrVariantDecl { name: "A".into(), kind: IrVariantKind::Unit },
                    ],
                    is_generic: false,
                    boxed_args: HashSet::new(),
                    boxed_record_fields: HashSet::new(),
                },
                deriving: None,
                generics: None,
                visibility: IrVisibility::Public,
                doc: None,
                blank_lines_before: 0,
            }],
            var_table: vt,
            def_table: Default::default(),
            modules: vec![],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
            used_stdlib_modules: Default::default(),
        };
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicate variant case 'A'"));
    }

    #[test]
    fn detects_duplicate_param_var_ids() {
        let mut vt = VarTable::new();
        let x = vt.alloc("x".into(), Ty::Int, Mutability::Let, None);
        let f = IrFunction {
            name: "bad".into(),
            params: vec![
                IrParam { var: x, ty: Ty::Int, name: "a".into(), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] },
                IrParam { var: x, ty: Ty::Int, name: "b".into(), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] },
            ],
            ret_ty: Ty::Int,
            body: lit_int(0),
            is_effect: false,
            is_async: false,
            is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![], attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![],
            module_origin: None,
        };
        let prog = make_program(vec![f], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("duplicate parameter VarId"));
    }

    #[test]
    fn detects_index_access_on_map() {
        let vt = VarTable::new();
        let map_ty = Ty::Applied(almide_lang::types::TypeConstructorId::Map, vec![Ty::String, Ty::Int]);
        let body = IrExpr {
            kind: IrExprKind::IndexAccess {
                object: Box::new(IrExpr { kind: IrExprKind::EmptyMap, ty: map_ty, span: None, def_id: None }),
                index: Box::new(IrExpr { kind: IrExprKind::LitStr { value: "k".into() }, ty: Ty::String, span: None, def_id: None }),
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("IndexAccess used on Map"));
    }

    #[test]
    fn detects_map_access_on_non_map() {
        let vt = VarTable::new();
        let list_ty = Ty::Applied(almide_lang::types::TypeConstructorId::List, vec![Ty::Int]);
        let body = IrExpr {
            kind: IrExprKind::MapAccess {
                object: Box::new(IrExpr { kind: IrExprKind::List { elements: vec![] }, ty: list_ty, span: None, def_id: None }),
                key: Box::new(lit_int(0)),
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("MapAccess used on non-Map"));
    }

    #[test]
    fn allows_map_access_on_map() {
        let vt = VarTable::new();
        let map_ty = Ty::Applied(almide_lang::types::TypeConstructorId::Map, vec![Ty::String, Ty::Int]);
        let body = IrExpr {
            kind: IrExprKind::MapAccess {
                object: Box::new(IrExpr { kind: IrExprKind::EmptyMap, ty: map_ty, span: None, def_id: None }),
                key: Box::new(IrExpr { kind: IrExprKind::LitStr { value: "k".into() }, ty: Ty::String, span: None, def_id: None }),
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        assert!(verify_program(&prog).is_empty());
    }

    #[test]
    fn pow_int_type_consistency() {
        let vt = VarTable::new();
        // PowInt with Int operands — should pass
        let body = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::PowInt,
                left: Box::new(lit_int(2)),
                right: Box::new(lit_int(3)),
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        assert!(verify_program(&prog).is_empty());

        // PowInt with Float operand — should fail
        let vt2 = VarTable::new();
        let body2 = IrExpr {
            kind: IrExprKind::BinOp {
                op: BinOp::PowInt,
                left: Box::new(IrExpr { kind: IrExprKind::LitFloat { value: 2.0 }, ty: Ty::Float, span: None, def_id: None }),
                right: Box::new(lit_int(3)),
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        let prog2 = make_program(vec![make_fn("main", body2)], vt2);
        let errors = verify_program(&prog2);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("PowInt"));
    }

    #[test]
    fn detects_call_to_unknown_module_function() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: "mymod".into(), func: "nonexistent".into(), def_id: None },
                args: vec![],
                type_args: vec![],
            },
            ty: Ty::Unit,
            span: None, def_id: None,
        };
        // Create program with a module that has a "helper" function but not "nonexistent"
        let mod_fn = make_fn("helper", lit_int(0));
        let prog = IrProgram {
            functions: vec![make_fn("main", body)],
            top_lets: vec![],
            type_decls: vec![],
            var_table: vt,
            modules: vec![IrModule {
                name: "mymod".into(),
                versioned_name: None,
                type_decls: vec![],
                functions: vec![mod_fn],
                top_lets: vec![],
                var_table: VarTable::new(),
                exports: vec![], imports: vec![],
            }],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
            def_table: Default::default(),
            used_stdlib_modules: Default::default(),
        };
        let errors = verify_program(&prog);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("unknown function 'mymod.nonexistent'"));
    }

    #[test]
    fn allows_call_to_known_module_function() {
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: "mymod".into(), func: "helper".into(), def_id: None },
                args: vec![],
                type_args: vec![],
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        let mod_fn = make_fn("helper", lit_int(42));
        let prog = IrProgram {
            functions: vec![make_fn("main", body)],
            top_lets: vec![],
            type_decls: vec![],
            var_table: vt,
            modules: vec![IrModule {
                name: "mymod".into(),
                versioned_name: None,
                type_decls: vec![],
                functions: vec![mod_fn],
                top_lets: vec![],
                var_table: VarTable::new(),
                exports: vec![], imports: vec![],
            }],
            type_registry: Default::default(),
            effect_fn_names: Default::default(),
            effect_map: Default::default(),
            codegen_annotations: Default::default(),
            def_table: Default::default(),
            used_stdlib_modules: Default::default(),
        };
        assert!(verify_program(&prog).is_empty());
    }

    #[test]
    fn allows_call_to_stdlib_module() {
        // stdlib modules (like "string") are not in known_module_functions — should not error
        let vt = VarTable::new();
        let body = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Module { module: "string".into(), func: "len".into(), def_id: None },
                args: vec![IrExpr { kind: IrExprKind::LitStr { value: "hi".into() }, ty: Ty::String, span: None, def_id: None }],
                type_args: vec![],
            },
            ty: Ty::Int,
            span: None, def_id: None,
        };
        let prog = make_program(vec![make_fn("main", body)], vt);
        assert!(verify_program(&prog).is_empty());
    }
}
