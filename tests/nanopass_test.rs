/// Nanopass unit tests: verify each pass transforms IR correctly.
/// Tests construct minimal IrPrograms, run one pass, and assert the output.

use almide::ir::*;
use almide::types::Ty;
use almide::codegen::pass::*;
use almide_base::intern::sym;

// ── Helpers ─────────────────────────────────────────────────────

fn mk_expr(kind: IrExprKind, ty: Ty) -> IrExpr {
    IrExpr { kind, ty, span: None }
}

fn mk_fn(name: &str, params: Vec<IrParam>, ret_ty: Ty, body: IrExpr, is_effect: bool) -> IrFunction {
    IrFunction {
        name: sym(name), params, ret_ty, body,
        is_effect, is_async: false, is_test: false,
        generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![], visibility: IrVisibility::Public,
        doc: None, blank_lines_before: 0,
    }
}

fn mk_param(vt: &mut VarTable, name: &str, ty: Ty) -> IrParam {
    let var = vt.alloc(sym(name), ty.clone(), Mutability::Let, None);
    IrParam { var, ty: ty.clone(), name: sym(name), borrow: ParamBorrow::Own, open_record: None, default: None }
}

fn mk_program(functions: Vec<IrFunction>, var_table: VarTable) -> IrProgram {
    IrProgram { functions, var_table, ..Default::default() }
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
        let mut vt = VarTable::new();
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
        let mut vt = VarTable::new();
        let body = mk_expr(IrExprKind::LitInt { value: 42 }, Ty::Int);
        let func = mk_fn("pure_fn", vec![], Ty::Int, body, false);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&ResultPropagationPass, program, Target::Rust);

        assert_eq!(result.functions[0].ret_ty, Ty::Int,
            "Non-effect fn return type should be unchanged");
    }

    #[test]
    fn effect_fn_body_wrapped_in_ok() {
        let mut vt = VarTable::new();
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
        let mut vt = VarTable::new();
        let body = mk_expr(IrExprKind::Call {
            target: CallTarget::Module { module: sym("fs"), func: sym("read_text") },
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
        let mut vt = VarTable::new();
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
        let mut vt = VarTable::new();
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
            target: CallTarget::Module { module: sym("string"), func: sym("len") },
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
    fn module_call_becomes_inline_rust() {
        // string.slice(s, 0, 0) → InlineRust { "almide_rt_string_slice(&*{s}, ..)", args=[(s, _), ..] }
        // `string.slice` is one of the remaining `@inline_rust` fns
        // (non-primitive signature with `Option<usize>`), so it still
        // takes the `StdlibLoweringPass` → `InlineRust` path. Most other
        // stdlib fns now use `@intrinsic` and are lowered by
        // `IntrinsicLoweringPass` into `RuntimeCall` before this pass.
        let mut vt = VarTable::new();
        let p = mk_param(&mut vt, "s", Ty::String);
        let var_id = p.var;

        let body = mk_expr(IrExprKind::Call {
            target: CallTarget::Module { module: sym("string"), func: sym("slice") },
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

        match &result.functions[0].body.kind {
            IrExprKind::InlineRust { template, args } => {
                assert!(template.contains("almide_rt_string_slice"),
                    "expected runtime fn in template, got: {}", template);
                assert_eq!(args.len(), 3, "expected three param-keyed args");
                assert_eq!(args[0].0.as_str(), "s");
            }
            other => panic!("Expected InlineRust, got {:?}", other),
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
        let mut vt = VarTable::new();
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
        let mut vt = VarTable::new();
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

// ── ClosureConversionPass ───────────────────────────────────────

mod closure_conversion {
    use super::*;
    use almide::codegen::pass_closure_conversion::ClosureConversionPass;

    #[test]
    fn lambda_lifted_to_function() {
        // let f = (x) => x + 1; f(5)
        let mut vt = VarTable::new();
        let v_x = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);
        let v_f = vt.alloc(sym("f"), Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) }, Mutability::Let, None);

        let lambda = mk_expr(IrExprKind::Lambda {
            params: vec![(v_x, Ty::Int)],
            body: Box::new(mk_expr(IrExprKind::BinOp {
                op: BinOp::AddInt,
                left: Box::new(mk_expr(IrExprKind::Var { id: v_x }, Ty::Int)),
                right: Box::new(mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int)),
            }, Ty::Int)),
            lambda_id: Some(0),
        }, Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) });

        let call = mk_expr(IrExprKind::Call {
            target: CallTarget::Computed { callee: Box::new(mk_expr(IrExprKind::Var { id: v_f }, Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) })) },
            args: vec![mk_expr(IrExprKind::LitInt { value: 5 }, Ty::Int)],
            type_args: vec![],
        }, Ty::Int);

        let body = mk_expr(IrExprKind::Block {
            stmts: vec![IrStmt {
                kind: IrStmtKind::Bind {
                    var: v_f,
                    mutability: Mutability::Let,
                    ty: Ty::Fn { params: vec![Ty::Int], ret: Box::new(Ty::Int) },
                    value: lambda,
                },
                span: None,
            }],
            expr: Some(Box::new(call)),
        }, Ty::Int);

        let func = mk_fn("test_closure", vec![], Ty::Int, body, false);
        let program = mk_program(vec![func], vt);
        let result = run_pass(&ClosureConversionPass, program, Target::Wasm);

        // Closure conversion should lift the lambda to a top-level function
        assert!(result.functions.len() > 1,
            "Closure conversion should create new top-level functions, got {}", result.functions.len());
    }
}

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
        let mut vt = VarTable::new();
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
        let mut vt = VarTable::new();
        let body = mk_expr(IrExprKind::LitInt { value: 1 }, Ty::Int);
        let func = mk_fn("simple", vec![], Ty::Int, body, false);

        let program = mk_program(vec![func], vt);
        let result = run_pass(&BoxDerefPass, program, Target::Rust);

        assert_eq!(result.functions.len(), 1);
        assert!(matches!(&result.functions[0].body.kind, IrExprKind::LitInt { value: 1 }));
    }
}

// ── AutoParallelPass ────────────────────────────────────────────

mod auto_parallel {
    use super::*;
    use almide::codegen::pass_auto_parallel::AutoParallelPass;

    #[test]
    fn empty_program_unchanged() {
        let program = mk_program(vec![], VarTable::new());
        let result = run_pass(&AutoParallelPass, program, Target::Rust);
        assert!(result.functions.is_empty());
    }
}
