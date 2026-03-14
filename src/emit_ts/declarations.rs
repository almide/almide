use crate::emit_ts_runtime;
use crate::ir::{IrFunction, IrTypeDecl, IrTypeDeclKind, IrVariantKind, IrModule, IrVisibility};
use crate::types::Ty;
use super::TsEmitter;

impl TsEmitter {
    /// Pre-collect variant info from IR type declarations.
    fn collect_generic_variant_info_from_ir(&mut self, type_decls: &[IrTypeDecl]) {
        for td in type_decls {
            if let IrTypeDeclKind::Variant { cases, is_generic, .. } = &td.kind {
                for case in cases {
                    match &case.kind {
                        IrVariantKind::Unit => {
                            self.unit_variant_names.insert(case.name.clone());
                            if *is_generic {
                                self.generic_variant_unit_ctors.insert(case.name.clone());
                            }
                        }
                        IrVariantKind::Record { .. } => {
                            self.variant_constructors.insert(case.name.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    pub(crate) fn emit_program(&mut self) {
        let ir = self.ir_program.as_ref().expect("IR required for codegen").clone();

        self.collect_generic_variant_info_from_ir(&ir.type_decls);
        for ir_mod in &ir.modules {
            self.collect_generic_variant_info_from_ir(&ir_mod.type_decls);
        }
        self.out.push_str(&emit_ts_runtime::full_runtime(self.js_mode));
        self.out.push('\n');

        // Register and emit imported modules as namespace objects
        for ir_mod in &ir.modules {
            self.user_modules.push(ir_mod.name.clone());
        }
        // Pre-declare namespace objects for parent modules that don't have their own module
        let module_names: std::collections::HashSet<String> = ir.modules.iter().map(|m| m.name.clone()).collect();
        let mut emitted_ns: std::collections::HashSet<String> = std::collections::HashSet::new();
        for ir_mod in &ir.modules {
            if ir_mod.name.contains('.') {
                let parts: Vec<&str> = ir_mod.name.split('.').collect();
                for i in 1..parts.len() {
                    let ancestor = parts[..i].join(".");
                    if !module_names.contains(&ancestor) && !emitted_ns.contains(&ancestor) {
                        emitted_ns.insert(ancestor.clone());
                        if ancestor.contains('.') {
                            self.out.push_str(&format!("{} = {{}};\n", ancestor));
                        } else {
                            self.out.push_str(&format!("const {} = {{}};\n", ancestor));
                        }
                    }
                }
            }
        }
        for ir_mod in &ir.modules {
            self.emit_ir_user_module(&ir_mod);
        }

        // Emit import aliases from IR imports (stored as the first module entries or from program metadata)
        // Import aliases are handled by the caller before emit_program
        self.out.push('\n');

        // Emit declarations from IR
        let mut has_main = false;

        // Type declarations
        for td in &ir.type_decls {
            self.out.push_str(&self.gen_ir_type_decl(td));
            self.out.push_str("\n\n");
        }

        // Top-level lets
        for tl in &ir.top_lets {
            let name = ir.var_table.get(tl.var).name.clone();
            let val_str = self.gen_ir_expr(&tl.value);
            if self.js_mode {
                self.out.push_str(&format!("var {} = {};", name, val_str));
            } else {
                self.out.push_str(&format!("const {} = {};", name, val_str));
            }
            self.out.push_str("\n\n");
        }

        // Functions
        for func in &ir.functions {
            if func.name == "main" {
                has_main = true;
            }
            if func.is_test {
                self.out.push_str(&self.gen_ir_test(func));
            } else {
                self.out.push_str(&self.gen_ir_fn_decl(func));
            }
            self.out.push_str("\n\n");
        }

        if has_main {
            self.out.push_str("// ---- Entry Point ----\n");
            if self.js_mode {
                self.out.push_str("try { main([\"app\", ...__node_process.argv.slice(2)]); } catch (e) { if (e instanceof Error) { console.error(e.message); __node_process.exit(1); } throw e; }\n");
            } else {
                self.out.push_str("try { main([\"minigit\", ...Deno.args]); } catch (e) { if (e instanceof Error) { eprintln(e.message); Deno.exit(1); } throw e; }\n");
            }
        }
    }

    fn emit_ir_user_module(&mut self, ir_mod: &IrModule) {
        self.out.push_str(&format!("// module: {}\n", ir_mod.name));
        if ir_mod.name.contains('.') {
            self.out.push_str(&format!("{} = (() => {{\n", ir_mod.name));
        } else {
            self.out.push_str(&format!("const {} = (() => {{\n", ir_mod.name));
        }

        for td in &ir_mod.type_decls {
            self.out.push_str(&self.gen_ir_type_decl(td));
            self.out.push('\n');
        }

        for func in &ir_mod.functions {
            self.out.push_str(&self.gen_ir_fn_decl(func));
            self.out.push('\n');
        }

        // Export non-local functions
        let fn_names: Vec<String> = ir_mod.functions.iter()
            .filter(|f| f.visibility != IrVisibility::Private)
            .map(|f| Self::sanitize(&f.name))
            .collect();
        self.out.push_str(&format!("  return {{ {} }};\n", fn_names.join(", ")));
        self.out.push_str("})();\n\n");
    }

    /// Convert an IR Ty to a TypeScript type string.
    pub(crate) fn ir_ty_to_ts(&self, ty: &Ty) -> String {
        match ty {
            Ty::Int | Ty::Float => "number".to_string(),
            Ty::String => "string".to_string(),
            Ty::Bool => "boolean".to_string(),
            Ty::Unit => "void".to_string(),
            Ty::List(inner) => format!("{}[]", self.ir_ty_to_ts(inner)),
            Ty::Map(k, v) => format!("Map<{}, {}>", self.ir_ty_to_ts(k), self.ir_ty_to_ts(v)),
            Ty::Option(inner) => format!("{} | null", self.ir_ty_to_ts(inner)),
            Ty::Result(ok, _) => self.ir_ty_to_ts(ok),
            Ty::Tuple(elems) => {
                let ts: Vec<String> = elems.iter().map(|e| self.ir_ty_to_ts(e)).collect();
                format!("[{}]", ts.join(", "))
            }
            Ty::Fn { params, ret } => {
                let ps: Vec<String> = params.iter().enumerate()
                    .map(|(i, p)| format!("_{}: {}", i, self.ir_ty_to_ts(p)))
                    .collect();
                format!("({}) => {}", ps.join(", "), self.ir_ty_to_ts(ret))
            }
            Ty::Record { fields } | Ty::OpenRecord { fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|(name, ty)| format!("{}: {}", name, self.ir_ty_to_ts(ty)))
                    .collect();
                format!("{{ {} }}", fs.join(", "))
            }
            Ty::Named(name, _) => {
                match name.as_str() {
                    "Path" => "string".to_string(),
                    other => other.to_string(),
                }
            }
            Ty::TypeVar(name) => name.clone(),
            Ty::Union(members) => {
                let ms: Vec<String> = members.iter().map(|m| self.ir_ty_to_ts(m)).collect();
                ms.join(" | ")
            }
            Ty::Variant { name, .. } => name.clone(),
            Ty::Unknown => "any".to_string(),
        }
    }

    /// Generate a type declaration from IR.
    fn gen_ir_type_decl(&self, td: &IrTypeDecl) -> String {
        let generic_str = if self.js_mode {
            String::new()
        } else {
            match &td.generics {
                Some(gs) if !gs.is_empty() => {
                    let gparams: Vec<String> = gs.iter().map(|g| g.name.clone()).collect();
                    format!("<{}>", gparams.join(", "))
                }
                _ => String::new(),
            }
        };

        match &td.kind {
            IrTypeDeclKind::Record { fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("  {}: {};", f.name, self.ir_ty_to_ts(&f.ty)))
                    .collect();
                format!("interface {}{} {{\n{}\n}}", td.name, generic_str, fs.join("\n"))
            }
            IrTypeDeclKind::Alias { target } => {
                if let Ty::OpenRecord { fields } = target {
                    let fs: Vec<String> = fields.iter()
                        .map(|(name, ty)| format!("  {}: {};", name, self.ir_ty_to_ts(ty)))
                        .collect();
                    format!("interface {}{} {{\n{}\n}}", td.name, generic_str, fs.join("\n"))
                } else {
                    format!("type {}{} = {};", td.name, generic_str, self.ir_ty_to_ts(target))
                }
            }
            IrTypeDeclKind::Variant { cases, is_generic, .. } => {
                if self.js_mode {
                    // In JS mode, only generate variant constructors
                    self.gen_ir_variant_constructors(&td.name, cases, *is_generic)
                } else {
                    self.gen_ir_variant_constructors(&td.name, cases, *is_generic)
                }
            }
        }
    }

    fn gen_ir_variant_constructors(&self, _name: &str, cases: &[crate::ir::IrVariantDecl], is_generic: bool) -> String {
        let mut lines = vec![format!("// variant type {}", _name)];
        for case in cases {
            match &case.kind {
                IrVariantKind::Unit => {
                    if is_generic {
                        lines.push(format!("function {}() {{ return {{ tag: {} }}; }}", case.name, Self::json_string(&case.name)));
                    } else {
                        lines.push(format!("const {} = {{ tag: {} }};", case.name, Self::json_string(&case.name)));
                    }
                }
                IrVariantKind::Tuple { fields } => {
                    let params: Vec<String> = fields.iter().enumerate()
                        .map(|(i, _)| format!("_{}", i))
                        .collect();
                    let obj_fields: Vec<String> = fields.iter().enumerate()
                        .map(|(i, _)| format!("_{}: _{}", i, i))
                        .collect();
                    lines.push(format!("function {}({}) {{ return {{ tag: {}, {} }}; }}",
                        case.name, params.join(", "), Self::json_string(&case.name), obj_fields.join(", ")));
                }
                IrVariantKind::Record { fields } => {
                    let params: Vec<String> = fields.iter()
                        .map(|f| f.name.clone())
                        .collect();
                    let obj_fields: Vec<String> = fields.iter()
                        .map(|f| format!("{}: {}", f.name, f.name))
                        .collect();
                    lines.push(format!("function {}({}) {{ return {{ tag: {}, {} }}; }}",
                        case.name, params.join(", "), Self::json_string(&case.name), obj_fields.join(", ")));
                }
            }
        }
        lines.join("\n")
    }

    /// Generate a function declaration from IR.
    fn gen_ir_fn_decl(&self, ir_fn: &IrFunction) -> String {
        let async_ = if ir_fn.is_async { "async " } else { "" };
        let sname = Self::sanitize(&ir_fn.name);
        let generic_str = if self.js_mode {
            String::new()
        } else {
            match &ir_fn.generics {
                Some(gs) if !gs.is_empty() => {
                    let gparams: Vec<String> = gs.iter().map(|g| g.name.clone()).collect();
                    format!("<{}>", gparams.join(", "))
                }
                _ => String::new(),
            }
        };
        let params_str: Vec<String> = ir_fn.params.iter()
            .filter(|p| p.name != "self")
            .map(|p| {
                if self.js_mode {
                    Self::sanitize(&p.name)
                } else {
                    format!("{}: {}", Self::sanitize(&p.name), self.ir_ty_to_ts(&p.ty))
                }
            })
            .collect();
        let ret_str = if self.js_mode { String::new() } else { format!(": {}", self.ir_ty_to_ts(&ir_fn.ret_ty)) };

        // Check for @extern(ts, ...)
        if let Some(ext) = ir_fn.extern_attrs.iter().find(|a| a.target == "ts") {
            let args: Vec<String> = ir_fn.params.iter()
                .filter(|p| p.name != "self")
                .map(|p| Self::sanitize(&p.name))
                .collect();
            let call = format!("{}.{}({})", ext.module, ext.function, args.join(", "));
            return format!("{}function {}{}({}){} {{\n  return {};\n}}", async_, sname, generic_str, params_str.join(", "), ret_str, call);
        }

        let prev = self.in_effect.get();
        if ir_fn.is_effect { self.in_effect.set(true); }
        let body_str = self.gen_ir_expr(&ir_fn.body);
        self.in_effect.set(prev);
        match &ir_fn.body.kind {
            crate::ir::IrExprKind::Block { .. } => {
                format!("{}function {}{}({}){} {}", async_, sname, generic_str, params_str.join(", "), ret_str, body_str)
            }
            crate::ir::IrExprKind::DoBlock { .. } => {
                format!("{}function {}{}({}){} {{\n{}\n}}", async_, sname, generic_str, params_str.join(", "), ret_str, body_str)
            }
            _ => {
                format!("{}function {}{}({}){} {{\n  return {};\n}}", async_, sname, generic_str, params_str.join(", "), ret_str, body_str)
            }
        }
    }

    /// Generate a test from IR.
    fn gen_ir_test(&self, ir_fn: &IrFunction) -> String {
        let prev_test = self.in_test.get();
        self.in_test.set(true);
        let body_str = self.gen_ir_expr(&ir_fn.body);
        self.in_test.set(prev_test);
        if self.js_mode {
            let escaped = ir_fn.name.replace('\\', "\\\\").replace('"', "\\\"");
            format!(
                "try {{ (() => {})(); console.log(\"  test {} ... ok\"); }} catch(__e) {{ console.log(\"  test {} ... FAILED\"); console.log(\"    \" + __e.message.split(\"\\n\").join(\"\\n    \")); __node_process.exitCode = 1; }}",
                body_str,
                escaped,
                escaped,
            )
        } else {
            format!("Deno.test({}, () => {});", Self::json_string(&ir_fn.name), body_str)
        }
    }

    // ── npm package emission ──

    /// Emit user code for npm target: no inlined runtime, `export` for public functions,
    /// no entry point, no test blocks.
    pub(crate) fn emit_npm_program(&mut self) {
        let ir = self.ir_program.as_ref().expect("IR required for codegen").clone();

        self.collect_generic_variant_info_from_ir(&ir.type_decls);
        for ir_mod in &ir.modules {
            self.collect_generic_variant_info_from_ir(&ir_mod.type_decls);
        }
        // Register user modules
        for ir_mod in &ir.modules {
            self.user_modules.push(ir_mod.name.clone());
        }

        // Pre-declare namespace objects for parent modules
        let module_names: std::collections::HashSet<String> = ir.modules.iter().map(|m| m.name.clone()).collect();
        let mut emitted_ns: std::collections::HashSet<String> = std::collections::HashSet::new();
        for ir_mod in &ir.modules {
            if ir_mod.name.contains('.') {
                let parts: Vec<&str> = ir_mod.name.split('.').collect();
                for i in 1..parts.len() {
                    let ancestor = parts[..i].join(".");
                    if !module_names.contains(&ancestor) && !emitted_ns.contains(&ancestor) {
                        emitted_ns.insert(ancestor.clone());
                        if ancestor.contains('.') {
                            self.out.push_str(&format!("{} = {{}};\n", ancestor));
                        } else {
                            self.out.push_str(&format!("const {} = {{}};\n", ancestor));
                        }
                    }
                }
            }
        }

        // Emit user modules
        for ir_mod in &ir.modules {
            self.emit_ir_user_module(&ir_mod);
        }

        self.out.push('\n');

        // Emit declarations: all functions, skip tests
        // Collect public names for clean export at the end
        let mut public_fns: Vec<(String, String)> = Vec::new(); // (sanitized, original)
        let mut public_variants: Vec<String> = Vec::new(); // variant constructor names

        // Type declarations (variant constructors for public types)
        for td in &ir.type_decls {
            if td.visibility == IrVisibility::Public {
                if let IrTypeDeclKind::Variant { cases, .. } = &td.kind {
                    self.out.push_str(&format!("// variant type {}\n", td.name));
                    for case in cases {
                        match &case.kind {
                            IrVariantKind::Unit => {
                                self.out.push_str(&format!("const {} = {{ tag: {} }};\n", case.name, Self::json_string(&case.name)));
                                public_variants.push(case.name.clone());
                            }
                            IrVariantKind::Tuple { fields } => {
                                let params: Vec<String> = fields.iter().enumerate()
                                    .map(|(i, _)| format!("_{}", i))
                                    .collect();
                                let obj_fields: Vec<String> = fields.iter().enumerate()
                                    .map(|(i, _)| format!("_{}: _{}", i, i))
                                    .collect();
                                self.out.push_str(&format!("function {}({}) {{ return {{ tag: {}, {} }}; }}\n",
                                    case.name, params.join(", "), Self::json_string(&case.name), obj_fields.join(", ")));
                                public_variants.push(case.name.clone());
                            }
                            IrVariantKind::Record { fields } => {
                                let params: Vec<String> = fields.iter()
                                    .map(|f| f.name.clone())
                                    .collect();
                                let obj_fields: Vec<String> = fields.iter()
                                    .map(|f| format!("{}: {}", f.name, f.name))
                                    .collect();
                                self.out.push_str(&format!("function {}({}) {{ return {{ tag: {}, {} }}; }}\n",
                                    case.name, params.join(", "), Self::json_string(&case.name), obj_fields.join(", ")));
                                public_variants.push(case.name.clone());
                            }
                        }
                    }
                    self.out.push('\n');
                } else {
                    self.out.push_str(&self.gen_ir_type_decl(td));
                    self.out.push_str("\n\n");
                }
            } else {
                self.out.push_str(&self.gen_ir_type_decl(td));
                self.out.push_str("\n\n");
            }
        }

        // Top-level lets
        for tl in &ir.top_lets {
            let name = ir.var_table.get(tl.var).name.clone();
            let val_str = self.gen_ir_expr(&tl.value);
            self.out.push_str(&format!("var {} = {};", name, val_str));
            self.out.push_str("\n\n");
        }

        // Functions (skip tests)
        for func in &ir.functions {
            if func.is_test { continue; }
            self.out.push_str(&self.gen_ir_fn_decl(func));
            self.out.push_str("\n\n");
            if func.visibility == IrVisibility::Public {
                public_fns.push((Self::sanitize(&func.name), func.name.clone()));
            }
        }

        // Emit clean camelCase exports
        if !public_fns.is_empty() || !public_variants.is_empty() {
            self.out.push_str("// ---- Exports ----\n");
            let mut exports = Vec::new();
            for (sanitized, _original) in &public_fns {
                let clean = crate::emit_common::to_clean_export_name(sanitized);
                if clean == *sanitized {
                    exports.push(sanitized.clone());
                } else {
                    exports.push(format!("{} as {}", sanitized, clean));
                }
            }
            for vname in &public_variants {
                exports.push(vname.clone());
            }
            self.out.push_str(&format!("export {{ {} }};\n", exports.join(", ")));
        }
    }

    /// Generate TypeScript declaration file (index.d.ts) for public functions and types.
    /// Uses clean camelCase names matching the export aliases in index.js.
    pub(crate) fn generate_dts(&self) -> String {
        let ir = self.ir_program.as_ref().expect("IR required for codegen");
        let mut dts = String::new();

        for func in &ir.functions {
            if func.visibility != IrVisibility::Public { continue; }
            let sname = Self::sanitize(&func.name);
            let clean = crate::emit_common::to_clean_export_name(&sname);
            let ps: Vec<String> = func.params.iter()
                .filter(|p| p.name != "self")
                .map(|p| {
                    let pname = crate::emit_common::to_clean_export_name(&Self::sanitize(&p.name));
                    format!("{}: {}", pname, self.ir_ty_to_ts(&p.ty))
                })
                .collect();
            let ret = self.ir_ty_to_ts(&func.ret_ty);
            let ret_str = if func.is_async {
                format!("Promise<{}>", ret)
            } else {
                ret
            };
            dts.push_str(&format!("export declare function {}({}): {};\n", clean, ps.join(", "), ret_str));
        }

        for td in &ir.type_decls {
            if td.visibility != IrVisibility::Public { continue; }
            match &td.kind {
                IrTypeDeclKind::Record { fields } => {
                    let fs: Vec<String> = fields.iter()
                        .map(|f| format!("  {}: {};", f.name, self.ir_ty_to_ts(&f.ty)))
                        .collect();
                    dts.push_str(&format!("export interface {} {{\n{}\n}}\n", td.name, fs.join("\n")));
                }
                IrTypeDeclKind::Alias { target } => {
                    if let Ty::OpenRecord { fields } = target {
                        let fs: Vec<String> = fields.iter()
                            .map(|(name, ty)| format!("  {}: {};", name, self.ir_ty_to_ts(ty)))
                            .collect();
                        dts.push_str(&format!("export interface {} {{\n{}\n}}\n", td.name, fs.join("\n")));
                    } else {
                        dts.push_str(&format!("export type {} = {};\n", td.name, self.ir_ty_to_ts(target)));
                    }
                }
                IrTypeDeclKind::Variant { cases, .. } => {
                    for case in cases {
                        match &case.kind {
                            IrVariantKind::Unit => {
                                dts.push_str(&format!("export declare const {}: {{ tag: \"{}\" }};\n", case.name, case.name));
                            }
                            IrVariantKind::Tuple { fields } => {
                                let ps: Vec<String> = fields.iter().enumerate()
                                    .map(|(i, f)| format!("_{}: {}", i, self.ir_ty_to_ts(f)))
                                    .collect();
                                let ret_fields: Vec<String> = std::iter::once(format!("tag: \"{}\"", case.name))
                                    .chain(fields.iter().enumerate().map(|(i, f)| format!("_{}: {}", i, self.ir_ty_to_ts(f))))
                                    .collect();
                                dts.push_str(&format!("export declare function {}({}): {{ {} }};\n", case.name, ps.join(", "), ret_fields.join(", ")));
                            }
                            IrVariantKind::Record { fields } => {
                                let ps: Vec<String> = fields.iter()
                                    .map(|f| format!("{}: {}", f.name, self.ir_ty_to_ts(&f.ty)))
                                    .collect();
                                let ret_fields: Vec<String> = std::iter::once(format!("tag: \"{}\"", case.name))
                                    .chain(fields.iter().map(|f| format!("{}: {}", f.name, self.ir_ty_to_ts(&f.ty))))
                                    .collect();
                                dts.push_str(&format!("export declare function {}({}): {{ {} }};\n", case.name, ps.join(", "), ret_fields.join(", ")));
                            }
                        }
                    }
                }
            }
        }

        dts
    }
}
