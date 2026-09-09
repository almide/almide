/// Nanopass unit tests: verify each pass transforms IR correctly.
/// Tests construct minimal IrPrograms, run one pass, and assert the output.

use almide::ir::*;
use almide::types::Ty;
use almide::codegen::pass::*;
use almide_base::intern::sym;

// ── Helpers ─────────────────────────────────────────────────────

fn mk_expr(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None, def_id: None }
}

fn mk_fn(name: &str, params: Vec<IrParam>, ret_ty: Ty, body: IrExpr, is_effect: bool) -> IrFunction {
    IrFunction {
        name: sym(name), params, ret_ty, body,
        is_effect, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
        def_id: None, mutated_params: vec![], module_origin: None,
    }
}

fn mk_param(vt: &mut VarTable, name: &str, ty: Ty) -> IrParam {
    let var = vt.alloc(sym(name), ty.clone(), Mutability::Let, None);
    IrParam { var, ty: ty.clone(), name: sym(name), borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] }
}

fn mk_program(functions: Vec<IrFunction>, var_table: VarTable) -> IrProgram {
    IrProgram { functions, var_table, ..Default::default() }
}

fn mk_record_decl(name: &str, fields: &[(&str, Ty)]) -> IrTypeDecl {
    IrTypeDecl {
        name: sym(name),
        kind: IrTypeDeclKind::Record {
            fields: fields.iter().map(|(fname, fty)| IrFieldDecl {
                name: sym(fname), ty: fty.clone(), default: None, alias: None, attrs: vec![],
            }).collect(),
        },
        deriving: None,
        generics: None,
        visibility: IrVisibility::Public,
        doc: None,
        blank_lines_before: 0,
    }
}

fn run_pass<P: NanoPass>(pass: &P, program: IrProgram, target: Target) -> IrProgram {
    pass.run(program, target).program
}

fn run_pass_changed<P: NanoPass>(pass: &P, program: IrProgram, target: Target) -> (IrProgram, bool) {
    let result = pass.run(program, target);
    (result.program, result.changed)
}

// ── TailCallOptPass ─────────────────────────────────────────────

mod tco {
    use super::*;
    use almide::codegen::pass_tco::TailCallOptPass;

    #[test]
    fn recursive_tail_call_becomes_loop() {
        // fn countdown(n: Int) -> Int = if n <= 0 then 0 else countdown(n - 1)
        let mut vt = VarTable::new();
        let p_n = mk_param(&mut vt, "n", Ty::Int);
        let var_n = p_n.var;

        let body = mk_expr(IrExprKind::If {
            cond: Box::new(mk_expr(IrExprKind::BinOp {
                op: BinOp::Lte,
                left: Box::new(mk_expr(IrExprKind::Var { id: var_n }, Ty::Int)),
                right: Box::new(mk_expr(IrExprKind::LitInt { value: 0 }, Ty::Int)),
            }, Ty::Bool)),
            then: Box::new(mk_expr(IrExprKind::LitInt { value: 0 }, Ty::Int)),
            else_: Box::new(mk_expr(IrExprKind::Call {
                target: CallTarget::Named { name: sym("countdown") },
                args: vec![mk_expr(IrExprKind::BinOp {
                    op: BinOp::SubInt,
                    left: Box::new(mk_expr(IrExprKind::Var { id: var_n }, Ty::Int)),
                    right: Box::new(mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int)),
                }, Ty::Int)],
                type_args: vec![],
            }, Ty::Int)),
        }, Ty::Int);

        let func = mk_fn("countdown", vec![p_n], Ty::Int, body, false);
        let program = mk_program(vec![func], vt);
        let result = run_pass(&TailCallOptPass, program, Target::Rust);

        // The body should now contain a While loop instead of recursive call
        let body = &result.functions[0].body;
        assert!(contains_while(body), "TCO should convert tail recursion to while loop");
        assert!(!contains_self_call(body, "countdown"), "TCO should eliminate recursive call");
    }

    #[test]
    fn non_tail_call_unchanged() {
        // fn factorial(n: Int) -> Int = if n <= 1 then 1 else n * factorial(n - 1)
        // The recursive call is NOT in tail position (it's inside n * ...)
        let mut vt = VarTable::new();
        let p_n = mk_param(&mut vt, "n", Ty::Int);
        let var_n = p_n.var;

        let body = mk_expr(IrExprKind::If {
            cond: Box::new(mk_expr(IrExprKind::BinOp {
                op: BinOp::Lte,
                left: Box::new(mk_expr(IrExprKind::Var { id: var_n }, Ty::Int)),
                right: Box::new(mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int)),
            }, Ty::Bool)),
            then: Box::new(mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int)),
            else_: Box::new(mk_expr(IrExprKind::BinOp {
                op: BinOp::MulInt,
                left: Box::new(mk_expr(IrExprKind::Var { id: var_n }, Ty::Int)),
                right: Box::new(mk_expr(IrExprKind::Call {
                    target: CallTarget::Named { name: sym("factorial") },
                    args: vec![mk_expr(IrExprKind::BinOp {
                        op: BinOp::SubInt,
                        left: Box::new(mk_expr(IrExprKind::Var { id: var_n }, Ty::Int)),
                        right: Box::new(mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int)),
                    }, Ty::Int)],
                    type_args: vec![],
                }, Ty::Int)),
            }, Ty::Int)),
        }, Ty::Int);

        let func = mk_fn("factorial", vec![p_n], Ty::Int, body, false);
        let program = mk_program(vec![func], vt);
        let result = run_pass(&TailCallOptPass, program, Target::Rust);

        // Non-tail recursive call should be preserved
        assert!(contains_self_call(&result.functions[0].body, "factorial"),
            "Non-tail recursion should not be transformed");
    }

    fn contains_while(expr: &IrExpr) -> bool {
        match &expr.kind {
            IrExprKind::While { .. } => true,
            IrExprKind::Block { stmts, expr: tail } => {
                stmts.iter().any(|s| match &s.kind {
                    IrStmtKind::Expr { expr } => contains_while(expr),
                    IrStmtKind::Bind { value, .. } => contains_while(value),
                    _ => false,
                }) || tail.as_ref().map_or(false, |e| contains_while(e))
            }
            IrExprKind::If { then, else_, .. } => contains_while(then) || contains_while(else_),
            _ => false,
        }
    }

    fn contains_self_call(expr: &IrExpr, fn_name: &str) -> bool {
        match &expr.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
                name == fn_name || args.iter().any(|a| contains_self_call(a, fn_name))
            }
            IrExprKind::If { cond, then, else_ } =>
                contains_self_call(cond, fn_name) || contains_self_call(then, fn_name) || contains_self_call(else_, fn_name),
            IrExprKind::BinOp { left, right, .. } =>
                contains_self_call(left, fn_name) || contains_self_call(right, fn_name),
            IrExprKind::Block { stmts, expr: tail } => {
                stmts.iter().any(|s| match &s.kind {
                    IrStmtKind::Expr { expr } => contains_self_call(expr, fn_name),
                    IrStmtKind::Bind { value, .. } => contains_self_call(value, fn_name),
                    _ => false,
                }) || tail.as_ref().map_or(false, |e| contains_self_call(e, fn_name))
            }
            IrExprKind::While { cond, body } => {
                contains_self_call(cond, fn_name) || body.iter().any(|s| match &s.kind {
                    IrStmtKind::Expr { expr } => contains_self_call(expr, fn_name),
                    _ => false,
                })
            }
            _ => false,
        }
    }
}

// ── ResultPropagationPass ───────────────────────────────────────

mod result_propagation {
    use super::*;
    use almide::codegen::pass_result_propagation::ResultPropagationPass;

    #[test]
    fn effect_fn_gets_result_return_type() {
        let vt = VarTable::new();
        let body = mk_expr(IrExprKind::LitInt { value: 42 }, Ty::Int);
        let func = mk_fn("do_io", vec![], Ty::Int, body, true); // is_effect = true

        let mut program = mk_program(vec![func], vt);
        program.effect_fn_names.insert(sym("do_io"));
        let result = run_pass(&ResultPropagationPass, program, Target::Rust);

        // Return type should be wrapped in Result
        assert!(result.functions[0].ret_ty.is_result(),
            "Effect fn return type should be wrapped in Result, got {:?}", result.functions[0].ret_ty);
    }

    #[test]
    fn non_effect_fn_unchanged() {
        let vt = VarTable::new();
        let body = mk_expr(IrExprKind::LitInt { value: 42 }, Ty::Int);
        let func = mk_fn("pure_fn", vec![], Ty::Int, body, false);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&ResultPropagationPass, program, Target::Rust);

        assert_eq!(result.functions[0].ret_ty, Ty::Int,
            "Non-effect fn return type should be unchanged");
    }

    #[test]
    fn effect_fn_body_wrapped_in_ok() {
        let vt = VarTable::new();
        let body = mk_expr(IrExprKind::LitStr { value: "hello".into() }, Ty::String);
        let func = mk_fn("greet", vec![], Ty::String, body, true);

        let mut program = mk_program(vec![func], vt);
        program.effect_fn_names.insert(sym("greet"));
        let result = run_pass(&ResultPropagationPass, program, Target::Rust);

        // Body should be wrapped in Ok(...)
        assert!(matches!(&result.functions[0].body.kind, IrExprKind::ResultOk { .. }),
            "Effect fn body should be wrapped in Ok, got {:?}", result.functions[0].body.kind);
    }
}

// ── EffectInferencePass ─────────────────────────────────────────

mod effect_inference {
    use super::*;
    use almide::codegen::pass_effect_inference::EffectInferencePass;

    #[test]
    fn module_call_detected() {
        // fn read() -> String = fs.read_text("file.txt")
        let vt = VarTable::new();
        let body = mk_expr(IrExprKind::Call {
            target: CallTarget::Module { module: sym("fs"), func: sym("read_text"), def_id: None },
            args: vec![mk_expr(IrExprKind::LitStr { value: "file.txt".into() }, Ty::String)],
            type_args: vec![],
        }, Ty::String);
        let func = mk_fn("read", vec![], Ty::String, body, true);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&EffectInferencePass, program, Target::Rust);

        // effect_map should have an entry for "read"
        assert!(!result.effect_map.functions.is_empty(),
            "EffectInference should populate effect_map");
    }
}

// ── FanLoweringPass ─────────────────────────────────────────────

mod fan_lowering {
    use super::*;
    // FanLoweringPass is in almide::codegen::pass (not pass_fan_lowering)

    #[test]
    fn try_inside_fan_stripped() {
        // fan { try_expr? }
        let vt = VarTable::new();
        let inner = mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int);
        let try_expr = mk_expr(IrExprKind::Try {
            expr: Box::new(inner),
        }, Ty::Int);
        let fan = mk_expr(IrExprKind::Fan {
            exprs: vec![try_expr],
        }, Ty::Int);

        let func = mk_fn("test_fan", vec![], Ty::Int, fan, true);
        let mut program = mk_program(vec![func], vt);
        program.effect_fn_names.insert(sym("test_fan"));
        let result = run_pass(&FanLoweringPass, program, Target::Rust);

        // Try should be stripped from inside Fan
        let body = &result.functions[0].body;
        assert!(!contains_try_in_fan(body), "Try should be stripped from fan expressions");
    }

    fn contains_try_in_fan(expr: &IrExpr) -> bool {
        if let IrExprKind::Fan { exprs } = &expr.kind {
            return exprs.iter().any(|e| matches!(&e.kind, IrExprKind::Try { .. }));
        }
        false
    }
}

// ── PeepholePass ────────────────────────────────────────────────

mod peephole {
    use super::*;
    use almide::codegen::pass_peephole::PeepholePass;

    #[test]
    fn empty_program_unchanged() {
        let program = mk_program(vec![], VarTable::new());
        let result = run_pass(&PeepholePass, program, Target::Rust);
        assert!(result.functions.is_empty());
    }

    #[test]
    fn simple_function_unchanged() {
        // A function with no peephole patterns should pass through unchanged
        let vt = VarTable::new();
        let body = mk_expr(IrExprKind::LitInt { value: 42 }, Ty::Int);
        let func = mk_fn("simple", vec![], Ty::Int, body, false);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&PeepholePass, program, Target::Rust);

        assert_eq!(result.functions.len(), 1);
        assert!(matches!(&result.functions[0].body.kind, IrExprKind::LitInt { value: 42 }));
    }
}

// ── BorrowInsertionPass ─────────────────────────────────────────

mod borrow_insertion {
    use super::*;
    // BorrowInsertionPass is in almide::codegen::pass

    #[test]
    fn string_param_borrowed_when_only_compared() {
        // fn check(s: String) -> Bool = s == "hello"
        // The String param is only used in comparison (BinOp::Eq), not passed to a call
        // or returned → should be RefStr
        let mut vt = VarTable::new();
        let p = mk_param(&mut vt, "s", Ty::String);
        let var_id = p.var;
        let body = mk_expr(IrExprKind::BinOp {
            op: BinOp::Eq,
            left: Box::new(mk_expr(IrExprKind::Var { id: var_id }, Ty::String)),
            right: Box::new(mk_expr(IrExprKind::LitStr { value: "hello".into() }, Ty::String)),
        }, Ty::Bool);
        let func = mk_fn("check", vec![p], Ty::Bool, body, false);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&BorrowInsertionPass, program, Target::Rust);

        let param = &result.functions[0].params[0];
        assert!(matches!(param.borrow, ParamBorrow::RefStr),
            "String param (only compared) should be RefStr, got {:?}", param.borrow);
    }

    #[test]
    fn string_param_own_when_passed_to_call() {
        // fn len(name: String) -> Int = string.len(name)
        // Conservative: passing to any call requires ownership
        let mut vt = VarTable::new();
        let p = mk_param(&mut vt, "name", Ty::String);
        let var_id = p.var;
        let body = mk_expr(IrExprKind::Call {
            target: CallTarget::Module { module: sym("string"), func: sym("len"), def_id: None },
            args: vec![mk_expr(IrExprKind::Var { id: var_id }, Ty::String)],
            type_args: vec![],
        }, Ty::Int);
        let func = mk_fn("len", vec![p], Ty::Int, body, false);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&BorrowInsertionPass, program, Target::Rust);

        assert_eq!(result.functions[0].params[0].borrow, ParamBorrow::Own,
            "String param passed to call should stay Own (conservative)");
    }

    #[test]
    fn string_param_own_when_returned() {
        // fn identity(name: String) -> String = name
        // Directly returning the param requires ownership
        let mut vt = VarTable::new();
        let p = mk_param(&mut vt, "name", Ty::String);
        let var_id = p.var;
        let body = mk_expr(IrExprKind::Var { id: var_id }, Ty::String);
        let func = mk_fn("identity", vec![p], Ty::String, body, false);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&BorrowInsertionPass, program, Target::Rust);

        assert_eq!(result.functions[0].params[0].borrow, ParamBorrow::Own,
            "String param directly returned should stay Own");
    }

    #[test]
    fn int_param_stays_own() {
        let mut vt = VarTable::new();
        let p = mk_param(&mut vt, "x", Ty::Int);
        let body = mk_expr(IrExprKind::Var { id: p.var }, Ty::Int);
        let func = mk_fn("identity", vec![p], Ty::Int, body, false);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&BorrowInsertionPass, program, Target::Rust);

        assert_eq!(result.functions[0].params[0].borrow, ParamBorrow::Own,
            "Int param should remain Own");
    }

    #[test]
    fn record_param_reffed_when_only_field_read() {
        // type Tok = { kind: String, value: Int }
        // fn score(t: Tok) -> Int = t.value
        // The record param is only field-read, never consumed → &Tok (Ref).
        // Locks the #647 perf win: record-typed leaf readers borrow instead of
        // deep-cloning the whole record on every call.
        let mut vt = VarTable::new();
        let tok = Ty::Named(sym("Tok"), vec![]);
        let p = mk_param(&mut vt, "t", tok.clone());
        let var_id = p.var;
        let body = mk_expr(IrExprKind::Member {
            object: Box::new(mk_expr(IrExprKind::Var { id: var_id }, tok.clone())),
            field: sym("value"),
        }, Ty::Int);
        let func = mk_fn("score", vec![p], Ty::Int, body, false);

        let mut program = mk_program(vec![func], vt);
        program.type_decls = vec![mk_record_decl("Tok", &[("kind", Ty::String), ("value", Ty::Int)])];
        let result = run_pass(&BorrowInsertionPass, program, Target::Rust);

        assert_eq!(result.functions[0].params[0].borrow, ParamBorrow::Ref,
            "Record param only field-read should borrow as &Tok (#647), got {:?}",
            result.functions[0].params[0].borrow);
    }

    #[test]
    fn derived_fn_record_param_stays_own() {
        // @derived fn encode(p: Tok) -> Int = p.value
        // Auto-derived convention fns are excluded from borrow inference: their
        // call sites (often cross-module, where the borrow signature can't be
        // looked up) pass OWNED values, so a Ref param would mismatch (E0308).
        // Identification is structural — the `@derived` attribute the generator
        // stamps — NOT the method name. So a derived fn that field-reads a record
        // must keep Own even though plain inference would make it Ref (see
        // record_param_reffed_when_only_field_read).
        let mut vt = VarTable::new();
        let tok = Ty::Named(sym("Tok"), vec![]);
        let p = mk_param(&mut vt, "p", tok.clone());
        let var_id = p.var;
        let body = mk_expr(IrExprKind::Member {
            object: Box::new(mk_expr(IrExprKind::Var { id: var_id }, tok.clone())),
            field: sym("value"),
        }, Ty::Int);
        let mut func = mk_fn("encode", vec![p], Ty::Int, body, false);
        func.attrs.push(almide::ast::Attribute { name: sym("derived"), args: vec![], span: None });

        let mut program = mk_program(vec![func], vt);
        program.type_decls = vec![mk_record_decl("Tok", &[("kind", Ty::String), ("value", Ty::Int)])];
        let result = run_pass(&BorrowInsertionPass, program, Target::Rust);

        assert_eq!(result.functions[0].params[0].borrow, ParamBorrow::Own,
            "@derived fn param must stay Own (excluded from borrow inference), got {:?}",
            result.functions[0].params[0].borrow);
    }
}

// ── CloneInsertionPass ──────────────────────────────────────────

mod clone_insertion {
    use super::*;
    use almide::codegen::pass_clone::CloneInsertionPass;

    #[test]
    fn multi_use_local_string_triggers_pass() {
        // let s = "hello"
        // s + s  — s is a local let binding used twice in concat
        let mut vt = VarTable::new();
        let v_s = vt.alloc(sym("s"), Ty::String, Mutability::Let, None);

        let body = mk_expr(IrExprKind::Block {
            stmts: vec![IrStmt {
                kind: IrStmtKind::Bind {
                    var: v_s,
                    mutability: Mutability::Let,
                    ty: Ty::String,
                    value: mk_expr(IrExprKind::LitStr { value: "hello".into() }, Ty::String),
                },
                span: None,
            }],
            expr: Some(Box::new(mk_expr(IrExprKind::BinOp {
                op: BinOp::ConcatStr,
                left: Box::new(mk_expr(IrExprKind::Var { id: v_s }, Ty::String)),
                right: Box::new(mk_expr(IrExprKind::Var { id: v_s }, Ty::String)),
            }, Ty::String))),
        }, Ty::String);

        let func = mk_fn("dup", vec![], Ty::String, body, false);
        let mut program = mk_program(vec![func], vt);
        program = run_pass(&BorrowInsertionPass, program, Target::Rust);
        let (result, changed) = run_pass_changed(&CloneInsertionPass, program, Target::Rust);

        // CloneInsertion should have processed this program (changed = true)
        // and the String var used twice should be cloned somewhere in the output
        assert!(changed, "CloneInsertion should report changes for multi-use String var");
        assert!(contains_clone(&result.functions[0].body),
            "multi-use String var should be wrapped in a Clone node, got {:?}", result.functions[0].body.kind);
    }

    #[test]
    fn single_use_string_no_clone() {
        let mut vt = VarTable::new();
        let p = mk_param(&mut vt, "s", Ty::String);
        let var_id = p.var;
        vt.entries[var_id.0 as usize].use_count = 1;

        let body = mk_expr(IrExprKind::Var { id: var_id }, Ty::String);
        let func = mk_fn("passthrough", vec![p], Ty::String, body, false);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&CloneInsertionPass, program, Target::Rust);

        assert!(!contains_clone(&result.functions[0].body),
            "Single-use var should not have Clone");
    }

    fn contains_clone(expr: &IrExpr) -> bool {
        match &expr.kind {
            IrExprKind::Clone { .. } => true,
            IrExprKind::BinOp { left, right, .. } => contains_clone(left) || contains_clone(right),
            IrExprKind::Block { stmts, expr: tail } => {
                stmts.iter().any(|s| match &s.kind {
                    IrStmtKind::Bind { value, .. } => contains_clone(value),
                    IrStmtKind::Expr { expr } => contains_clone(expr),
                    _ => false,
                }) || tail.as_ref().map_or(false, |e| contains_clone(e))
            }
            _ => false,
        }
    }
}

// ── MatchSubjectPass ────────────────────────────────────────────

mod match_subject {
    use super::*;
    use almide::codegen::pass_match_subject::MatchSubjectPass;

    #[test]
    fn string_match_annotated() {
        // match s { "a" => 1, _ => 0 }
        let mut vt = VarTable::new();
        let p = mk_param(&mut vt, "s", Ty::String);
        let var_id = p.var;

        let body = mk_expr(IrExprKind::Match {
            subject: Box::new(mk_expr(IrExprKind::Var { id: var_id }, Ty::String)),
            arms: vec![
                IrMatchArm {
                    pattern: IrPattern::Literal { expr: mk_expr(IrExprKind::LitStr { value: "a".into() }, Ty::String) },
                    guard: None,
                    body: mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int),
                },
                IrMatchArm {
                    pattern: IrPattern::Wildcard,
                    guard: None,
                    body: mk_expr(IrExprKind::LitInt { value: 0 }, Ty::Int),
                },
            ],
        }, Ty::Int);

        let func = mk_fn("test_match", vec![p], Ty::Int, body, false);
        let program = mk_program(vec![func], vt);
        let result = run_pass(&MatchSubjectPass, program, Target::Rust);

        // Match subject should have .as_str() annotation or be wrapped
        // The pass sets codegen_annotations or transforms the subject
        assert_eq!(result.functions.len(), 1);
    }
}

// ── StdlibLoweringPass ──────────────────────────────────────────

mod stdlib_lowering {
    use super::*;
    use almide::codegen::pass_stdlib_lowering::StdlibLoweringPass;

    #[test]
    fn module_call_passes_through_stdlib_lowering() {
        // Post Stdlib-Unification completion, every bundled stdlib fn
        // routes through either `@intrinsic` (→ `IntrinsicLoweringPass`
        // produces `RuntimeCall`) or a TOML-dispatcher. `StdlibLowering`
        // now sees neither — any residual `CallTarget::Module` at this
        // stage is a test-synthesized fragment with no pass-level
        // rewriting to apply, so the pass leaves it untouched. This test
        // pins that "left untouched" contract so a future regression
        // that unintentionally re-introduces `@inline_rust` → InlineRust
        // dispatch for stdlib shows up here.
        let mut vt = VarTable::new();
        let p = mk_param(&mut vt, "s", Ty::String);
        let var_id = p.var;

        let body = mk_expr(IrExprKind::Call {
            target: CallTarget::Module { module: sym("string"), func: sym("slice"), def_id: None },
            args: vec![
                mk_expr(IrExprKind::Var { id: var_id }, Ty::String),
                mk_expr(IrExprKind::LitInt { value: 0 }, Ty::Int),
                mk_expr(IrExprKind::LitInt { value: 0 }, Ty::Int),
            ],
            type_args: vec![],
        }, Ty::String);

        let func = mk_fn("test_slice", vec![p], Ty::String, body, false);
        let program = mk_program(vec![func], vt);
        let result = run_pass(&StdlibLoweringPass, program, Target::Rust);

        // StdlibLowering leaves the Module call intact — the real
        // lowering happens in IntrinsicLoweringPass which runs before it
        // in the full pipeline.
        match &result.functions[0].body.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
                assert_eq!(module.as_str(), "string");
                assert_eq!(func.as_str(), "slice");
            }
            other => panic!("Expected untouched Module call, got {:?}", other),
        }
    }
}

// ── BuiltinLoweringPass ─────────────────────────────────────────

mod builtin_lowering {
    use super::*;
    use almide::codegen::pass_builtin_lowering::BuiltinLoweringPass;

    #[test]
    fn assert_eq_becomes_macro() {
        // assert_eq(1, 1)
        let vt = VarTable::new();
        let body = mk_expr(IrExprKind::Call {
            target: CallTarget::Named { name: sym("assert_eq") },
            args: vec![
                mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int),
                mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int),
            ],
            type_args: vec![],
        }, Ty::Unit);

        let func = mk_fn("test_assert", vec![], Ty::Unit, body, false);
        let program = mk_program(vec![func], vt);
        let result = run_pass(&BuiltinLoweringPass, program, Target::Rust);

        assert!(matches!(&result.functions[0].body.kind, IrExprKind::RustMacro { .. }),
            "assert_eq should be lowered to RustMacro, got {:?}", result.functions[0].body.kind);
    }

    #[test]
    fn println_becomes_macro() {
        // println("hello")
        let vt = VarTable::new();
        let body = mk_expr(IrExprKind::Call {
            target: CallTarget::Named { name: sym("println") },
            args: vec![mk_expr(IrExprKind::LitStr { value: "hello".into() }, Ty::String)],
            type_args: vec![],
        }, Ty::Unit);

        let func = mk_fn("test_print", vec![], Ty::Unit, body, false);
        let program = mk_program(vec![func], vt);
        let result = run_pass(&BuiltinLoweringPass, program, Target::Rust);

        assert!(matches!(&result.functions[0].body.kind, IrExprKind::RustMacro { .. }),
            "println should be lowered to RustMacro");
    }
}

// ClosureConversionPass was wasm-pipeline-only and was deleted with the dead
// wasm pipeline in #930 — closure lifting on the live path is exercised by the
// almide-mir lowering tests and the spec/wasm_cross fixtures.

// ── LICMPass ────────────────────────────────────────────────────

mod licm {
    use super::*;
    use almide::codegen::pass_licm::LICMPass;

    #[test]
    fn empty_program_unchanged() {
        let program = mk_program(vec![], VarTable::new());
        let result = run_pass(&LICMPass, program, Target::Rust);
        assert!(result.functions.is_empty());
    }

    #[test]
    fn no_loops_unchanged() {
        let vt = VarTable::new();
        let body = mk_expr(IrExprKind::LitInt { value: 42 }, Ty::Int);
        let func = mk_fn("no_loop", vec![], Ty::Int, body, false);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&LICMPass, program, Target::Rust);

        assert_eq!(result.functions.len(), 1);
        assert!(matches!(&result.functions[0].body.kind, IrExprKind::LitInt { value: 42 }));
    }
}

// ── EggSaturationPass ───────────────────────────────────────────
// Replaces the retired StreamFusionPass / MatrixFusionPass. The
// equality-saturation driver doesn't see plain IntLit / empty
// programs (is_saturation_target filters to list + matrix calls),
// so the same smoke-level pass-identity tests apply.

mod egg_saturation {
    use super::*;
    use almide::codegen::pass_egg_saturation::EggSaturationPass;

    #[test]
    fn empty_program_unchanged() {
        let program = mk_program(vec![], VarTable::new());
        let result = run_pass(&EggSaturationPass, program, Target::Rust);
        assert!(result.functions.is_empty());
    }

    #[test]
    fn simple_function_unchanged() {
        let vt = VarTable::new();
        let body = mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int);
        let func = mk_fn("simple", vec![], Ty::Int, body, false);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&EggSaturationPass, program, Target::Rust);

        assert_eq!(result.functions.len(), 1);
    }
}

// ── CaptureClonePass ────────────────────────────────────────────

mod capture_clone {
    use super::*;
    use almide::codegen::pass_capture_clone::CaptureClonePass;

    #[test]
    fn empty_program_unchanged() {
        let program = mk_program(vec![], VarTable::new());
        let result = run_pass(&CaptureClonePass, program, Target::Rust);
        assert!(result.functions.is_empty());
    }
}

// ── BoxDerefPass ────────────────────────────────────────────────

mod box_deref {
    use super::*;
    use almide::codegen::pass_box_deref::BoxDerefPass;

    #[test]
    fn empty_program_unchanged() {
        let program = mk_program(vec![], VarTable::new());
        let result = run_pass(&BoxDerefPass, program, Target::Rust);
        assert!(result.functions.is_empty());
    }

    #[test]
    fn no_recursive_types_unchanged() {
        let vt = VarTable::new();
        let body = mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int);
        let func = mk_fn("simple", vec![], Ty::Int, body, false);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&BoxDerefPass, program, Target::Rust);

        assert_eq!(result.functions.len(), 1);
        assert!(matches!(&result.functions[0].body.kind, IrExprKind::LitInt { value: 1 }));
    }
}

// ── RegionWindowPass (#1991) ─────────────────────────────────────

mod region_window {
    use super::*;
    use almide::codegen::pass_region_window::RegionWindowPass;

    fn tree_ty() -> Ty { Ty::Named(sym("Tree"), vec![]) }

    fn call(name: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
        mk_expr(IrExprKind::Call { target: CallTarget::Named { name: sym(name) }, args, type_args: vec![] }, ty)
    }

    fn var(id: VarId, ty: Ty) -> IrExpr { mk_expr(IrExprKind::Var { id }, ty) }

    fn lit(v: i64) -> IrExpr { mk_expr(IrExprKind::LitInt { value: v }, Ty::Int) }

    /// `type Tree = Leaf | Node(<payload>...)` with `Tree, Tree` after `extra`.
    fn tree_decl(extra: Vec<Ty>) -> IrTypeDecl {
        let mut fields = extra;
        fields.extend([tree_ty(), tree_ty()]);
        IrTypeDecl {
            name: sym("Tree"),
            kind: IrTypeDeclKind::Variant {
                cases: vec![
                    IrVariantDecl { name: sym("Leaf"), kind: IrVariantKind::Unit },
                    IrVariantDecl { name: sym("Node"), kind: IrVariantKind::Tuple { fields } },
                ],
                is_generic: false,
                boxed_args: Default::default(),
                boxed_record_fields: Default::default(),
            },
            deriving: None,
            generics: None,
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
        }
    }

    /// `fn make(d) = if d == 0 then Leaf else Node(<extra>, make(d - 1), make(d - 1))`.
    fn make_fn(vt: &mut VarTable, extra: Vec<IrExpr>) -> IrFunction {
        let d = mk_param(vt, "depth", Ty::Int);
        let dv = d.var;
        let rec = || call("make", vec![mk_expr(IrExprKind::BinOp { op: BinOp::SubInt, left: Box::new(var(dv, Ty::Int)), right: Box::new(lit(1)) }, Ty::Int)], tree_ty());
        let mut args = extra;
        args.extend([rec(), rec()]);
        let body = mk_expr(IrExprKind::If {
            cond: Box::new(mk_expr(IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(var(dv, Ty::Int)), right: Box::new(lit(0)) }, Ty::Bool)),
            then: Box::new(call("Leaf", vec![], tree_ty())),
            else_: Box::new(call("Node", args, tree_ty())),
        }, tree_ty());
        mk_fn("make", vec![d], tree_ty(), body, false)
    }

    /// `fn check(t) = match t { Leaf => 1, Node(<wild>..., l, r) => check(l) + check(r) + 1 }`.
    fn check_fn(vt: &mut VarTable, extra_wild: usize) -> IrFunction {
        let t = mk_param(vt, "tree", tree_ty());
        let tv = t.var;
        let l = vt.alloc(sym("left"), tree_ty(), Mutability::Let, None);
        let r = vt.alloc(sym("right"), tree_ty(), Mutability::Let, None);
        let mut args: Vec<IrPattern> = (0..extra_wild).map(|_| IrPattern::Wildcard).collect();
        args.extend([IrPattern::Bind { var: l, ty: tree_ty() }, IrPattern::Bind { var: r, ty: tree_ty() }]);
        let sum = mk_expr(IrExprKind::BinOp {
            op: BinOp::AddInt,
            left: Box::new(call("check", vec![var(l, tree_ty())], Ty::Int)),
            right: Box::new(call("check", vec![var(r, tree_ty())], Ty::Int)),
        }, Ty::Int);
        let body = mk_expr(IrExprKind::Match {
            subject: Box::new(var(tv, tree_ty())),
            arms: vec![
                IrMatchArm { pattern: IrPattern::Constructor { name: "Leaf".into(), args: vec![] }, guard: None, body: lit(1) },
                IrMatchArm { pattern: IrPattern::Constructor { name: "Node".into(), args }, guard: None, body: sum },
            ],
        }, Ty::Int);
        mk_fn("check", vec![t], Ty::Int, body, false)
    }

    fn has_window(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::InlineRust { template, .. } if template.contains("almide_region_window") => true,
            _ => {
                let mut found = false;
                e.clone().map_children(&mut |c| { found |= has_window(&c); c });
                found
            }
        }
    }

    fn fn_names(p: &IrProgram) -> Vec<String> { p.functions.iter().map(|f| f.name.to_string()).collect() }

    #[test]
    fn window_fires_on_check_of_make() {
        let mut vt = VarTable::new();
        let make = make_fn(&mut vt, vec![]);
        let check = check_fn(&mut vt, 0);
        let d = mk_param(&mut vt, "d", Ty::Int);
        let site = call("check", vec![call("make", vec![var(d.var, Ty::Int)], tree_ty())], Ty::Int);
        let caller = mk_fn("run", vec![d], Ty::Int, site, false);
        let mut program = mk_program(vec![make, check, caller], vt);
        program.type_decls.push(tree_decl(vec![]));

        let (out, changed) = run_pass_changed(&RegionWindowPass, program, Target::Rust);
        assert!(changed);
        let names = fn_names(&out);
        assert!(names.contains(&"__rgn_make".to_string()) && names.contains(&"__rgn_check".to_string()), "{names:?}");
        assert!(out.type_decls.iter().any(|td| td.name.as_str() == "__rgn_Tree"));
        assert!(out.codegen_annotations.region_enums.contains("__rgn_Tree"));
        let run = out.functions.iter().find(|f| f.name.as_str() == "run").unwrap();
        assert!(has_window(&run.body), "the site was not rewritten: {:?}", run.body.kind);
        // The originals are untouched: `make` still constructs `Node`.
        let make = out.functions.iter().find(|f| f.name.as_str() == "make").unwrap();
        assert!(!has_window(&make.body));
    }

    #[test]
    fn twin_enum_is_copy_admissible() {
        let mut vt = VarTable::new();
        let make = make_fn(&mut vt, vec![]);
        let check = check_fn(&mut vt, 0);
        let d = mk_param(&mut vt, "d", Ty::Int);
        let site = call("check", vec![call("make", vec![var(d.var, Ty::Int)], tree_ty())], Ty::Int);
        let mut program = mk_program(vec![make, check, mk_fn("run", vec![d], Ty::Int, site, false)], vt);
        program.type_decls.push(tree_decl(vec![]));
        let out = run_pass(&RegionWindowPass, program, Target::Rust);
        let twin = out.type_decls.iter().find(|td| td.name.as_str() == "__rgn_Tree").expect("twin enum");
        let IrTypeDeclKind::Variant { cases, .. } = &twin.kind else { panic!("twin is a variant") };
        assert_eq!(cases.iter().map(|c| c.name.to_string()).collect::<Vec<_>>(), vec!["__rgn_Leaf", "__rgn_Node"]);
        let IrVariantKind::Tuple { fields } = &cases[1].kind else { panic!("Node is a tuple case") };
        // Every payload is the twin itself: the walker renders it as a
        // `Copy` `AlmideRgn` handle and the enum gets `impl Copy`.
        assert!(fields.iter().all(|f| matches!(f, Ty::Named(n, _) if n.as_str() == "__rgn_Tree")), "{fields:?}");
        // The twin fn's param carries the twin type and a FRESH VarId.
        let check = out.functions.iter().find(|f| f.name.as_str() == "check").unwrap();
        let twin_check = out.functions.iter().find(|f| f.name.as_str() == "__rgn_check").unwrap();
        assert_eq!(twin_check.params[0].ty, Ty::Named(sym("__rgn_Tree"), vec![]));
        assert_ne!(twin_check.params[0].var, check.params[0].var);
    }

    #[test]
    fn held_tree_is_not_a_window() {
        let mut vt = VarTable::new();
        let make = make_fn(&mut vt, vec![]);
        let check = check_fn(&mut vt, 0);
        let d = mk_param(&mut vt, "d", Ty::Int);
        let t = vt.alloc(sym("t"), tree_ty(), Mutability::Let, None);
        let body = mk_expr(IrExprKind::Block {
            stmts: vec![IrStmt { kind: IrStmtKind::Bind { var: t, mutability: Mutability::Let, ty: tree_ty(), value: call("make", vec![var(d.var, Ty::Int)], tree_ty()) }, span: None }],
            expr: Some(Box::new(call("check", vec![var(t, tree_ty())], Ty::Int))),
        }, Ty::Int);
        let mut program = mk_program(vec![make, check, mk_fn("run", vec![d], Ty::Int, body, false)], vt);
        program.type_decls.push(tree_decl(vec![]));
        let (out, changed) = run_pass_changed(&RegionWindowPass, program, Target::Rust);
        assert!(!changed);
        assert!(!fn_names(&out).iter().any(|n| n.starts_with("__rgn_")));
        assert!(out.type_decls.len() == 1);
    }

    /// v1 is root-module only: a dependency module declaring a SAME-NAMED
    /// `Tree` / `make` / `check` (#1955's collision shape) is neither
    /// scanned for sites nor twinned, and the root twins are the root's.
    #[test]
    fn same_named_dependency_module_fns_and_types_are_left_alone() {
        let mut vt = VarTable::new();
        let make = make_fn(&mut vt, vec![]);
        let check = check_fn(&mut vt, 0);
        let d = mk_param(&mut vt, "d", Ty::Int);
        let site = call("check", vec![call("make", vec![var(d.var, Ty::Int)], tree_ty())], Ty::Int);
        let mut program = mk_program(vec![make, check, mk_fn("run", vec![d], Ty::Int, site, false)], vt);
        program.type_decls.push(tree_decl(vec![]));
        let dep_make = make_fn(&mut program.var_table, vec![]);
        let dep_check = check_fn(&mut program.var_table, 0);
        let dd = mk_param(&mut program.var_table, "d", Ty::Int);
        let dep_site = call("check", vec![call("make", vec![var(dd.var, Ty::Int)], tree_ty())], Ty::Int);
        program.modules.push(IrModule {
            name: sym("dep.shape"),
            versioned_name: None,
            type_decls: vec![tree_decl(vec![])],
            functions: vec![dep_make, dep_check, mk_fn("run", vec![dd], Ty::Int, dep_site, false)],
            top_lets: vec![],
            var_table: VarTable::new(),
            exports: vec![],
            imports: vec![],
        });
        let out = run_pass(&RegionWindowPass, program, Target::Rust);
        let twins = fn_names(&out).iter().filter(|n| n.starts_with("__rgn_")).count();
        assert_eq!(twins, 2, "exactly the root pair is twinned");
        assert_eq!(out.type_decls.iter().filter(|td| td.name.as_str() == "__rgn_Tree").count(), 1);
        let dep = &out.modules[0];
        assert!(dep.functions.iter().all(|f| !f.name.as_str().starts_with("__rgn_")));
        assert!(dep.type_decls.iter().all(|td| !td.name.as_str().starts_with("__rgn_")));
        assert!(!has_window(&dep.functions[2].body), "a module site is not rewritten in v1");
    }

    #[test]
    fn non_scalar_payload_refuses() {
        let mut vt = VarTable::new();
        let label = mk_expr(IrExprKind::LitStr { value: "n".into() }, Ty::String);
        let make = make_fn(&mut vt, vec![label]);
        let check = check_fn(&mut vt, 1);
        let d = mk_param(&mut vt, "d", Ty::Int);
        let site = call("check", vec![call("make", vec![var(d.var, Ty::Int)], tree_ty())], Ty::Int);
        let mut program = mk_program(vec![make, check, mk_fn("run", vec![d], Ty::Int, site, false)], vt);
        program.type_decls.push(tree_decl(vec![Ty::String]));
        let (out, changed) = run_pass_changed(&RegionWindowPass, program, Target::Rust);
        assert!(!changed, "a String payload cannot be Copy — no twin");
        assert!(!fn_names(&out).iter().any(|n| n.starts_with("__rgn_")));
        let run = out.functions.iter().find(|f| f.name.as_str() == "run").unwrap();
        assert!(!has_window(&run.body));
    }
}

// ── RustLowering: fan parallel routing (#2044) ─────────────────

mod fan_parallel_routing {
    use super::*;
    use almide::codegen::pass_rust_lowering_fan::route_fan_parallel;

    /// The routing step alone (RustLoweringPass calls it first).
    fn route(mut program: IrProgram) -> IrProgram {
        route_fan_parallel(&mut program);
        program
    }

    #[test]
    fn empty_program_unchanged() {
        let mut program = mk_program(vec![], VarTable::new());
        assert!(!route_fan_parallel(&mut program));
        assert!(program.functions.is_empty());
    }

    use almide::types::constructor::TypeConstructorId as TC;

    fn list_ty(t: Ty) -> Ty { Ty::Applied(TC::List, vec![t]) }
    fn result_ty(t: Ty) -> Ty { Ty::Applied(TC::Result, vec![t, Ty::String]) }

    /// `(x) => ok(<body>)` with `x: Int`; `body` is built from the param var.
    fn ok_lambda(vt: &mut VarTable, body: impl FnOnce(VarId) -> IrExpr) -> IrExpr {
        let x = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
        let inner = body(x);
        let ret = result_ty(inner.ty.clone());
        let body = mk_expr(IrExprKind::ResultOk { expr: Box::new(inner) }, ret.clone());
        mk_expr(
            IrExprKind::Lambda { params: vec![(x, Ty::Int)], body: Box::new(body), lambda_id: None },
            Ty::Fn { params: vec![Ty::Int], ret: Box::new(ret), is_effect: false },
        )
    }

    fn int_list() -> IrExpr {
        mk_expr(IrExprKind::List { elements: vec![mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int)] }, list_ty(Ty::Int))
    }

    fn fan_map(lambda: IrExpr) -> IrExpr {
        let elem = match &lambda.ty { Ty::Fn { ret, .. } => match &**ret { Ty::Applied(_, a) => a[0].clone(), _ => unreachable!() }, _ => unreachable!() };
        mk_expr(
            IrExprKind::Call {
                target: CallTarget::Module { module: sym("fan"), func: sym("map"), def_id: None },
                args: vec![int_list(), lambda],
                type_args: vec![],
            },
            result_ty(list_ty(elem)),
        )
    }

    fn fan_func_name(program: &IrProgram) -> String {
        match &program.functions[0].body.kind {
            IrExprKind::Call { target: CallTarget::Module { func, .. }, .. } => func.to_string(),
            other => panic!("unexpected body {other:?}"),
        }
    }

    #[test]
    fn fan_map_pure_scalar_lambda_goes_parallel() {
        // fan.map([1], (x) => ok(x)) — Int in, Int out, no captures.
        let mut vt = VarTable::new();
        let lambda = ok_lambda(&mut vt, |x| mk_expr(IrExprKind::Var { id: x }, Ty::Int));
        let program = mk_program(vec![mk_fn("main", vec![], Ty::Unit, fan_map(lambda), true)], vt);
        let out = route(program);
        assert_eq!(fan_func_name(&out), "map_par");
    }

    #[test]
    fn fan_map_string_result_stays_sequential() {
        // fan.map([1], (x) => ok("s")) — String crosses the thread: declined.
        let mut vt = VarTable::new();
        let lambda = ok_lambda(&mut vt, |_| mk_expr(IrExprKind::LitStr { value: "s".into() }, Ty::String));
        let program = mk_program(vec![mk_fn("main", vec![], Ty::Unit, fan_map(lambda), true)], vt);
        let out = route(program);
        assert_eq!(fan_func_name(&out), "map");
    }

    #[test]
    fn fan_map_list_capture_stays_sequential() {
        // let ls: List[Int]; fan.map([1], (x) => ok(x)) whose body reads `ls`
        // (an Rc-shaped capture): declined even though the lambda is pure.
        let mut vt = VarTable::new();
        let ls = vt.alloc(sym("ls"), list_ty(Ty::Int), Mutability::Let, None);
        let lambda = ok_lambda(&mut vt, |x| mk_expr(
            IrExprKind::Block {
                stmts: vec![IrStmt { kind: IrStmtKind::Expr { expr: mk_expr(IrExprKind::Var { id: ls }, list_ty(Ty::Int)) }, span: None }],
                expr: Some(Box::new(mk_expr(IrExprKind::Var { id: x }, Ty::Int))),
            },
            Ty::Int,
        ));
        let program = mk_program(vec![mk_fn("main", vec![], Ty::Unit, fan_map(lambda), true)], vt);
        let out = route(program);
        assert_eq!(fan_func_name(&out), "map");
    }

    #[test]
    fn fan_map_effect_callback_stays_sequential() {
        // fan.map([1], (x) => ok(probe(x))) with `probe` an effect fn.
        let mut vt = VarTable::new();
        let lambda = ok_lambda(&mut vt, |x| mk_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("probe") },
                args: vec![mk_expr(IrExprKind::Var { id: x }, Ty::Int)],
                type_args: vec![],
            },
            Ty::Int,
        ));
        let probe = mk_fn("probe", vec![], Ty::Int, mk_expr(IrExprKind::LitInt { value: 0 }, Ty::Int), true);
        let program = mk_program(vec![mk_fn("main", vec![], Ty::Unit, fan_map(lambda), true), probe], vt);
        let out = route(program);
        assert_eq!(fan_func_name(&out), "map");
    }

    #[test]
    fn fan_map_plain_effect_helper_stays_sequential() {
        // A plain function can reach an effect even without an effect declaration.
        let mut vt = VarTable::new();
        let lambda = ok_lambda(&mut vt, |x| mk_expr(
            IrExprKind::Call {
                target: CallTarget::Named { name: sym("probe") },
                args: vec![mk_expr(IrExprKind::Var { id: x }, Ty::Int)],
                type_args: vec![],
            },
            Ty::Int,
        ));
        let probe = mk_fn("probe", vec![], Ty::Int, mk_expr(
            IrExprKind::RuntimeCall { symbol: sym("almide_rt_random_int"), args: vec![] },
            Ty::Int,
        ), false);
        let program = mk_program(vec![mk_fn("main", vec![], Ty::Unit, fan_map(lambda), true), probe], vt);
        let out = route(program);
        assert_eq!(fan_func_name(&out), "map");
    }

    fn list_map_call(vt: &mut VarTable) -> IrExpr {
        let x = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
        let lambda = mk_expr(
            IrExprKind::Lambda { params: vec![(x, Ty::Int)], body: Box::new(mk_expr(IrExprKind::Var { id: x }, Ty::Int)), lambda_id: None },
            Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int), is_effect: false },
        );
        mk_expr(IrExprKind::RuntimeCall { symbol: sym("almide_rt_list_map"), args: vec![int_list(), lambda] }, list_ty(Ty::Int))
    }

    #[test]
    fn fusion_preserves_explicit_fan_routing() {
        use almide::codegen::pass_stream_fusion::StreamFusionPass;
        for explicit in [false, true] {
            let mut vt = VarTable::new();
            let call = list_map_call(&mut vt);
            let body = if explicit {
                mk_expr(IrExprKind::Fan { exprs: vec![call] }, list_ty(Ty::Int))
            } else { call };
            let program = mk_program(vec![mk_fn("main", vec![], Ty::Unit, body, true)], vt);
            let fused = StreamFusionPass.run(program, Target::Rust).program;
            let out = route(fused);
            if explicit {
                assert_eq!(runtime_symbol(&out.functions[0].body), "almide_rt_list_par_map");
            } else {
                assert!(matches!(out.functions[0].body.kind, IrExprKind::IterChain { .. }));
            }
        }
    }

    fn runtime_symbol(e: &IrExpr) -> String {
        match &e.kind {
            IrExprKind::RuntimeCall { symbol, .. } => symbol.to_string(),
            IrExprKind::Fan { exprs } => runtime_symbol(&exprs[0]),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn list_map_under_fan_block_goes_parallel_but_not_outside() {
        let mut vt = VarTable::new();
        let under = mk_expr(IrExprKind::Fan { exprs: vec![list_map_call(&mut vt)] }, list_ty(Ty::Int));
        let outside = list_map_call(&mut vt);
        let program = mk_program(vec![
            mk_fn("a", vec![], Ty::Unit, under, false),
            mk_fn("b", vec![], Ty::Unit, outside, false),
        ], vt);
        let out = route(program);
        assert_eq!(runtime_symbol(&out.functions[0].body), "almide_rt_list_par_map");
        assert_eq!(runtime_symbol(&out.functions[1].body), "almide_rt_list_map");
    }
}
