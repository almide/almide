use crate::ast::*;
use super::Emitter;
use super::JSON_RUNTIME;
use super::HTTP_RUNTIME;
use super::TIME_RUNTIME;
use super::REGEX_RUNTIME;
use super::IO_RUNTIME;
use super::PLATFORM_RUNTIME;
use super::COLLECTION_RUNTIME;
use super::CORE_RUNTIME;

impl Emitter {
    /// Scan declarations to classify effect/result functions (single pass).
    fn collect_fn_info(&mut self, decls: &[Decl]) {
        for decl in decls {
            if let Decl::Fn { name, effect, return_type, .. } = decl {
                let is_effect = effect.unwrap_or(false);
                let ret_str = self.gen_type(return_type);
                let returns_result = ret_str.starts_with("Result<");
                if is_effect {
                    self.effect_fns.push(name.clone());
                }
                if returns_result || is_effect {
                    self.result_fns.push(name.clone());
                }
            }
        }
    }

    /// Pre-collect named record types so anonymous record literals can use the correct struct name.
    fn collect_named_records(&mut self, decls: &[Decl]) {
        for decl in decls {
            match decl {
                Decl::Type { name, ty: TypeExpr::Record { fields }, .. } => {
                    let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                    self.named_record_types.insert(field_names, name.clone());
                }
                Decl::Type { name: enum_name, ty: TypeExpr::Variant { cases }, generics: Some(gs), .. } if !gs.is_empty() => {
                    for case in cases {
                        let ctor_name = match case {
                            VariantCase::Unit { name } => {
                                self.generic_variant_unit_ctors.insert(name.clone());
                                name.clone()
                            }
                            VariantCase::Tuple { name, fields } => {
                                // Track which args are recursive (need Box wrapping)
                                for (i, f) in fields.iter().enumerate() {
                                    if Self::type_references_name(f, enum_name) {
                                        self.boxed_variant_args.insert((name.clone(), i));
                                    }
                                }
                                name.clone()
                            }
                            VariantCase::Record { name, fields } => {
                                for (i, f) in fields.iter().enumerate() {
                                    if Self::type_references_name(&f.ty, enum_name) {
                                        self.boxed_variant_args.insert((name.clone(), i));
                                    }
                                }
                                name.clone()
                            }
                        };
                        self.generic_variant_constructors.insert(ctor_name, enum_name.clone());
                    }
                }
                _ => {}
            }
        }
    }

    /// Check if a TypeExpr references a given type name (for detecting recursive variants).
    fn type_references_name(ty: &TypeExpr, target: &str) -> bool {
        match ty {
            TypeExpr::Simple { name } => name == target,
            TypeExpr::Generic { name, args } => {
                name == target || args.iter().any(|a| Self::type_references_name(a, target))
            }
            TypeExpr::Record { fields } => fields.iter().any(|f| Self::type_references_name(&f.ty, target)),
            TypeExpr::Fn { params, ret } => {
                params.iter().any(|p| Self::type_references_name(p, target))
                    || Self::type_references_name(ret, target)
            }
            TypeExpr::Tuple { elements } => elements.iter().any(|e| Self::type_references_name(e, target)),
            _ => false,
        }
    }

    /// Generate a type string, wrapping with Box if it references the given recursive type name.
    fn gen_type_boxed(&self, ty: &TypeExpr, recursive_name: &str) -> String {
        if Self::type_references_name(ty, recursive_name) {
            format!("Box<{}>", self.gen_type(ty))
        } else {
            self.gen_type(ty)
        }
    }

    pub(crate) fn emit_program(&mut self, prog: &Program, modules: &[(String, Program, Option<crate::project::PkgId>, bool)]) {
        self.collect_fn_info(&prog.decls);
        self.collect_named_records(&prog.decls);
        for (_, mod_prog, _, _) in modules {
            self.collect_fn_info(&mod_prog.decls);
            self.collect_named_records(&mod_prog.decls);
        }
        // Build module_aliases and user_modules from PkgId info
        for (name, _, pkg_id, _) in modules {
            if let Some(pid) = pkg_id {
                let versioned = pid.mod_name();
                self.module_aliases.insert(name.clone(), versioned.clone());
                self.user_modules.push(versioned);
            } else {
                self.user_modules.push(name.clone());
            }
        }

        self.emitln("#![allow(unused_parens, unused_variables, dead_code, unused_imports, unused_mut, unused_must_use)]");
        self.emitln("");
        self.emit_runtime();
        self.emitln("");

        // Placeholder for anonymous record structs — filled after codegen
        let anon_record_placeholder = "/* __ALMD_ANON_RECORDS__ */";
        self.emitln(anon_record_placeholder);
        self.emitln("");

        // Emit imported modules as `mod name { ... }`
        for (mod_name, mod_prog, pkg_id, _) in modules {
            self.emit_user_module(mod_name, mod_prog, pkg_id.as_ref());
            self.emitln("");
        }

        for decl in &prog.decls {
            self.emit_decl(decl);
            self.emitln("");
        }

        let main_decl = prog.decls.iter().find(|d| matches!(d, Decl::Fn { name, .. } if name == "main"));
        if let Some(Decl::Fn { params, effect, return_type, .. }) = main_decl {
            let has_args = !params.is_empty();
            let is_effect = effect.unwrap_or(false);
            let ret_str = self.gen_type(return_type);
            let returns_result = ret_str.starts_with("Result<") || is_effect;

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
            self.emitln("eprintln!(\"{}\", e);");
            self.emitln("std::process::exit(1);");
            self.indent -= 1;
            self.emitln("}");
        } else {
            self.emitln(&format!("{};", call));
        }
    }

    fn emit_user_module(&mut self, name: &str, prog: &Program, pkg_id: Option<&crate::project::PkgId>) {
        let mod_name = if let Some(pid) = pkg_id {
            pid.mod_name()
        } else {
            name.to_string()
        };
        let rust_mod_name = mod_name.replace('.', "_");
        self.emitln(&format!("mod {} {{", rust_mod_name));
        self.indent += 1;
        self.emitln("use super::*;");
        self.emitln("");

        for decl in &prog.decls {
            match decl {
                Decl::Fn { name: fn_name, params, return_type, body, effect, r#async, visibility, extern_attrs, generics, .. } => {
                    let rs_extern = extern_attrs.iter().find(|a| a.target == "rs");
                    let is_effect = effect.unwrap_or(false);
                    let is_async = r#async.unwrap_or(false);
                    let params_str: Vec<String> = params.iter()
                        .map(|p| format!("{}: {}", p.name, self.gen_type(&p.ty)))
                        .collect();
                    let ret_str = self.gen_type(return_type);

                    let actual_ret = if is_effect && !ret_str.starts_with("Result<") {
                        if ret_str == "()" {
                            "Result<(), String>".to_string()
                        } else {
                            format!("Result<{}, String>", ret_str)
                        }
                    } else {
                        ret_str.clone()
                    };

                    let vis = match visibility {
                        Visibility::Public => "pub ",
                        Visibility::Mod => "pub(crate) ",
                        Visibility::Local => "",
                    };
                    let generic_str = match generics {
                        Some(gs) if !gs.is_empty() => {
                            let gparams: Vec<String> = gs.iter().map(|g| {
                                format!("{}: Clone + std::fmt::Debug + PartialEq + PartialOrd", g.name)
                            }).collect();
                            format!("<{}>", gparams.join(", "))
                        }
                        _ => String::new(),
                    };
                    let async_prefix = if is_async { &format!("{}async ", vis) } else { vis };
                    let safe_fn_name = crate::emit_common::sanitize(fn_name);
                    self.emitln(&format!("{}fn {}{}({}) -> {} {{", async_prefix, safe_fn_name, generic_str, params_str.join(", "), actual_ret));
                    self.indent += 1;

                    if let Some(ext) = rs_extern {
                        let args: Vec<String> = params.iter().map(|p| format!("{}.clone()", p.name)).collect();
                        let call = format!("{}::{}({})", ext.module.replace('.', "::"), ext.function, args.join(", "));
                        if is_effect {
                            self.emitln(&format!("{}?", call));
                        } else {
                            self.emitln(&call);
                        }
                    } else if let Some(body) = body {
                        let prev_effect = self.in_effect;
                        self.in_effect = is_effect;
                        let body_code = self.gen_expr(body);

                        if is_effect {
                            if ret_str.starts_with("Result<") {
                                self.emitln(&body_code);
                            } else if ret_str == "()" {
                                self.emitln(&format!("{};", body_code));
                                self.emitln("Ok(())");
                            } else {
                                self.emitln(&format!("Ok({})", body_code));
                            }
                        } else {
                            self.emitln(&body_code);
                        }
                        self.in_effect = prev_effect;
                    } else {
                        self.emitln("unimplemented!(\"no body and no @extern for rs target\")");
                    }

                    self.indent -= 1;
                    self.emitln("}");
                    self.emitln("");
                }
                Decl::Type { name: type_name, ty, deriving, visibility, generics, .. } => {
                    let vis_prefix = match visibility {
                        Visibility::Public => "pub ",
                        Visibility::Mod => "pub(crate) ",
                        Visibility::Local => "",
                    };
                    self.emit_type_decl_vis(type_name, ty, deriving, vis_prefix, generics.as_ref());
                }
                _ => {}
            }
        }

        self.indent -= 1;
        self.emitln("}");
    }

    fn emit_runtime(&mut self) {
        self.emitln("use std::collections::HashMap;");
        self.emitln("trait AlmideConcat<Rhs> { type Output; fn concat(self, rhs: Rhs) -> Self::Output; }");
        self.emitln("impl AlmideConcat<String> for String { type Output = String; fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }");
        self.emitln("impl AlmideConcat<&str> for String { type Output = String; fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }");
        self.emitln("impl AlmideConcat<String> for &str { type Output = String; fn concat(self, rhs: String) -> String { format!(\"{}{}\", self, rhs) } }");
        self.emitln("impl AlmideConcat<&str> for &str { type Output = String; fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }");
        self.emitln("impl<T: Clone> AlmideConcat<Vec<T>> for Vec<T> { type Output = Vec<T>; fn concat(self, rhs: Vec<T>) -> Vec<T> { let mut r = self; r.extend(rhs); r } }");
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
    }

    pub(crate) fn emit_decl(&mut self, decl: &Decl) {
        match decl {
            Decl::Module { path, .. } => {
                self.emitln(&format!("// module: {}", path.join(".")));
            }
            Decl::Import { path, .. } => {
                self.emitln(&format!("// import: {}", path.join(".")));
            }
            Decl::Type { name, ty, deriving, generics, .. } => {
                self.emit_type_decl(name, ty, deriving, generics.as_ref());
            }
            Decl::Fn { name, params, return_type, body, effect, r#async, extern_attrs, generics, .. } => {
                self.emit_fn_decl(name, params, return_type, body.as_ref(), effect.unwrap_or(false), r#async.unwrap_or(false), extern_attrs, generics.as_ref());
            }
            Decl::Impl { trait_, for_, methods, .. } => {
                self.emitln(&format!("// impl {} for {}", trait_, for_));
                for m in methods {
                    self.emit_decl(m);
                }
            }
            Decl::Test { name, body, .. } => {
                self.emitln("#[test]");
                let safe_name = name.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect::<String>();
                let prev_effect = self.in_effect;
                let prev_test = self.in_test;
                self.in_effect = true;
                self.in_test = true;
                let expr = self.gen_expr(body);
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
            _ => {}
        }
    }

    pub(crate) fn emit_type_decl(&mut self, name: &str, ty: &TypeExpr, deriving: &Option<Vec<String>>, generics: Option<&Vec<crate::ast::GenericParam>>) {
        self.emit_type_decl_vis(name, ty, deriving, "", generics);
    }

    pub(crate) fn emit_type_decl_vis(&mut self, name: &str, ty: &TypeExpr, deriving: &Option<Vec<String>>, vis: &str, generics: Option<&Vec<crate::ast::GenericParam>>) {
        let generic_str = match generics {
            Some(gs) if !gs.is_empty() => {
                let gparams: Vec<String> = gs.iter().map(|g| {
                    format!("{}: Clone + std::fmt::Debug + PartialEq + PartialOrd", g.name)
                }).collect();
                format!("<{}>", gparams.join(", "))
            }
            _ => String::new(),
        };
        // Just the type param names (for impl blocks)
        let generic_names = match generics {
            Some(gs) if !gs.is_empty() => {
                let names: Vec<String> = gs.iter().map(|g| g.name.clone()).collect();
                format!("<{}>", names.join(", "))
            }
            _ => String::new(),
        };
        match ty {
            TypeExpr::Record { fields } => {
                self.emitln("#[derive(Debug, Clone, PartialEq)]");
                self.emitln(&format!("{}struct {}{} {{", vis, name, generic_str));
                self.indent += 1;
                for f in fields {
                    let ty_str = self.gen_type(&f.ty);
                    let field_vis = if vis.is_empty() { "" } else { "pub " };
                    self.emitln(&format!("{}{}: {},", field_vis, f.name, ty_str));
                }
                self.indent -= 1;
                self.emitln("}");
            }
            TypeExpr::Simple { .. } | TypeExpr::Generic { .. } => {
                let ty_str = self.gen_type(ty);
                self.emitln(&format!("{}type {}{} = {};", vis, name, generic_str, ty_str));
            }
            TypeExpr::Newtype { inner } => {
                let ty_str = self.gen_type(inner);
                self.emitln(&format!("{}struct {}{}({});", vis, name, generic_str, ty_str));
            }
            TypeExpr::Variant { cases } => {
                self.emitln("#[derive(Debug, Clone, PartialEq)]");
                self.emitln(&format!("{}enum {}{} {{", vis, name, generic_str));
                self.indent += 1;
                // Detect if this variant has any recursive fields
                let is_recursive = cases.iter().any(|c| match c {
                    VariantCase::Tuple { fields, .. } => fields.iter().any(|f| Self::type_references_name(f, name)),
                    VariantCase::Record { fields, .. } => fields.iter().any(|f| Self::type_references_name(&f.ty, name)),
                    _ => false,
                });
                for case in cases {
                    match case {
                        VariantCase::Unit { name: cname } => {
                            self.emitln(&format!("{},", cname));
                        }
                        VariantCase::Tuple { name: cname, fields } => {
                            let fs: Vec<String> = fields.iter().map(|f| {
                                if is_recursive { self.gen_type_boxed(f, name) } else { self.gen_type(f) }
                            }).collect();
                            self.emitln(&format!("{}({}),", cname, fs.join(", ")));
                        }
                        VariantCase::Record { name: cname, fields } => {
                            let fs: Vec<String> = fields.iter().map(|f| {
                                let ty_str = if is_recursive { self.gen_type_boxed(&f.ty, name) } else { self.gen_type(&f.ty) };
                                format!("{}: {}", f.name, ty_str)
                            }).collect();
                            self.emitln(&format!("{} {{ {} }},", cname, fs.join(", ")));
                        }
                    }
                }
                self.indent -= 1;
                self.emitln("}");
                // impl Display for error types (so they work with .to_string())
                self.emitln(&format!("impl{} std::fmt::Display for {}{} {{", generic_str, name, generic_names));
                self.emitln(&format!("    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {{ write!(f, \"{{:?}}\", self) }}"));
                self.emitln("}");
                // Allow using variant names without prefix
                if generic_str.is_empty() {
                    self.emitln(&format!("use {}::*;", name));
                } else {
                    // For generic enums, generate constructor wrapper functions
                    for case in cases {
                        match case {
                            VariantCase::Unit { name: cname } => {
                                self.emitln(&format!("#[allow(non_snake_case)]"));
                                self.emitln(&format!("fn {}{}() -> {}{} {{ {}::{} }}", cname, generic_str, name, generic_names, name, cname));
                            }
                            VariantCase::Tuple { name: cname, fields } => {
                                let params: Vec<String> = fields.iter().enumerate()
                                    .map(|(i, f)| format!("_{}: {}", i, self.gen_type(f)))
                                    .collect();
                                let args: Vec<String> = fields.iter().enumerate().map(|(i, f)| {
                                    if is_recursive && Self::type_references_name(f, name) {
                                        format!("Box::new(_{})", i)
                                    } else {
                                        format!("_{}", i)
                                    }
                                }).collect();
                                self.emitln(&format!("#[allow(non_snake_case)]"));
                                self.emitln(&format!("fn {}{}({}) -> {}{} {{ {}::{}({}) }}", cname, generic_str, params.join(", "), name, generic_names, name, cname, args.join(", ")));
                            }
                            VariantCase::Record { name: cname, fields } => {
                                let params: Vec<String> = fields.iter()
                                    .map(|f| format!("{}: {}", f.name, self.gen_type(&f.ty)))
                                    .collect();
                                let args: Vec<String> = fields.iter()
                                    .map(|f| {
                                        if is_recursive && Self::type_references_name(&f.ty, name) {
                                            format!("{}: Box::new({})", f.name, f.name)
                                        } else {
                                            f.name.clone()
                                        }
                                    })
                                    .collect();
                                self.emitln(&format!("#[allow(non_snake_case)]"));
                                self.emitln(&format!("fn {}{}({}) -> {}{} {{ {}::{} {{ {} }} }}", cname, generic_str, params.join(", "), name, generic_names, name, cname, args.join(", ")));
                            }
                        }
                    }
                }
                // deriving From: generate impl From<InnerType> for VariantType
                if deriving.as_ref().map_or(false, |d| d.iter().any(|s| s == "From")) {
                    for case in cases {
                        if let VariantCase::Tuple { name: cname, fields } = case {
                            if fields.len() == 1 {
                                let inner_ty = self.gen_type(&fields[0]);
                                self.emitln(&format!("impl From<{}> for {} {{", inner_ty, name));
                                self.emitln(&format!("    fn from(e: {}) -> Self {{ {}::{}(e) }}", inner_ty, name, cname));
                                self.emitln("}");
                            }
                        }
                    }
                }
            }
            _ => {
                self.emitln(&format!("// type {} (unsupported)", name));
            }
        }
    }

    fn emit_fn_decl(&mut self, name: &str, params: &[Param], ret_type: &TypeExpr, body: Option<&Expr>, is_effect: bool, is_async: bool, extern_attrs: &[ExternAttr], generics: Option<&Vec<crate::ast::GenericParam>>) {
        let fn_name = if name == "main" { "almide_main".to_string() } else { crate::emit_common::sanitize(name) };
        let ret_str = self.gen_type(ret_type);
        let is_unit_ret = ret_str == "()";

        let actual_ret = if is_effect {
            if ret_str.starts_with("Result<") {
                // Already a Result type, don't double-wrap
                ret_str.clone()
            } else if is_unit_ret {
                "Result<(), String>".to_string()
            } else {
                format!("Result<{}, String>", ret_str)
            }
        } else {
            ret_str.clone()
        };

        let params_str: Vec<String> = params.iter()
            .filter(|p| p.name != "self")
            .map(|p| {
                let ty = self.gen_type(&p.ty);
                format!("{}: {}", p.name, ty)
            })
            .collect();

        // Generate generic type parameter list
        let generic_str = match generics {
            Some(gs) if !gs.is_empty() => {
                let gparams: Vec<String> = gs.iter().map(|g| {
                    // All generic types need Clone + Debug + PartialEq for Almide runtime compatibility
                    format!("{}: Clone + std::fmt::Debug + PartialEq + PartialOrd", g.name)
                }).collect();
                format!("<{}>", gparams.join(", "))
            }
            _ => String::new(),
        };

        let async_prefix = if is_async { "async " } else { "" };
        self.emitln(&format!("{}fn {}{}({}) -> {} {{", async_prefix, fn_name, generic_str, params_str.join(", "), actual_ret));
        self.indent += 1;

        // Check for @extern(rs, ...)
        let rs_extern = extern_attrs.iter().find(|a| a.target == "rs");

        if let Some(ext) = rs_extern {
            let args: Vec<String> = params.iter()
                .filter(|p| p.name != "self")
                .map(|p| format!("{}.clone()", p.name))
                .collect();
            let call = format!("{}::{}({})", ext.module.replace('.', "::"), ext.function, args.join(", "));
            if is_effect {
                self.emitln(&format!("{}?", call));
            } else {
                self.emitln(&call);
            }
        } else if let Some(body) = body {
            let prev_effect = self.in_effect;
            // Treat fn as effect if explicitly marked OR if it returns Result
            self.in_effect = is_effect || ret_str.starts_with("Result<");

            match body {
                Expr::Block { stmts, expr: final_expr, .. } => {
                    self.emit_stmts(stmts);
                    let ret_is_result = ret_str.starts_with("Result<");
                    if is_effect {
                        if ret_is_result {
                            if let Some(fe) = final_expr {
                                let already_result = matches!(fe.as_ref(),
                                    Expr::Ok { .. } | Expr::Err { .. } | Expr::Match { .. } | Expr::If { .. }
                                );
                                let e = self.gen_expr(fe);
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
                                let e = self.gen_expr(fe);
                                self.emitln(&format!("{};", e));
                                self.emitln("Ok(())");
                            } else {
                                let e = self.gen_expr(fe);
                                self.emitln(&format!("Ok({})", e));
                            }
                        } else {
                            self.emitln("Ok(())");
                        }
                    } else {
                        if let Some(fe) = final_expr {
                            let e = self.gen_expr(fe);
                            self.emitln(&e);
                        }
                    }
                }
                _ => {
                    let expr = self.gen_expr(body);
                    if is_effect {
                        let ret_is_result = ret_str.starts_with("Result<");
                        if ret_is_result {
                            self.emitln(&expr);
                        } else {
                            self.emitln(&format!("Ok({})", expr));
                        }
                    } else {
                        self.emitln(&expr);
                    }
                }
            }

            self.in_effect = prev_effect;
        } else {
            self.emitln("unimplemented!(\"no body and no @extern for rs target\")");
        }

        self.indent -= 1;
        self.emitln("}");
    }

    pub(crate) fn emit_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            let s = self.gen_stmt(stmt);
            self.emitln(&s);
        }
    }

    pub(crate) fn gen_type(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Simple { name } => match name.as_str() {
                "Int" => "i64".to_string(),
                "Float" => "f64".to_string(),
                "String" => "String".to_string(),
                "Bool" => "bool".to_string(),
                "Unit" => "()".to_string(),
                "IoError" => "AlmideIoError".to_string(),
                "Path" => "String".to_string(),
                "Json" => "AlmideJson".to_string(),
                "Request" => "AlmideHttpRequest".to_string(),
                "Response" => "AlmideHttpResponse".to_string(),
                other => other.to_string(),
            },
            TypeExpr::Generic { name, args } => match name.as_str() {
                "List" => format!("Vec<{}>", self.gen_type(&args[0])),
                "Option" => format!("Option<{}>", self.gen_type(&args[0])),
                "Result" if args.len() >= 2 => format!("Result<{}, {}>", self.gen_type(&args[0]), self.gen_type(&args[1])),
                "Result" => format!("Result<{}, String>", self.gen_type(&args[0])),
                "Map" => format!("HashMap<{}, {}>", self.gen_type(&args[0]), self.gen_type(&args[1])),
                other => format!("{}<{}>", other, args.iter().map(|a| self.gen_type(a)).collect::<Vec<_>>().join(", ")),
            },
            TypeExpr::Record { fields } => {
                let fs: Vec<String> = fields.iter().map(|f| format!("{}: {}", f.name, self.gen_type(&f.ty))).collect();
                format!("{{ {} }}", fs.join(", "))
            }
            TypeExpr::Fn { params, ret } => {
                let ps: Vec<String> = params.iter().map(|p| self.gen_type(p)).collect();
                format!("fn({}) -> {}", ps.join(", "), self.gen_type(ret))
            }
            TypeExpr::Tuple { elements } => {
                let ts: Vec<String> = elements.iter().map(|e| self.gen_type(e)).collect();
                format!("({})", ts.join(", "))
            }
            TypeExpr::Newtype { inner } => self.gen_type(inner),
            TypeExpr::Variant { cases: _ } => "/* variant */".to_string(),
        }
    }

    /// Generate expression as function argument — clone Idents to avoid move
    pub(crate) fn gen_arg(&self, expr: &Expr) -> String {
        match expr {
            Expr::Ident { .. } => format!("{}.clone()", self.gen_expr(expr)),
            _ => self.gen_expr(expr),
        }
    }
}
