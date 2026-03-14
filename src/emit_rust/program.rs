use super::Emitter;
use super::JSON_RUNTIME;
use super::HTTP_RUNTIME;
use super::TIME_RUNTIME;
use super::REGEX_RUNTIME;
use super::IO_RUNTIME;
use super::PLATFORM_RUNTIME;
use super::COLLECTION_RUNTIME;
use super::CORE_RUNTIME;
use super::DATETIME_RUNTIME;
use super::UUID_RUNTIME;

impl<'ir> Emitter<'ir> {
    /// Scan IR functions to classify effect/result functions.
    fn collect_fn_info_from_ir(&mut self, functions: &[almide::ir::IrFunction]) {
        for func in functions {
            if func.is_test { continue; }
            let ret_str = self.ir_ty_to_rust_sig(&func.ret_ty);
            let returns_result = ret_str.starts_with("Result<");
            if func.is_effect {
                self.effect_fns.push(func.name.clone());
            }
            if returns_result || func.is_effect {
                self.result_fns.push(func.name.clone());
            }
        }
    }

    /// Collect named record types and variant metadata from IR type_decls.
    /// This populates the same fields as `collect_named_records` but from IR instead of AST.
    fn collect_named_records_from_ir(&mut self, type_decls: &[almide::ir::IrTypeDecl]) {
        use almide::ir::{IrTypeDeclKind, IrVariantKind};
        for td in type_decls {
            match &td.kind {
                IrTypeDeclKind::Record { fields } => {
                    let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                    self.named_record_types.insert(field_names, td.name.clone());
                }
                IrTypeDeclKind::Variant { cases, is_generic, boxed_args, boxed_record_fields } => {
                    // Merge boxed_args and boxed_record_fields
                    for ba in boxed_args {
                        self.boxed_variant_args.insert(ba.clone());
                    }
                    for brf in boxed_record_fields {
                        self.boxed_variant_record_fields.insert(brf.clone());
                    }
                    for case in cases {
                        if *is_generic {
                            self.generic_variant_constructors.insert(case.name.clone(), td.name.clone());
                            if matches!(&case.kind, IrVariantKind::Unit) {
                                self.generic_variant_unit_ctors.insert(case.name.clone());
                            }
                        }
                    }
                }
                IrTypeDeclKind::Alias { target } => {
                    if let almide::types::Ty::OpenRecord { fields } = target {
                        self.open_record_aliases.insert(td.name.clone(), fields.clone());
                    }
                }
            }
        }
    }

    /// Collect top-level let names from IR, pre-classifying using TopLetKind.
    fn collect_top_lets_from_ir(&mut self, ir: &almide::ir::IrProgram) {
        for tl in &ir.top_lets {
            let info = ir.var_table.get(tl.var);
            let needs_deref = match tl.kind {
                almide::ir::TopLetKind::Lazy => true,
                almide::ir::TopLetKind::Const => false,
            };
            // String top-lets always need LazyLock
            let needs_deref = needs_deref || matches!(tl.ty, almide::types::Ty::String);
            self.top_let_names.insert(info.name.clone(), needs_deref);
        }
    }

    pub(crate) fn emit_program(&mut self) {
        let ir = self.ir_program;

        // ── Phase 1: Collect metadata ──
        self.collect_fn_info_from_ir(&ir.functions);
        self.collect_named_records_from_ir(&ir.type_decls);
        self.collect_top_lets_from_ir(&ir);

        // Module metadata
        for ir_mod in &ir.modules {
            self.collect_fn_info_from_ir(&ir_mod.functions);
            self.collect_named_records_from_ir(&ir_mod.type_decls);
        }
        for ir_mod in &ir.modules {
            if let Some(ref versioned) = ir_mod.versioned_name {
                self.module_aliases.insert(ir_mod.name.clone(), versioned.clone());
                self.user_modules.push(versioned.clone());
            } else {
                self.user_modules.push(ir_mod.name.clone());
            }
        }

        // ── Phase 2: Emit runtime and preamble ──
        self.emitln("#![allow(unused_parens, unused_variables, dead_code, unused_imports, unused_mut, unused_must_use)]");
        self.emitln("");
        self.emit_runtime();
        self.emitln("");

        let anon_record_placeholder = "/* __ALMD_ANON_RECORDS__ */";
        self.emitln(anon_record_placeholder);
        self.emitln("");

        // ── Phase 3: Collect cross-module type info ──
        let mut module_variant_types: Vec<(String, Vec<String>)> = Vec::new();
        let mut module_record_types: Vec<(String, Vec<String>)> = Vec::new();

        for ir_mod in &ir.modules {
            let rust_mod = ir_mod.versioned_name.as_ref()
                .unwrap_or(&ir_mod.name)
                .replace('.', "_");
            let mut variant_names = Vec::new();
            let mut record_names = Vec::new();
            for td in &ir_mod.type_decls {
                match &td.kind {
                    almide::ir::IrTypeDeclKind::Variant { .. } => variant_names.push(td.name.clone()),
                    almide::ir::IrTypeDeclKind::Record { .. } => record_names.push(td.name.clone()),
                    _ => {}
                }
            }
            module_variant_types.push((rust_mod.clone(), variant_names));
            module_record_types.push((rust_mod, record_names));
        }

        // ── Phase 4: Emit modules ──
        for ir_mod in &ir.modules {
            self.emit_ir_user_module(&ir_mod, &module_variant_types, &module_record_types);
            self.emitln("");
        }

        // Import variant/record types from modules into top-level scope
        for (rust_mod, variant_names) in &module_variant_types {
            for vname in variant_names {
                self.emitln(&format!("use {}::{};", rust_mod, vname));
                self.emitln(&format!("use {}::{}::*;", rust_mod, vname));
            }
        }
        for (rust_mod, record_names) in &module_record_types {
            for rname in record_names {
                self.emitln(&format!("use {}::{};", rust_mod, rname));
            }
        }
        if !module_variant_types.iter().all(|(_, v)| v.is_empty())
            || !module_record_types.iter().all(|(_, r)| r.is_empty()) {
            self.emitln("");
        }

        // ── Phase 5: Emit declarations from IR ──
        // Emit type declarations
        for td in &ir.type_decls {
            self.emit_ir_type_decl(td);
            self.emitln("");
        }

        // Emit top-level lets
        for tl in &ir.top_lets {
            self.emit_ir_top_let(tl);
            self.emitln("");
        }

        // Emit functions
        for func in &ir.functions {
            // Skip original structurally-bounded generic functions (mono pass emits specialized copies)
            if Self::has_structural_bounds(func.generics.as_ref()) {
                continue;
            }
            if func.is_test {
                self.emit_ir_test(func);
            } else {
                self.emit_ir_fn_decl(func);
            }
            self.emitln("");
        }

        // Emit main() wrapper
        if let Some(main_fn) = ir.functions.iter().find(|f| f.name == "main") {
            let has_args = !main_fn.params.is_empty();
            let ret_str = self.ir_ty_to_rust_sig(&main_fn.ret_ty);
            let returns_result = ret_str.starts_with("Result<") || main_fn.is_effect;

            self.emitln("fn main() {");
            self.indent += 1;

            if self.no_thread_wrap {
                self.emit_main_body(has_args, returns_result);
            } else {
                self.emitln("let t = std::thread::Builder::new().stack_size(8 * 1024 * 1024).spawn(|| {");
                self.indent += 1;
                self.emit_main_body(has_args, returns_result);
                self.indent -= 1;
                self.emitln("}).expect(\"failed to spawn main thread\");");
                self.emitln("t.join().expect(\"main thread panicked\");");
            }

            self.indent -= 1;
            self.emitln("}");
        }

        // Replace placeholder with collected anonymous record struct definitions
        let structs_code = {
            let anon_records = self.anon_record_structs.borrow();
            if anon_records.is_empty() {
                None
            } else {
                let mut code = String::new();
                for (field_names, struct_name) in anon_records.iter() {
                    let n = field_names.len();
                    let type_params: Vec<String> = (0..n).map(|i| format!("T{}", i)).collect();
                    code.push_str(&format!("#[derive(Debug, Clone, PartialEq)]\nstruct {}<{}> {{\n", struct_name, type_params.join(", ")));
                    for (i, fname) in field_names.iter().enumerate() {
                        code.push_str(&format!("    pub {}: T{},\n", fname, i));
                    }
                    code.push_str("}\n\n");
                }
                Some(code)
            }
        };
        if let Some(code) = structs_code {
            self.out = self.out.replace("/* __ALMD_ANON_RECORDS__ */\n", &code);
        } else {
            self.out = self.out.replace("/* __ALMD_ANON_RECORDS__ */\n", "");
        }
    }

    fn emit_main_body(&mut self, has_args: bool, returns_result: bool) {
        if has_args {
            self.emitln("let args: Vec<String> = std::env::args().collect();");
        }
        let call = if has_args { "almide_main(args)" } else { "almide_main()" };
        if returns_result {
            self.emitln(&format!("if let Err(e) = {} {{", call));
            self.indent += 1;
            self.emitln("eprintln!(\"Error: {}\", e);");
            self.emitln("std::process::exit(1);");
            self.indent -= 1;
            self.emitln("}");
        } else {
            self.emitln(&format!("{};", call));
        }
    }

    fn emit_runtime(&mut self) {
        self.emitln("use std::collections::HashMap;");
        self.emitln("trait AlmideConcat<Rhs> { type Output; fn concat(self, rhs: Rhs) -> Self::Output; }");
        self.emitln("impl AlmideConcat<String> for String { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }");
        self.emitln("impl AlmideConcat<&str> for String { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }");
        self.emitln("impl AlmideConcat<String> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }");
        self.emitln("impl AlmideConcat<&str> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }");
        self.emitln("impl<T: Clone> AlmideConcat<Vec<T>> for Vec<T> { type Output = Vec<T>; #[inline(always)] fn concat(self, rhs: Vec<T>) -> Vec<T> { let mut r = self; r.extend(rhs); r } }");
        self.emitln("trait AlmidePushConcat<Rhs> { fn almide_push_concat(&mut self, rhs: Rhs); }");
        self.emitln("impl AlmidePushConcat<String> for String { #[inline(always)] fn almide_push_concat(&mut self, rhs: String) { self.push_str(&rhs); } }");
        self.emitln("impl AlmidePushConcat<&str> for String { #[inline(always)] fn almide_push_concat(&mut self, rhs: &str) { self.push_str(rhs); } }");
        self.emitln("impl<T: Clone> AlmidePushConcat<Vec<T>> for Vec<T> { #[inline(always)] fn almide_push_concat(&mut self, rhs: Vec<T>) { self.extend(rhs); } }");
        self.emitln("macro_rules! almide_eq { ($a:expr, $b:expr) => { ($a) == ($b) }; }");
        self.emitln("macro_rules! almide_ne { ($a:expr, $b:expr) => { ($a) != ($b) }; }");
        self.emitln("");
        // Minimal async runtime (block_on)
        self.emitln("fn almide_block_on<F: std::future::Future>(future: F) -> F::Output {");
        self.emitln("    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};");
        self.emitln("    use std::pin::Pin;");
        self.emitln("    fn dummy_raw_waker() -> RawWaker { fn no_op(_: *const ()) {} fn clone(p: *const ()) -> RawWaker { RawWaker::new(p, &VTABLE) } static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op); RawWaker::new(std::ptr::null(), &VTABLE) }");
        self.emitln("    let waker = unsafe { Waker::from_raw(dummy_raw_waker()) };");
        self.emitln("    let mut cx = Context::from_waker(&waker);");
        self.emitln("    let mut future = Box::pin(future);");
        self.emitln("    loop { match future.as_mut().poll(&mut cx) { Poll::Ready(val) => return val, Poll::Pending => std::thread::yield_now(), } }");
        self.emitln("}");
        self.emitln("");
        self.out.push_str(IO_RUNTIME);
        self.emitln("");
        self.out.push_str(JSON_RUNTIME);
        self.emitln("");
        self.out.push_str(HTTP_RUNTIME);
        self.emitln("");
        self.out.push_str(TIME_RUNTIME);
        self.emitln("");
        self.out.push_str(REGEX_RUNTIME);
        self.emitln("");
        self.out.push_str(PLATFORM_RUNTIME);
        self.emitln("");
        self.out.push_str(COLLECTION_RUNTIME);
        self.emitln("");
        self.out.push_str(CORE_RUNTIME);
        self.emitln("");
        self.out.push_str(DATETIME_RUNTIME);
        self.emitln("");
        self.out.push_str(UUID_RUNTIME);
        self.emitln("");
    }

    /// Find an IR function by name
    /// Check if an IR expression tree contains any float values
    fn ir_expr_contains_float(&self, expr: &almide::ir::IrExpr) -> bool {
        use almide::ir::IrExprKind;
        match &expr.kind {
            IrExprKind::LitFloat { .. } => true,
            IrExprKind::BinOp { left, right, .. } => {
                self.ir_expr_contains_float(left) || self.ir_expr_contains_float(right)
            }
            IrExprKind::UnOp { operand, .. } => self.ir_expr_contains_float(operand),
            _ => false,
        }
    }

    /// Check if a function body has self-recursive tail calls.
    fn has_tail_self_call(fn_name: &str, expr: &almide::ir::IrExpr) -> bool {
        use almide::ir::{IrExprKind, CallTarget};
        match &expr.kind {
            // Direct tail call
            IrExprKind::Call { target: CallTarget::Named { name }, .. } if name == fn_name => true,
            // Tail position in if/else branches
            IrExprKind::If { then, else_, .. } => {
                Self::has_tail_self_call(fn_name, then) || Self::has_tail_self_call(fn_name, else_)
            }
            // Tail position in match arms
            IrExprKind::Match { arms, .. } => {
                arms.iter().any(|arm| Self::has_tail_self_call(fn_name, &arm.body))
            }
            // Tail position in block (last expr)
            IrExprKind::Block { expr: Some(e), .. } => Self::has_tail_self_call(fn_name, e),
            _ => false,
        }
    }

    /// Emit TCO-transformed function body: wraps in loop, replaces tail calls with continue.
    fn emit_ir_fn_body_tco(&mut self, body: &almide::ir::IrExpr, is_effect: bool, ret_str: &str, _is_unit_ret: bool, fn_name: &str, params: &[almide::ir::IrParam]) {
        let param_names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
        self.emitln("'_tco: loop {");
        self.indent += 1;
        self.emit_tco_expr(body, is_effect, ret_str, fn_name, &param_names);
        self.indent -= 1;
        self.emitln("}");
    }

    /// Emit an expression in TCO context — tail calls become param reassignment + continue.
    fn emit_tco_expr(&mut self, expr: &almide::ir::IrExpr, is_effect: bool, ret_str: &str, fn_name: &str, params: &[String]) {
        use almide::ir::{IrExprKind, CallTarget};
        match &expr.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if name == fn_name => {
                // Tail self-call → reassign params + continue
                // Use temporaries to prevent aliasing
                for (i, arg) in args.iter().enumerate() {
                    let tmp = self.gen_ir_expr(arg);
                    self.emitln(&format!("let _tco_tmp_{} = {};", i, tmp));
                }
                for (i, param) in params.iter().enumerate() {
                    self.emitln(&format!("{} = _tco_tmp_{};", param, i));
                }
                self.emitln("continue '_tco;");
            }
            IrExprKind::If { cond, then, else_ } => {
                let c = self.gen_ir_expr(cond);
                self.emitln(&format!("if {} {{", c));
                self.indent += 1;
                self.emit_tco_expr(then, is_effect, ret_str, fn_name, params);
                self.indent -= 1;
                self.emitln("} else {");
                self.indent += 1;
                self.emit_tco_expr(else_, is_effect, ret_str, fn_name, params);
                self.indent -= 1;
                self.emitln("}");
            }
            IrExprKind::Match { subject, arms } => {
                let s = self.gen_ir_expr(subject);
                self.emitln(&format!("match {} {{", s));
                self.indent += 1;
                for arm in arms {
                    let pat = self.gen_ir_pattern(&arm.pattern);
                    if let Some(ref g) = arm.guard {
                        let guard = self.gen_ir_expr(g);
                        self.emitln(&format!("{} if {} => {{", pat, guard));
                    } else {
                        self.emitln(&format!("{} => {{", pat));
                    }
                    self.indent += 1;
                    self.emit_tco_expr(&arm.body, is_effect, ret_str, fn_name, params);
                    self.indent -= 1;
                    self.emitln("}");
                }
                self.indent -= 1;
                self.emitln("}");
            }
            IrExprKind::Block { stmts, expr: final_expr } => {
                for stmt in stmts {
                    let s = self.gen_ir_stmt(stmt);
                    self.emitln(&s);
                }
                if let Some(fe) = final_expr {
                    self.emit_tco_expr(fe, is_effect, ret_str, fn_name, params);
                }
            }
            // Non-tail expression → emit as return
            _ => {
                let e = self.gen_ir_expr(expr);
                let ret_is_result = ret_str.starts_with("Result<");
                if is_effect && ret_is_result {
                    let already_result = matches!(&expr.kind,
                        IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                    );
                    if already_result {
                        self.emitln(&format!("return {};", e));
                    } else {
                        self.emitln(&format!("return Ok({});", e));
                    }
                } else {
                    self.emitln(&format!("return {};", e));
                }
            }
        }
    }

    /// Emit function body from IR
    fn emit_ir_fn_body(&mut self, body: &almide::ir::IrExpr, is_effect: bool, ret_str: &str, is_unit_ret: bool) {
        use almide::ir::IrExprKind;
        match &body.kind {
            IrExprKind::Block { stmts, expr: final_expr } => {
                for stmt in stmts {
                    let s = self.gen_ir_stmt(stmt);
                    self.emitln(&s);
                }
                let ret_is_result = ret_str.starts_with("Result<");
                if is_effect {
                    if ret_is_result {
                        if let Some(fe) = final_expr {
                            let already_result = matches!(&fe.kind,
                                IrExprKind::ResultOk { .. } | IrExprKind::ResultErr { .. }
                                | IrExprKind::Match { .. } | IrExprKind::If { .. }
                            );
                            let e = self.gen_ir_expr(fe);
                            if already_result {
                                self.emitln(&e);
                            } else {
                                self.emitln(&format!("Ok({})", e));
                            }
                        } else {
                            self.emitln("Ok(())");
                        }
                    } else if let Some(fe) = final_expr {
                        if is_unit_ret {
                            let e = self.gen_ir_expr(fe);
                            self.emitln(&format!("{};", e));
                            self.emitln("Ok(())");
                        } else {
                            let e = self.gen_ir_expr(fe);
                            self.emitln(&format!("Ok({})", e));
                        }
                    } else {
                        self.emitln("Ok(())");
                    }
                } else {
                    if let Some(fe) = final_expr {
                        let e = self.gen_ir_expr(fe);
                        self.emitln(&e);
                    }
                }
            }
            _ => {
                let expr = self.gen_ir_expr(body);
                if is_effect {
                    let body_is_result = matches!(&body.ty, almide::types::Ty::Result(_, _));
                    if body_is_result {
                        self.emitln(&expr);
                    } else {
                        self.emitln(&format!("Ok({})", expr));
                    }
                } else {
                    self.emitln(&expr);
                }
            }
        }
    }

    /// Infer Rust const type from a constant expression (for top-level let without annotation).
    /// Convert an owned Rust type to its borrowed equivalent.
    fn to_borrow_type(ty: &str) -> String {
        if ty == "String" {
            "&str".to_string()
        } else if ty.starts_with("Vec<") {
            format!("&[{}]", &ty[4..ty.len()-1])
        } else if ty.starts_with("HashMap<") {
            format!("&{}", ty)
        } else {
            ty.to_string()
        }
    }

    /// Convert a borrowed value to owned: &str → .to_owned(), &[T] → .to_vec(), &HashMap → .clone()
    pub(crate) fn borrow_to_owned(expr: &str, borrow_ty: &str) -> String {
        if borrow_ty == "&str" {
            format!("{}.to_owned()", expr)
        } else if borrow_ty.starts_with("&[") {
            format!("{}.to_vec()", expr)
        } else if borrow_ty.starts_with("&HashMap") {
            format!("{}.clone()", expr)
        } else {
            format!("{}.clone()", expr)
        }
    }

    // ── Phase 5: IR-only codegen methods ─────────────────────────────

    fn ir_vis_prefix(vis: almide::ir::IrVisibility) -> &'static str {
        match vis {
            almide::ir::IrVisibility::Public => "pub ",
            almide::ir::IrVisibility::Mod => "pub(crate) ",
            almide::ir::IrVisibility::Private => "",
        }
    }

    /// Check if any generic parameter has a structural bound (e.g., `T: { name: String, .. }`).
    fn has_structural_bounds(generics: Option<&Vec<crate::ast::GenericParam>>) -> bool {
        match generics {
            Some(gs) => gs.iter().any(|g| g.structural_bound.is_some()),
            None => false,
        }
    }

    fn ir_generic_str(generics: Option<&Vec<crate::ast::GenericParam>>) -> String {
        match generics {
            Some(gs) if !gs.is_empty() => {
                let gparams: Vec<String> = gs.iter().map(|g| {
                    format!("{}: Clone + std::fmt::Debug + PartialEq + PartialOrd", g.name)
                }).collect();
                format!("<{}>", gparams.join(", "))
            }
            _ => String::new(),
        }
    }

    fn ir_generic_names(generics: Option<&Vec<crate::ast::GenericParam>>) -> String {
        match generics {
            Some(gs) if !gs.is_empty() => {
                let names: Vec<String> = gs.iter().map(|g| g.name.clone()).collect();
                format!("<{}>", names.join(", "))
            }
            _ => String::new(),
        }
    }

    /// Check if a Ty references a given type name (for detecting recursive variants in IR).
    fn ty_references_name(ty: &almide::types::Ty, target: &str) -> bool {
        use almide::types::Ty;
        match ty {
            Ty::Named(name, args) => name == target || args.iter().any(|a| Self::ty_references_name(a, target)),
            Ty::List(inner) | Ty::Option(inner) => Self::ty_references_name(inner, target),
            Ty::Result(a, b) | Ty::Map(a, b) => Self::ty_references_name(a, target) || Self::ty_references_name(b, target),
            Ty::Tuple(elems) => elems.iter().any(|e| Self::ty_references_name(e, target)),
            Ty::Fn { params, ret } => params.iter().any(|p| Self::ty_references_name(p, target)) || Self::ty_references_name(ret, target),
            Ty::Record { fields } => fields.iter().any(|(_, fty)| Self::ty_references_name(fty, target)),
            _ => false,
        }
    }

    /// Generate a Rust type string from Ty, wrapping with Box if it references the given recursive type name.
    fn ir_ty_to_rust_boxed(&self, ty: &almide::types::Ty, recursive_name: &str) -> String {
        if Self::ty_references_name(ty, recursive_name) {
            format!("Box<{}>", self.ir_ty_to_rust_sig(ty))
        } else {
            self.ir_ty_to_rust_sig(ty)
        }
    }

    /// Emit a type declaration from IrTypeDecl (no AST needed).
    fn emit_ir_type_decl(&mut self, td: &almide::ir::IrTypeDecl) {
        self.emit_ir_type_decl_vis(td, Self::ir_vis_prefix(td.visibility));
    }

    fn emit_ir_type_decl_vis(&mut self, td: &almide::ir::IrTypeDecl, vis: &str) {
        use almide::ir::{IrTypeDeclKind, IrVariantKind};
        let generic_str = Self::ir_generic_str(td.generics.as_ref());
        let generic_names = Self::ir_generic_names(td.generics.as_ref());

        match &td.kind {
            IrTypeDeclKind::Record { fields } => {
                self.emitln("#[derive(Debug, Clone, PartialEq)]");
                self.emitln(&format!("{}struct {}{} {{", vis, td.name, generic_str));
                self.indent += 1;
                let field_vis = if vis.is_empty() { "" } else { "pub " };
                for f in fields {
                    let ty_str = self.ir_ty_to_rust_sig(&f.ty);
                    self.emitln(&format!("{}{}: {},", field_vis, f.name, ty_str));
                }
                self.indent -= 1;
                self.emitln("}");
            }
            IrTypeDeclKind::Alias { target } => {
                // Check if alias target is an open record (field-based struct)
                if let almide::types::Ty::OpenRecord { fields } = target {
                    let field_names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                    let struct_name = self.fresh_anon_record_name(&field_names);
                    let type_args: Vec<String> = fields.iter().map(|(_, t)| self.ir_ty_to_rust(t)).collect();
                    if type_args.is_empty() {
                        self.emitln(&format!("{}type {}{} = {};", vis, td.name, generic_str, struct_name));
                    } else {
                        self.emitln(&format!("{}type {}{} = {}<{}>;", vis, td.name, generic_str, struct_name, type_args.join(", ")));
                    }
                } else {
                    let ty_str = self.ir_ty_to_rust(target);
                    self.emitln(&format!("{}type {}{} = {};", vis, td.name, generic_str, ty_str));
                }
            }
            IrTypeDeclKind::Variant { cases, boxed_record_fields: _, .. } => {
                let is_recursive = cases.iter().any(|c| match &c.kind {
                    IrVariantKind::Tuple { fields } => fields.iter().any(|f| Self::ty_references_name(f, &td.name)),
                    IrVariantKind::Record { fields } => fields.iter().any(|f| Self::ty_references_name(&f.ty, &td.name)),
                    _ => false,
                });

                self.emitln("#[derive(Debug, Clone, PartialEq)]");
                self.emitln(&format!("{}enum {}{} {{", vis, td.name, generic_str));
                self.indent += 1;
                for case in cases {
                    match &case.kind {
                        IrVariantKind::Unit => {
                            self.emitln(&format!("{},", case.name));
                        }
                        IrVariantKind::Tuple { fields } => {
                            let fs: Vec<String> = fields.iter().map(|f| {
                                if is_recursive { self.ir_ty_to_rust_boxed(f, &td.name) } else { self.ir_ty_to_rust_sig(f) }
                            }).collect();
                            self.emitln(&format!("{}({}),", case.name, fs.join(", ")));
                        }
                        IrVariantKind::Record { fields } => {
                            let fs: Vec<String> = fields.iter().map(|f| {
                                let ty_str = if is_recursive { self.ir_ty_to_rust_boxed(&f.ty, &td.name) } else { self.ir_ty_to_rust_sig(&f.ty) };
                                format!("{}: {}", f.name, ty_str)
                            }).collect();
                            self.emitln(&format!("{} {{ {} }},", case.name, fs.join(", ")));
                        }
                    }
                }
                self.indent -= 1;
                self.emitln("}");

                // impl Display
                self.emitln(&format!("impl{} std::fmt::Display for {}{} {{", generic_str, td.name, generic_names));
                self.emitln(&format!("    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {{ write!(f, \"{{:?}}\", self) }}"));
                self.emitln("}");

                if generic_str.is_empty() {
                    self.emitln(&format!("use {}::*;", td.name));
                } else {
                    // Generic enum constructor wrappers
                    for case in cases {
                        match &case.kind {
                            IrVariantKind::Unit => {
                                self.emitln("#[allow(non_snake_case)]");
                                self.emitln(&format!("fn {}{}() -> {}{} {{ {}::{} }}", case.name, generic_str, td.name, generic_names, td.name, case.name));
                            }
                            IrVariantKind::Tuple { fields } => {
                                let params: Vec<String> = fields.iter().enumerate()
                                    .map(|(i, f)| format!("_{}: {}", i, self.ir_ty_to_rust_sig(f)))
                                    .collect();
                                let args: Vec<String> = fields.iter().enumerate().map(|(i, f)| {
                                    if is_recursive && Self::ty_references_name(f, &td.name) {
                                        format!("Box::new(_{})", i)
                                    } else {
                                        format!("_{}", i)
                                    }
                                }).collect();
                                self.emitln("#[allow(non_snake_case)]");
                                self.emitln(&format!("fn {}{}({}) -> {}{} {{ {}::{}({}) }}", case.name, generic_str, params.join(", "), td.name, generic_names, td.name, case.name, args.join(", ")));
                            }
                            IrVariantKind::Record { fields } => {
                                let params: Vec<String> = fields.iter()
                                    .map(|f| format!("{}: {}", f.name, self.ir_ty_to_rust_sig(&f.ty)))
                                    .collect();
                                let args: Vec<String> = fields.iter()
                                    .map(|f| {
                                        if is_recursive && Self::ty_references_name(&f.ty, &td.name) {
                                            format!("{}: Box::new({})", f.name, f.name)
                                        } else {
                                            f.name.clone()
                                        }
                                    })
                                    .collect();
                                self.emitln("#[allow(non_snake_case)]");
                                self.emitln(&format!("fn {}{}({}) -> {}{} {{ {}::{} {{ {} }} }}", case.name, generic_str, params.join(", "), td.name, generic_names, td.name, case.name, args.join(", ")));
                            }
                        }
                    }
                }
                // deriving From
                if td.deriving.as_ref().map_or(false, |d| d.iter().any(|s| s == "From")) {
                    for case in cases {
                        if let IrVariantKind::Tuple { fields } = &case.kind {
                            if fields.len() == 1 {
                                let inner_ty = self.ir_ty_to_rust_sig(&fields[0]);
                                self.emitln(&format!("impl From<{}> for {} {{", inner_ty, td.name));
                                self.emitln(&format!("    fn from(e: {}) -> Self {{ {}::{}(e) }}", inner_ty, td.name, case.name));
                                self.emitln("}");
                            }
                        }
                    }
                }
            }
        }
    }

    /// Emit a function declaration from IrFunction (no AST needed).
    fn emit_ir_fn_decl(&mut self, ir_fn: &almide::ir::IrFunction) {
        let fn_name = if ir_fn.name == "main" { "almide_main".to_string() } else { crate::emit_common::sanitize(&ir_fn.name) };
        // Use ret_ty, falling back to body's type when ret_ty is Unknown
        let effective_ret_ty = if matches!(&ir_fn.ret_ty, almide::types::Ty::Unknown) {
            &ir_fn.body.ty
        } else {
            &ir_fn.ret_ty
        };
        let ret_str = self.ir_ty_to_rust_sig(effective_ret_ty);
        let is_unit_ret = ret_str == "()";

        let actual_ret = if ir_fn.is_effect {
            if ret_str.starts_with("Result<") {
                ret_str.clone()
            } else if is_unit_ret {
                "Result<(), String>".to_string()
            } else {
                format!("Result<{}, String>", ret_str)
            }
        } else {
            ret_str.clone()
        };

        self.borrowed_params.clear();
        // Track open record params for call-site projection
        let mut fn_open_records: Vec<(usize, String, Vec<super::OpenFieldInfo>)> = Vec::new();
        let params_str: Vec<String> = ir_fn.params.iter().enumerate()
            .filter(|(_, p)| p.name != "self")
            .map(|(i, p)| {
                // Open record params via IR Ty
                if let almide::types::Ty::OpenRecord { fields } = &p.ty {
                    let field_infos = self.build_open_field_infos_from_ty(fields);
                    let field_names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                    let struct_name = self.fresh_anon_record_name(&field_names);
                    let type_args: Vec<String> = fields.iter().map(|(_, t)| self.ir_ty_to_rust(t)).collect();
                    fn_open_records.push((i, struct_name.clone(), field_infos));
                    let ty = if type_args.is_empty() {
                        struct_name
                    } else {
                        format!("{}<{}>", struct_name, type_args.join(", "))
                    };
                    return format!("{}: {}", p.name, ty);
                }
                // Check open_record_aliases for shape alias types
                if let almide::types::Ty::Named(alias_name, _) = &p.ty {
                    if let Some(fields) = self.open_record_aliases.get(alias_name).cloned() {
                        let field_infos = self.build_open_field_infos_from_ty(&fields);
                        let field_names: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
                        let struct_name = self.fresh_anon_record_name(&field_names);
                        let type_args: Vec<String> = fields.iter().map(|(_, t)| self.ir_ty_to_rust(t)).collect();
                        fn_open_records.push((i, struct_name.clone(), field_infos));
                        let ty = if type_args.is_empty() {
                            struct_name
                        } else {
                            format!("{}<{}>", struct_name, type_args.join(", "))
                        };
                        return format!("{}: {}", p.name, ty);
                    }
                }
                let ty = self.ir_ty_to_rust_sig(&p.ty);
                let ownership = self.borrow_info.param_ownership(&ir_fn.name, i);
                let ty = if ownership == super::borrow::ParamOwnership::Borrow {
                    let borrowed = Self::to_borrow_type(&ty);
                    self.borrowed_params.insert(p.name.clone(), borrowed.clone());
                    borrowed
                } else {
                    ty
                };
                format!("{}: {}", p.name, ty)
            })
            .collect();
        if !fn_open_records.is_empty() {
            self.open_record_params.insert(ir_fn.name.clone(), fn_open_records);
        }

        // TCO detection (before signature emission so we can add `mut` to params)
        let use_tco = Self::has_tail_self_call(&ir_fn.name, &ir_fn.body);
        let final_params_str: Vec<String> = if use_tco {
            params_str.iter().map(|p| {
                // Add `mut` prefix to each param for TCO reassignment
                if p.starts_with("mut ") { p.clone() } else { format!("mut {}", p) }
            }).collect()
        } else {
            params_str
        };

        let generic_str = Self::ir_generic_str(ir_fn.generics.as_ref());
        let async_prefix = if ir_fn.is_async { "async " } else { "" };
        self.emitln(&format!("{}fn {}{}({}) -> {} {{", async_prefix, fn_name, generic_str, final_params_str.join(", "), actual_ret));
        self.indent += 1;

        // Check for @extern(rs, ...)
        let rs_extern = ir_fn.extern_attrs.iter().find(|a| a.target == "rs");
        if let Some(ext) = rs_extern {
            let args: Vec<String> = ir_fn.params.iter()
                .filter(|p| p.name != "self")
                .map(|p| format!("{}.clone()", p.name))
                .collect();
            let call = format!("{}::{}({})", ext.module.replace('.', "::"), ext.function, args.join(", "));
            if ir_fn.is_effect {
                self.emitln(&format!("{}?", call));
            } else {
                self.emitln(&call);
            }
        } else {
            self.analyze_ir_single_use(&ir_fn.body, &ir_fn.params);
            let prev_effect = self.in_effect;
            self.in_effect = ir_fn.is_effect || ret_str.starts_with("Result<");
            if use_tco {
                self.emit_ir_fn_body_tco(&ir_fn.body, ir_fn.is_effect, &actual_ret, is_unit_ret, &ir_fn.name, &ir_fn.params);
            } else {
                self.emit_ir_fn_body(&ir_fn.body, ir_fn.is_effect, &actual_ret, is_unit_ret);
            }
            self.in_effect = prev_effect;
        }

        self.indent -= 1;
        self.emitln("}");
    }

    /// Emit a function declaration from IrFunction with visibility (for modules).
    fn emit_ir_fn_decl_vis(&mut self, ir_fn: &almide::ir::IrFunction, module_name: &str) {
        let fn_name = crate::emit_common::sanitize(&ir_fn.name);
        // Use ret_ty, falling back to body's type when ret_ty is Unknown
        let effective_ret_ty = if matches!(&ir_fn.ret_ty, almide::types::Ty::Unknown) {
            &ir_fn.body.ty
        } else {
            &ir_fn.ret_ty
        };
        let ret_str = self.ir_ty_to_rust_sig(effective_ret_ty);
        let is_unit_ret = ret_str == "()" || ret_str == "Result<(), String>";

        let actual_ret = if ir_fn.is_effect && !ret_str.starts_with("Result<") {
            if ret_str == "()" {
                "Result<(), String>".to_string()
            } else {
                format!("Result<{}, String>", ret_str)
            }
        } else {
            ret_str.clone()
        };

        let qualified = format!("{}.{}", module_name, ir_fn.name);
        self.borrowed_params.clear();
        let params_str: Vec<String> = ir_fn.params.iter().enumerate()
            .map(|(i, p)| {
                let ty = self.ir_ty_to_rust_sig(&p.ty);
                let ownership = self.borrow_info.param_ownership(&qualified, i);
                let ty = if ownership == super::borrow::ParamOwnership::Borrow {
                    let borrowed = Self::to_borrow_type(&ty);
                    self.borrowed_params.insert(p.name.clone(), borrowed.clone());
                    borrowed
                } else {
                    ty
                };
                format!("{}: {}", p.name, ty)
            })
            .collect();

        let vis = Self::ir_vis_prefix(ir_fn.visibility);
        let generic_str = Self::ir_generic_str(ir_fn.generics.as_ref());
        let async_prefix = if ir_fn.is_async { &format!("{}async ", vis) } else { vis };
        self.emitln(&format!("{}fn {}{}({}) -> {} {{", async_prefix, fn_name, generic_str, params_str.join(", "), actual_ret));
        self.indent += 1;

        let rs_extern = ir_fn.extern_attrs.iter().find(|a| a.target == "rs");
        if let Some(ext) = rs_extern {
            let args: Vec<String> = ir_fn.params.iter().map(|p| format!("{}.clone()", p.name)).collect();
            let call = format!("{}::{}({})", ext.module.replace('.', "::"), ext.function, args.join(", "));
            if ir_fn.is_effect {
                self.emitln(&format!("{}?", call));
            } else {
                self.emitln(&call);
            }
        } else {
            // Switch VarTable to the module's VarTable for codegen
            let saved_vt = self.current_var_table;
            if let Some(m) = self.ir_program.modules.iter().find(|m| m.name == module_name) {
                self.current_var_table = &m.var_table;
            } else if let Some(mod_ir) = self.module_irs.get(module_name) {
                self.current_var_table = &mod_ir.var_table;
            }
            self.analyze_ir_single_use(&ir_fn.body, &ir_fn.params);
            let prev_effect = self.in_effect;
            self.in_effect = ir_fn.is_effect || ret_str.starts_with("Result<");
            self.emit_ir_fn_body(&ir_fn.body, ir_fn.is_effect, &actual_ret, is_unit_ret);
            self.in_effect = prev_effect;
            self.current_var_table = saved_vt;
        }

        self.indent -= 1;
        self.emitln("}");
        self.emitln("");
    }

    /// Emit a top-level let from IrTopLet (no AST needed).
    fn emit_ir_top_let(&mut self, ir_tl: &almide::ir::IrTopLet) {
        let name = self.ir_var_table().get(ir_tl.var).name.clone();
        let ty_str = {
            let inferred = self.ir_ty_to_rust_sig(&ir_tl.ty);
            if inferred == "i64" && self.ir_expr_contains_float(&ir_tl.value) {
                "f64".to_string()
            } else {
                inferred
            }
        };
        let val_str = self.gen_ir_expr(&ir_tl.value);
        let use_lazy = ty_str == "String" || ir_tl.kind == almide::ir::TopLetKind::Lazy;
        if use_lazy {
            self.top_let_names.insert(name.clone(), true);
            self.emitln(&format!("static {}: std::sync::LazyLock<{}> = std::sync::LazyLock::new(|| {});", name, ty_str, val_str));
        } else {
            self.emitln(&format!("const {}: {} = {};", name, ty_str, val_str));
        }
    }

    /// Emit a test from IrFunction (no AST needed).
    fn emit_ir_test(&mut self, ir_fn: &almide::ir::IrFunction) {
        self.emitln("#[test]");
        let safe_name = ir_fn.name.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect::<String>();
        let ir_fn = ir_fn.clone();
        self.analyze_ir_single_use(&ir_fn.body, &ir_fn.params);
        self.borrowed_params.clear();
        let prev_effect = self.in_effect;
        let prev_test = self.in_test;
        self.in_effect = true;
        self.in_test = true;
        let expr = self.gen_ir_expr(&ir_fn.body);
        let has_question = expr.contains("?");
        if has_question {
            self.emitln(&format!("fn test_{}() -> Result<(), String> {{", safe_name));
            self.indent += 1;
            self.emitln(&format!("{};", expr));
            self.emitln("Ok(())");
        } else {
            self.emitln(&format!("fn test_{}() {{", safe_name));
            self.indent += 1;
            self.emitln(&format!("{};", expr));
        }
        self.in_effect = prev_effect;
        self.in_test = prev_test;
        self.indent -= 1;
        self.emitln("}");
    }

    /// Emit a user module from IrModule (no AST Program needed).
    fn emit_ir_user_module(&mut self, ir_mod: &almide::ir::IrModule, module_variant_types: &[(String, Vec<String>)], module_record_types: &[(String, Vec<String>)]) {
        let mod_name = ir_mod.versioned_name.as_ref().unwrap_or(&ir_mod.name);
        let prev_module = self.current_module.take();
        self.current_module = Some(ir_mod.name.clone());
        let rust_mod_name = mod_name.replace('.', "_");
        self.emitln(&format!("mod {} {{", rust_mod_name));
        self.indent += 1;
        self.emitln("use super::*;");
        // Import variant types from other user modules
        for (other_mod, variant_names) in module_variant_types {
            if other_mod == &rust_mod_name { continue; }
            for vname in variant_names {
                self.emitln(&format!("use super::{}::{};", other_mod, vname));
                self.emitln(&format!("use super::{}::{}::*;", other_mod, vname));
            }
        }
        // Import record types from other user modules
        for (other_mod, record_names) in module_record_types {
            if other_mod == &rust_mod_name { continue; }
            for rname in record_names {
                self.emitln(&format!("use super::{}::{};", other_mod, rname));
            }
        }
        self.emitln("");

        // Emit functions
        for func in &ir_mod.functions {
            self.emit_ir_fn_decl_vis(func, &ir_mod.name);
        }

        // Emit type declarations
        for td in &ir_mod.type_decls {
            let vis = Self::ir_vis_prefix(td.visibility);
            self.emit_ir_type_decl_vis(td, vis);
            self.emitln("");
        }

        self.indent -= 1;
        self.emitln("}");
        self.current_module = prev_module;
    }

    /// Build OpenFieldInfo from IR Ty fields (for open record params).
    fn build_open_field_infos_from_ty(&self, fields: &[(String, almide::types::Ty)]) -> Vec<super::OpenFieldInfo> {
        fields.iter().map(|(name, ty)| {
            let nested = if let almide::types::Ty::OpenRecord { fields: inner } = ty {
                let inner_infos = self.build_open_field_infos_from_ty(inner);
                let inner_names: Vec<String> = inner.iter().map(|(n, _)| n.clone()).collect();
                let struct_name = self.fresh_anon_record_name(&inner_names);
                Some((struct_name, inner_infos))
            } else {
                None
            };
            super::OpenFieldInfo { name: name.clone(), nested }
        }).collect()
    }

}
