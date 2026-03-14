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

    /// Scan IR functions to classify effect/result functions (no AST needed).
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

    /// Pre-collect named record types so anonymous record literals can use the correct struct name.
    fn collect_named_records(&mut self, decls: &[Decl]) {
        for decl in decls {
            match decl {
                Decl::Type { name, ty: TypeExpr::Record { fields, .. }, .. } => {
                    let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                    self.named_record_types.insert(field_names, name.clone());
                }
                Decl::Type { name, ty: TypeExpr::OpenRecord { fields }, .. } => {
                    self.open_record_aliases.insert(name.clone(), fields.clone());
                }
                Decl::Type { name: enum_name, ty: TypeExpr::Variant { cases }, generics, .. } => {
                    let has_generics = matches!(generics, Some(gs) if !gs.is_empty());
                    for case in cases {
                        let ctor_name = match case {
                            VariantCase::Unit { name } => {
                                if has_generics {
                                    self.generic_variant_unit_ctors.insert(name.clone());
                                }
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
                                        self.boxed_variant_record_fields.insert((name.clone(), f.name.clone()));
                                    }
                                }
                                name.clone()
                            }
                        };
                        if has_generics {
                            self.generic_variant_constructors.insert(ctor_name, enum_name.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Collect open record aliases from AST (not yet modeled in IR).
    fn collect_open_record_aliases(&mut self, decls: &[Decl]) {
        for decl in decls {
            if let Decl::Type { name, ty: TypeExpr::OpenRecord { fields }, .. } = decl {
                self.open_record_aliases.insert(name.clone(), fields.clone());
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
            TypeExpr::Record { fields } | TypeExpr::OpenRecord { fields } => fields.iter().any(|f| Self::type_references_name(&f.ty, target)),
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
                IrTypeDeclKind::Alias { .. } => {}
            }
        }
    }

    /// Collect top-level let names. When IR is available, pre-classify using TopLetKind;
    /// otherwise the bool (needs_deref) is set later during emit.
    fn collect_top_lets(&mut self, decls: &[Decl]) {
        if let Some(ref ir) = self.ir_program {
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
        } else {
            for decl in decls {
                if let Decl::TopLet { name, .. } = decl {
                    self.top_let_names.insert(name.clone(), false);
                }
            }
        }
    }

    pub(crate) fn emit_program(&mut self, prog: &Program, modules: &[(String, Program, Option<crate::project::PkgId>, bool)]) {
        let use_ir = self.ir_program.is_some();

        // ── Phase 1: Collect metadata ──
        if use_ir {
            let ir_fns = self.ir_program.as_ref().unwrap().functions.clone();
            let ir_tds = self.ir_program.as_ref().unwrap().type_decls.clone();
            self.collect_fn_info_from_ir(&ir_fns);
            self.collect_named_records_from_ir(&ir_tds);
            self.collect_top_lets(&prog.decls);
        } else {
            self.collect_fn_info(&prog.decls);
            self.collect_named_records(&prog.decls);
            self.collect_top_lets(&prog.decls);
        }

        // Always collect from AST for open_record_aliases (not yet modeled in IR type system)
        self.collect_open_record_aliases(&prog.decls);

        // Module metadata
        let has_ir_modules = self.ir_program.as_ref().map_or(false, |ir| !ir.modules.is_empty());
        if has_ir_modules {
            let ir_modules = self.ir_program.as_ref().unwrap().modules.clone();
            for ir_mod in &ir_modules {
                self.collect_fn_info_from_ir(&ir_mod.functions);
                self.collect_named_records_from_ir(&ir_mod.type_decls);
                // Still need AST for open_record_aliases
                if let Some((_, mod_prog, _, _)) = modules.iter().find(|(n, _, _, _)| n == &ir_mod.name) {
                    self.collect_open_record_aliases(&mod_prog.decls);
                }
            }
            for ir_mod in &ir_modules {
                if let Some(ref versioned) = ir_mod.versioned_name {
                    self.module_aliases.insert(ir_mod.name.clone(), versioned.clone());
                    self.user_modules.push(versioned.clone());
                } else {
                    self.user_modules.push(ir_mod.name.clone());
                }
            }
        } else {
            for (mod_name, mod_prog, _, _) in modules {
                if use_ir {
                    if let Some(mod_ir) = self.module_irs.get(mod_name).cloned() {
                        self.collect_fn_info_from_ir(&mod_ir.functions);
                        self.collect_named_records_from_ir(&mod_ir.type_decls);
                    } else {
                        self.collect_fn_info(&mod_prog.decls);
                        self.collect_named_records(&mod_prog.decls);
                    }
                } else {
                    self.collect_fn_info(&mod_prog.decls);
                    self.collect_named_records(&mod_prog.decls);
                }
                self.collect_open_record_aliases(&mod_prog.decls);
            }
            for (name, _, pkg_id, _) in modules {
                if let Some(pid) = pkg_id {
                    let versioned = pid.mod_name();
                    self.module_aliases.insert(name.clone(), versioned.clone());
                    self.user_modules.push(versioned);
                } else {
                    self.user_modules.push(name.clone());
                }
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

        if has_ir_modules {
            let ir_modules = self.ir_program.as_ref().unwrap().modules.clone();
            for ir_mod in &ir_modules {
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
        } else {
            for (mod_name, mod_prog, pkg_id, _) in modules {
                let rust_mod = if let Some(pid) = pkg_id {
                    pid.mod_name().replace('.', "_")
                } else {
                    mod_name.replace('.', "_")
                };
                let mut variant_names = Vec::new();
                let mut record_names = Vec::new();
                for decl in &mod_prog.decls {
                    match decl {
                        Decl::Type { name, ty: TypeExpr::Variant { .. }, .. } => variant_names.push(name.clone()),
                        Decl::Type { name, ty: TypeExpr::Record { .. }, .. } => record_names.push(name.clone()),
                        _ => {}
                    }
                }
                module_variant_types.push((rust_mod.clone(), variant_names));
                module_record_types.push((rust_mod, record_names));
            }
        }

        // ── Phase 4: Emit modules ──
        if has_ir_modules {
            let ir_modules = self.ir_program.as_ref().unwrap().modules.clone();
            for ir_mod in &ir_modules {
                self.emit_ir_user_module(&ir_mod, &module_variant_types, &module_record_types);
                self.emitln("");
            }
        } else {
            for (mod_name, mod_prog, pkg_id, _) in modules {
                self.emit_user_module(mod_name, mod_prog, pkg_id.as_ref(), &module_variant_types, &module_record_types);
                self.emitln("");
            }
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

        // ── Phase 5: Emit declarations ──
        // When IR is available, emit from IR structures. Fall back to AST
        // for functions whose return type couldn't be fully resolved by lowering
        // (e.g., Result<T, Unknown> where the error type was inferred).
        if use_ir {
            let ir = self.ir_program.as_ref().unwrap().clone();

            // Build a lookup from AST decls for fallback when IR has Unknown types
            // (indexed by name for quick lookup)
            let ast_decl_map: std::collections::HashMap<&str, &Decl> = prog.decls.iter()
                .filter_map(|d| match d {
                    Decl::Fn { name, .. } => Some((name.as_str(), d)),
                    Decl::Type { name, .. } => Some((name.as_str(), d)),
                    Decl::TopLet { name, .. } => Some((name.as_str(), d)),
                    Decl::Test { name, .. } => Some((name.as_str(), d)),
                    _ => None,
                })
                .collect();

            // Emit type declarations from IR
            for td in &ir.type_decls {
                self.emit_ir_type_decl(td);
                self.emitln("");
            }

            // Emit top-level lets from IR
            for tl in &ir.top_lets {
                self.emit_ir_top_let(tl);
                self.emitln("");
            }

            // Emit functions from IR, with AST fallback for Unknown ret types
            for func in &ir.functions {
                let has_unknown_ret = Self::ty_contains_unknown(&func.ret_ty);

                if func.is_test {
                    self.emit_ir_test(func);
                } else if has_unknown_ret {
                    // IR lowering doesn't resolve all return types yet.
                    // Fall back to AST emit_decl for these functions.
                    if let Some(decl) = ast_decl_map.get(func.name.as_str()) {
                        self.emit_decl(decl);
                    } else {
                        self.emit_ir_fn_decl(func);
                    }
                } else {
                    self.emit_ir_fn_decl(func);
                }
                self.emitln("");
            }

            // Emit Impl blocks from AST (methods already in IR, just emit comment)
            for decl in &prog.decls {
                match decl {
                    Decl::Module { path, .. } => {
                        self.emitln(&format!("// module: {}", path.join(".")));
                        self.emitln("");
                    }
                    Decl::Import { path, .. } => {
                        self.emitln(&format!("// import: {}", path.join(".")));
                        self.emitln("");
                    }
                    Decl::Impl { trait_, for_, .. } => {
                        self.emitln(&format!("// impl {} for {}", trait_, for_));
                        self.emitln("");
                    }
                    _ => {}
                }
            }

            // Emit main() wrapper from IR
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
        } else {
            // Legacy AST-based path
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

    fn emit_user_module(&mut self, name: &str, prog: &Program, pkg_id: Option<&crate::project::PkgId>, module_variant_types: &[(String, Vec<String>)], module_record_types: &[(String, Vec<String>)]) {
        let mod_name = if let Some(pid) = pkg_id {
            pid.mod_name()
        } else {
            name.to_string()
        };
        let prev_module = self.current_module.take();
        self.current_module = Some(name.to_string());
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

        for decl in &prog.decls {
            match decl {
                Decl::Fn { name: fn_name, params, return_type, effect, r#async, visibility, extern_attrs, generics, .. } => {
                    let rs_extern = extern_attrs.iter().find(|a| a.target == "rs");
                    let is_effect = effect.unwrap_or(false);
                    let is_async = r#async.unwrap_or(false);
                    // Apply borrow inference to module function params
                    let qualified = format!("{}.{}", name, fn_name);
                    self.borrowed_params.clear();
                    let params_str: Vec<String> = params.iter().enumerate()
                        .map(|(i, p)| {
                            let ty = self.gen_type(&p.ty);
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
                    } else if let Some(ir_fn) = self.find_module_ir_function(name, fn_name) {
                        let ir_fn = ir_fn.clone();
                        // Temporarily swap ir_program to module's IR (for VarTable access)
                        let saved_ir = self.ir_program.take();
                        // Try IrModule first, fall back to module_irs
                        let module_ir_prog = saved_ir.as_ref()
                            .and_then(|ir| ir.modules.iter().find(|m| m.name == name))
                            .map(|m| almide::ir::IrProgram {
                                functions: m.functions.clone(),
                                top_lets: m.top_lets.clone(),
                                type_decls: m.type_decls.clone(),
                                var_table: m.var_table.clone(),
                                modules: Vec::new(),
                            })
                            .or_else(|| self.module_irs.get(name).cloned());
                        self.ir_program = module_ir_prog;
                        self.analyze_ir_single_use(&ir_fn.body, &ir_fn.params);
                        let prev_effect = self.in_effect;
                        self.in_effect = is_effect || ret_str.starts_with("Result<");
                        self.emit_ir_fn_body(&ir_fn.body, is_effect, &ret_str, ret_str == "()" || ret_str == "Result<(), String>");
                        self.in_effect = prev_effect;
                        self.ir_program = saved_ir;
                    } else {
                        unreachable!("IR required for codegen");
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
        self.current_module = prev_module;
    }

    fn find_module_ir_function(&self, module_name: &str, fn_name: &str) -> Option<&almide::ir::IrFunction> {
        // Try IrProgram::modules first (Phase 4 path)
        if let Some(ref ir) = self.ir_program {
            for ir_mod in &ir.modules {
                if ir_mod.name == module_name {
                    return ir_mod.functions.iter().find(|f| f.name == fn_name);
                }
            }
        }
        // Fall back to separate module_irs HashMap
        let ir = self.module_irs.get(module_name)?;
        ir.functions.iter().find(|f| f.name == fn_name)
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
                self.emit_fn_decl(name, params, return_type, effect.unwrap_or(false), r#async.unwrap_or(false), extern_attrs, generics.as_ref());
            }
            Decl::TopLet { name, ty, .. } => {
                if let Some(ir_tl) = self.find_ir_top_let(name).cloned() {
                    let ty_str = if let Some(te) = ty {
                        self.gen_type(te)
                    } else {
                        // Infer type from the IR expression when checker type is Int but value is float
                        let inferred = self.ir_ty_to_rust(&ir_tl.ty);
                        // If the value contains float literals/ops, use f64
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
            }
            Decl::Impl { trait_, for_, methods, .. } => {
                self.emitln(&format!("// impl {} for {}", trait_, for_));
                for m in methods {
                    self.emit_decl(m);
                }
            }
            Decl::Test { name, .. } => {
                self.emitln("#[test]");
                let safe_name = name.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect::<String>();
                if let Some(ir_fn) = self.find_ir_function(name) {
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
                } else {
                    self.emitln(&format!("fn test_{}() {{", safe_name));
                    self.indent += 1;
                    unreachable!("IR required for codegen");
                }
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
            TypeExpr::Record { fields, .. } => {
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
            TypeExpr::OpenRecord { fields } => {
                // Shape alias: type Named = { name: String, .. }
                // Emit as a type alias to the generated AlmdRec struct
                let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                let struct_name = self.fresh_anon_record_name(&field_names);
                let type_args: Vec<String> = fields.iter().map(|f| self.gen_type(&f.ty)).collect();
                if type_args.is_empty() {
                    self.emitln(&format!("{}type {}{} = {};", vis, name, generic_str, struct_name));
                } else {
                    self.emitln(&format!("{}type {}{} = {}<{}>;", vis, name, generic_str, struct_name, type_args.join(", ")));
                }
            }
            _ => {
                self.emitln(&format!("// type {} (unsupported)", name));
            }
        }
    }

    fn emit_fn_decl(&mut self, name: &str, params: &[Param], ret_type: &TypeExpr, is_effect: bool, is_async: bool, extern_attrs: &[ExternAttr], generics: Option<&Vec<crate::ast::GenericParam>>) {
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

        // Clear borrowed_params for this function
        self.borrowed_params.clear();
        // Track open record params for call-site projection
        let mut fn_open_records: Vec<(usize, String, Vec<super::OpenFieldInfo>)> = Vec::new();
        let params_str: Vec<String> = params.iter().enumerate()
            .filter(|(_, p)| p.name != "self")
            .map(|(i, p)| {
                // Open record params: detect from AST directly or via shape alias
                let open_fields = match &p.ty {
                    TypeExpr::OpenRecord { fields } => Some(fields.clone()),
                    TypeExpr::Simple { name } => {
                        self.open_record_aliases.get(name).cloned()
                    }
                    _ => None,
                };
                if let Some(fields) = open_fields {
                    let field_infos = self.build_open_field_infos(&fields);
                    let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                    let struct_name = self.fresh_anon_record_name(&field_names);
                    let type_args: Vec<String> = fields.iter().map(|f| self.gen_type(&f.ty)).collect();
                    fn_open_records.push((i, struct_name.clone(), field_infos));
                    let ty = if type_args.is_empty() {
                        struct_name
                    } else {
                        format!("{}<{}>", struct_name, type_args.join(", "))
                    };
                    return format!("{}: {}", p.name, ty);
                }
                let ty = self.gen_type(&p.ty);
                let ownership = self.borrow_info.param_ownership(name, i);
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
            self.open_record_params.insert(name.to_string(), fn_open_records);
        }

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
        } else if let Some(ir_fn) = self.find_ir_function(name) {
            // IR-based codegen path
            let ir_fn = ir_fn.clone();
            self.analyze_ir_single_use(&ir_fn.body, &ir_fn.params);

            let prev_effect = self.in_effect;
            self.in_effect = is_effect || ret_str.starts_with("Result<");

            self.emit_ir_fn_body(&ir_fn.body, is_effect, &ret_str, is_unit_ret);

            self.in_effect = prev_effect;
        } else {
            unreachable!("IR required for codegen");
        }

        self.indent -= 1;
        self.emitln("}");
    }

    /// Find an IR function by name
    fn find_ir_function(&self, name: &str) -> Option<&almide::ir::IrFunction> {
        self.ir_program.as_ref()?.functions.iter().find(|f| f.name == name)
    }

    /// Find an IR top-level let by name
    fn find_ir_top_let(&self, name: &str) -> Option<&almide::ir::IrTopLet> {
        let ir = self.ir_program.as_ref()?;
        ir.top_lets.iter().find(|tl| ir.var_table.get(tl.var).name == name)
    }

    /// Check if an IR expression is a simple const-evaluable literal
    fn ir_expr_is_const(&self, expr: &almide::ir::IrExpr) -> bool {
        use almide::ir::IrExprKind;
        matches!(&expr.kind,
            IrExprKind::LitInt { .. } |
            IrExprKind::LitFloat { .. } |
            IrExprKind::LitBool { .. } |
            IrExprKind::Unit
        )
    }

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
                "JsonPath" => "AlmideJsonPath".to_string(),
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
                let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                let struct_name = self.anon_record_name(&field_names);
                let type_args: Vec<String> = fields.iter().map(|f| self.gen_type(&f.ty)).collect();
                if type_args.is_empty() {
                    struct_name
                } else {
                    format!("{}<{}>", struct_name, type_args.join(", "))
                }
            }
            TypeExpr::OpenRecord { fields } => {
                let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                let struct_name = self.fresh_anon_record_name(&field_names);
                let type_args: Vec<String> = fields.iter().map(|f| self.gen_type(&f.ty)).collect();
                if type_args.is_empty() {
                    struct_name
                } else {
                    format!("{}<{}>", struct_name, type_args.join(", "))
                }
            }
            TypeExpr::Fn { params, ret } => {
                let ps: Vec<String> = params.iter().map(|p| self.gen_type(p)).collect();
                format!("impl Fn({}) -> {} + Clone", ps.join(", "), self.gen_type(ret))
            }
            TypeExpr::Tuple { elements } => {
                let ts: Vec<String> = elements.iter().map(|e| self.gen_type(e)).collect();
                format!("({})", ts.join(", "))
            }
            TypeExpr::Newtype { inner } => self.gen_type(inner),
            TypeExpr::Variant { cases: _ } => "/* variant */".to_string(),
        }
    }

    /// Build OpenFieldInfo recursively for nested open record projection.
    fn build_open_field_infos(&self, fields: &[crate::ast::FieldType]) -> Vec<super::OpenFieldInfo> {
        fields.iter().map(|f| {
            let nested = match &f.ty {
                TypeExpr::OpenRecord { fields: inner } => {
                    let inner_infos = self.build_open_field_infos(inner);
                    let inner_names: Vec<String> = inner.iter().map(|f| f.name.clone()).collect();
                    let struct_name = self.fresh_anon_record_name(&inner_names);
                    Some((struct_name, inner_infos))
                }
                _ => None,
            };
            super::OpenFieldInfo { name: f.name.clone(), nested }
        }).collect()
    }

    /// Convert an internal Ty to a TypeExpr (for open record fields detected via IR).
    fn ty_to_type_expr(&self, ty: &crate::types::Ty) -> crate::ast::TypeExpr {
        use crate::types::Ty;
        use crate::ast::TypeExpr;
        match ty {
            Ty::Int => TypeExpr::Simple { name: "Int".to_string() },
            Ty::Float => TypeExpr::Simple { name: "Float".to_string() },
            Ty::String => TypeExpr::Simple { name: "String".to_string() },
            Ty::Bool => TypeExpr::Simple { name: "Bool".to_string() },
            Ty::Unit => TypeExpr::Simple { name: "Unit".to_string() },
            Ty::List(inner) => TypeExpr::Generic { name: "List".to_string(), args: vec![self.ty_to_type_expr(inner)] },
            Ty::Option(inner) => TypeExpr::Generic { name: "Option".to_string(), args: vec![self.ty_to_type_expr(inner)] },
            Ty::Result(ok, err) => TypeExpr::Generic { name: "Result".to_string(), args: vec![self.ty_to_type_expr(ok), self.ty_to_type_expr(err)] },
            Ty::Map(k, v) => TypeExpr::Generic { name: "Map".to_string(), args: vec![self.ty_to_type_expr(k), self.ty_to_type_expr(v)] },
            Ty::Named(n, _) => TypeExpr::Simple { name: n.clone() },
            _ => TypeExpr::Simple { name: "Unknown".to_string() },
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

    /// Check if a Ty contains Ty::Unknown anywhere in its structure.
    fn ty_contains_unknown(ty: &almide::types::Ty) -> bool {
        use almide::types::Ty;
        match ty {
            Ty::Unknown => true,
            Ty::List(inner) | Ty::Option(inner) => Self::ty_contains_unknown(inner),
            Ty::Result(a, b) | Ty::Map(a, b) => Self::ty_contains_unknown(a) || Self::ty_contains_unknown(b),
            Ty::Tuple(elems) => elems.iter().any(|e| Self::ty_contains_unknown(e)),
            Ty::Named(_, args) => args.iter().any(|a| Self::ty_contains_unknown(a)),
            Ty::Fn { params, ret } => params.iter().any(|p| Self::ty_contains_unknown(p)) || Self::ty_contains_unknown(ret),
            Ty::Record { fields } => fields.iter().any(|(_, fty)| Self::ty_contains_unknown(fty)),
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
            IrTypeDeclKind::Variant { cases, is_generic, boxed_args, boxed_record_fields: _ } => {
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
                        let field_infos = self.build_open_field_infos(&fields);
                        let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                        let struct_name = self.fresh_anon_record_name(&field_names);
                        let type_args: Vec<String> = fields.iter().map(|f| self.gen_type(&f.ty)).collect();
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

        let generic_str = Self::ir_generic_str(ir_fn.generics.as_ref());
        let async_prefix = if ir_fn.is_async { "async " } else { "" };
        self.emitln(&format!("{}fn {}{}({}) -> {} {{", async_prefix, fn_name, generic_str, params_str.join(", "), actual_ret));
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
            self.emit_ir_fn_body(&ir_fn.body, ir_fn.is_effect, &ret_str, is_unit_ret);
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
            // Temporarily swap ir_program to module's IR (for VarTable access)
            let saved_ir = self.ir_program.take();
            let module_ir_prog = saved_ir.as_ref()
                .and_then(|ir| ir.modules.iter().find(|m| m.name == module_name))
                .map(|m| almide::ir::IrProgram {
                    functions: m.functions.clone(),
                    top_lets: m.top_lets.clone(),
                    type_decls: m.type_decls.clone(),
                    var_table: m.var_table.clone(),
                    modules: Vec::new(),
                })
                .or_else(|| self.module_irs.get(module_name).cloned());
            self.ir_program = module_ir_prog;
            self.analyze_ir_single_use(&ir_fn.body, &ir_fn.params);
            let prev_effect = self.in_effect;
            self.in_effect = ir_fn.is_effect || ret_str.starts_with("Result<");
            self.emit_ir_fn_body(&ir_fn.body, ir_fn.is_effect, &ret_str, is_unit_ret);
            self.in_effect = prev_effect;
            self.ir_program = saved_ir;
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
