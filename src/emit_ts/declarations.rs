use crate::ast::*;
use crate::emit_ts_runtime;
use super::TsEmitter;

impl TsEmitter {
    pub(crate) fn emit_program(&mut self, prog: &Program, modules: &[(String, Program)]) {
        self.out.push_str(&emit_ts_runtime::full_runtime(self.js_mode));
        self.out.push('\n');

        // Register and emit imported modules as namespace objects
        for (mod_name, _) in modules {
            self.user_modules.push(mod_name.clone());
        }
        // Pre-declare namespace objects for parent modules that don't have their own module
        let module_names: std::collections::HashSet<String> = modules.iter().map(|(n, _)| n.clone()).collect();
        let mut emitted_ns: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (mod_name, _) in modules {
            if mod_name.contains('.') {
                // Walk up the parent chain (e.g. a.b.c -> check a, a.b)
                let parts: Vec<&str> = mod_name.split('.').collect();
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
        for (mod_name, mod_prog) in modules {
            self.emit_user_module(mod_name, mod_prog);
        }

        // Emit import aliases and direct sub-module imports
        for imp in &prog.imports {
            if let Decl::Import { path, alias, .. } = imp {
                let full_path = path.join(".");
                if let Some(alias_name) = alias {
                    let kw = if self.js_mode { "var" } else { "const" };
                    self.out.push_str(&format!("{} {} = {};\n", kw, alias_name, full_path));
                } else if path.len() > 1 {
                    // Direct sub-module import: `import mylib.parser` makes `parser` available
                    let short_name = path.last().unwrap();
                    let kw = if self.js_mode { "var" } else { "const" };
                    self.out.push_str(&format!("{} {} = {};\n", kw, short_name, full_path));
                }
            }
        }
        self.out.push('\n');

        let mut has_main = false;
        for decl in &prog.decls {
            if let Decl::Fn { name, .. } = decl {
                if name == "main" {
                    has_main = true;
                }
            }
            self.out.push_str(&self.gen_decl(decl));
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

    fn emit_user_module(&mut self, name: &str, prog: &Program) {
        self.out.push_str(&format!("// module: {}\n", name));
        if name.contains('.') {
            // Sub-module: use property assignment instead of const declaration
            self.out.push_str(&format!("{} = (() => {{\n", name));
        } else {
            self.out.push_str(&format!("const {} = (() => {{\n", name));
        }

        for decl in &prog.decls {
            match decl {
                Decl::Fn { .. } => {
                    self.out.push_str(&self.gen_decl(decl));
                    self.out.push('\n');
                }
                Decl::Type { .. } => {
                    self.out.push_str(&self.gen_decl(decl));
                    self.out.push('\n');
                }
                _ => {}
            }
        }

        // Export non-local functions
        let fn_names: Vec<String> = prog.decls.iter().filter_map(|d| {
            if let Decl::Fn { name, visibility, .. } = d {
                if *visibility != Visibility::Local { Some(Self::sanitize(name)) } else { None }
            } else { None }
        }).collect();
        self.out.push_str(&format!("  return {{ {} }};\n", fn_names.join(", ")));
        self.out.push_str("})();\n\n");
    }

    pub(crate) fn gen_decl(&self, decl: &Decl) -> String {
        match decl {
            Decl::Module { path, .. } => format!("// module: {}", path.join(".")),
            Decl::Import { path, .. } => format!("// import: {}", path.join(".")),
            Decl::Type { name, ty, generics, .. } => {
                if self.js_mode {
                    // In JS mode, skip pure type decls but still generate variant constructors
                    if matches!(ty, TypeExpr::Variant { .. }) {
                        self.gen_type_decl(name, ty, generics.as_ref())
                    } else {
                        format!("// type: {}", name)
                    }
                } else {
                    self.gen_type_decl(name, ty, generics.as_ref())
                }
            }
            Decl::Fn { name, params, return_type, body, r#async, extern_attrs, generics, .. } => {
                self.gen_fn_decl(name, params, return_type, body.as_ref(), r#async.unwrap_or(false), extern_attrs, generics.as_ref())
            }
            Decl::Trait { name, .. } => format!("// trait {}", name),
            Decl::Impl { trait_, for_, methods, .. } => {
                let mut lines = vec![format!("// impl {} for {}", trait_, for_)];
                for m in methods {
                    lines.push(self.gen_decl(m));
                }
                lines.join("\n")
            }
            Decl::Test { name, body, .. } => {
                let body_str = self.gen_expr(body);
                if self.js_mode {
                    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
                    format!(
                        "try {{ (() => {})(); console.log(\"  test {} ... ok\"); }} catch(__e) {{ console.log(\"  test {} ... FAILED\"); console.log(\"    \" + __e.message.split(\"\\n\").join(\"\\n    \")); __node_process.exitCode = 1; }}",
                        body_str,
                        escaped,
                        escaped,
                    )
                } else {
                    format!("Deno.test({}, () => {});", Self::json_string(name), body_str)
                }
            }
            Decl::Strict { mode, .. } => format!("// strict {}", mode),
        }
    }

    pub(crate) fn gen_type_decl(&self, name: &str, ty: &TypeExpr, generics: Option<&Vec<crate::ast::GenericParam>>) -> String {
        let generic_str = if self.js_mode {
            String::new()
        } else {
            match generics {
                Some(gs) if !gs.is_empty() => {
                    let gparams: Vec<String> = gs.iter().map(|g| g.name.clone()).collect();
                    format!("<{}>", gparams.join(", "))
                }
                _ => String::new(),
            }
        };
        match ty {
            TypeExpr::Record { fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("  {}: {};", f.name, self.gen_type_expr(&f.ty)))
                    .collect();
                format!("interface {}{} {{\n{}\n}}", name, generic_str, fs.join("\n"))
            }
            TypeExpr::Variant { cases } => {
                let mut lines = vec![format!("// variant type {}", name)];
                for case in cases {
                    match case {
                        VariantCase::Unit { name: cname } => {
                            lines.push(format!("const {} = {{ tag: {} }};", cname, Self::json_string(cname)));
                        }
                        VariantCase::Tuple { name: cname, fields } => {
                            let params: Vec<String> = fields.iter().enumerate()
                                .map(|(i, _)| format!("_{}", i))
                                .collect();
                            let obj_fields: Vec<String> = fields.iter().enumerate()
                                .map(|(i, _)| format!("_{}: _{}", i, i))
                                .collect();
                            lines.push(format!("function {}({}) {{ return {{ tag: {}, {} }}; }}",
                                cname, params.join(", "), Self::json_string(cname), obj_fields.join(", ")));
                        }
                        VariantCase::Record { name: cname, fields } => {
                            let params: Vec<String> = fields.iter()
                                .map(|f| f.name.clone())
                                .collect();
                            let obj_fields: Vec<String> = fields.iter()
                                .map(|f| format!("{}: {}", f.name, f.name))
                                .collect();
                            lines.push(format!("function {}({}) {{ return {{ tag: {}, {} }}; }}",
                                cname, params.join(", "), Self::json_string(cname), obj_fields.join(", ")));
                        }
                    }
                }
                lines.join("\n")
            }
            TypeExpr::Newtype { inner } => {
                format!("type {}{} = {} & {{ readonly __brand: \"{}\" }};", name, generic_str, self.gen_type_expr(inner), name)
            }
            _ => format!("type {}{} = {};", name, generic_str, self.gen_type_expr(ty)),
        }
    }

    pub(crate) fn gen_type_expr(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Simple { name } => Self::map_type_name(name).to_string(),
            TypeExpr::Generic { name, args } => {
                match name.as_str() {
                    "List" => format!("{}[]", self.gen_type_expr(&args[0])),
                    "Map" => format!("Map<{}>", args.iter().map(|a| self.gen_type_expr(a)).collect::<Vec<_>>().join(", ")),
                    "Set" => format!("Set<{}>", self.gen_type_expr(&args[0])),
                    "Result" => self.gen_type_expr(&args[0]),
                    "Option" => format!("{} | null", self.gen_type_expr(&args[0])),
                    _ => format!("{}<{}>", name, args.iter().map(|a| self.gen_type_expr(a)).collect::<Vec<_>>().join(", ")),
                }
            }
            TypeExpr::Record { fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("{}: {}", f.name, self.gen_type_expr(&f.ty)))
                    .collect();
                format!("{{ {} }}", fs.join(", "))
            }
            TypeExpr::Fn { params, ret } => {
                let ps: Vec<String> = params.iter().enumerate()
                    .map(|(i, p)| format!("_{}: {}", i, self.gen_type_expr(p)))
                    .collect();
                format!("({}) => {}", ps.join(", "), self.gen_type_expr(ret))
            }
            TypeExpr::Tuple { elements } => {
                let ts: Vec<String> = elements.iter().map(|e| self.gen_type_expr(e)).collect();
                format!("[{}]", ts.join(", "))
            }
            TypeExpr::Newtype { inner } => self.gen_type_expr(inner),
            TypeExpr::Variant { .. } => "any".to_string(),
        }
    }

    fn map_type_name(name: &str) -> &str {
        match name {
            "Int" => "number",
            "Float" => "number",
            "String" => "string",
            "Bool" => "boolean",
            "Unit" => "void",
            "Path" => "string",
            other => other,
        }
    }

    fn gen_fn_decl(&self, name: &str, params: &[Param], ret_type: &TypeExpr, body: Option<&Expr>, is_async: bool, extern_attrs: &[ExternAttr], generics: Option<&Vec<crate::ast::GenericParam>>) -> String {
        let async_ = if is_async { "async " } else { "" };
        let sname = Self::sanitize(name);
        let generic_str = if self.js_mode {
            String::new()
        } else {
            match generics {
                Some(gs) if !gs.is_empty() => {
                    let gparams: Vec<String> = gs.iter().map(|g| g.name.clone()).collect();
                    format!("<{}>", gparams.join(", "))
                }
                _ => String::new(),
            }
        };
        let params_str: Vec<String> = params.iter()
            .filter(|p| p.name != "self")
            .map(|p| {
                if self.js_mode {
                    Self::sanitize(&p.name)
                } else {
                    format!("{}: {}", Self::sanitize(&p.name), self.gen_type_expr(&p.ty))
                }
            })
            .collect();
        let ret_str = if self.js_mode { String::new() } else { format!(": {}", self.gen_type_expr(ret_type)) };

        // Check for @extern(ts, ...)
        if let Some(ext) = extern_attrs.iter().find(|a| a.target == "ts") {
            let args: Vec<String> = params.iter()
                .filter(|p| p.name != "self")
                .map(|p| Self::sanitize(&p.name))
                .collect();
            let call = format!("{}.{}({})", ext.module, ext.function, args.join(", "));
            return format!("{}function {}{}({}){} {{\n  return {};\n}}", async_, sname, generic_str, params_str.join(", "), ret_str, call);
        }

        if let Some(body) = body {
            let body_str = self.gen_expr(body);
            match body {
                Expr::Block { .. } => {
                    format!("{}function {}{}({}){} {}", async_, sname, generic_str, params_str.join(", "), ret_str, body_str)
                }
                Expr::DoBlock { .. } => {
                    format!("{}function {}{}({}){} {{\n{}\n}}", async_, sname, generic_str, params_str.join(", "), ret_str, body_str)
                }
                _ => {
                    format!("{}function {}{}({}){} {{\n  return {};\n}}", async_, sname, generic_str, params_str.join(", "), ret_str, body_str)
                }
            }
        } else {
            format!("{}function {}{}({}){} {{\n  throw new Error(\"no body and no @extern for ts target\");\n}}", async_, sname, generic_str, params_str.join(", "), ret_str)
        }
    }

    // ── npm package emission ──

    /// Emit user code for npm target: no inlined runtime, `export` for public functions,
    /// no entry point, no test blocks.
    pub(crate) fn emit_npm_program(&mut self, prog: &Program, modules: &[(String, Program)]) {
        // Register user modules
        for (mod_name, _) in modules {
            self.user_modules.push(mod_name.clone());
        }

        // Pre-declare namespace objects for parent modules
        let module_names: std::collections::HashSet<String> = modules.iter().map(|(n, _)| n.clone()).collect();
        let mut emitted_ns: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (mod_name, _) in modules {
            if mod_name.contains('.') {
                let parts: Vec<&str> = mod_name.split('.').collect();
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
        for (mod_name, mod_prog) in modules {
            self.emit_user_module(mod_name, mod_prog);
        }

        // Emit import aliases
        for imp in &prog.imports {
            if let Decl::Import { path, alias, .. } = imp {
                let full_path = path.join(".");
                if let Some(alias_name) = alias {
                    self.out.push_str(&format!("var {} = {};\n", alias_name, full_path));
                } else if path.len() > 1 {
                    let short_name = path.last().unwrap();
                    self.out.push_str(&format!("var {} = {};\n", short_name, full_path));
                }
            }
        }
        self.out.push('\n');

        // Emit declarations: all functions without export keyword, skip tests
        // Collect public names for clean export at the end
        let mut public_fns: Vec<(String, String)> = Vec::new(); // (sanitized, original)
        let mut public_variants: Vec<String> = Vec::new(); // variant constructor names

        for decl in &prog.decls {
            match decl {
                Decl::Test { .. } => continue,
                Decl::Fn { name, visibility, .. } => {
                    self.out.push_str(&self.gen_decl(decl));
                    self.out.push_str("\n\n");
                    if *visibility == Visibility::Public {
                        public_fns.push((Self::sanitize(name), name.clone()));
                    }
                }
                Decl::Type { name, ty, visibility, .. } => {
                    if *visibility == Visibility::Public {
                        if let TypeExpr::Variant { cases } = ty {
                            self.out.push_str(&format!("// variant type {}\n", name));
                            for case in cases {
                                match case {
                                    VariantCase::Unit { name: cname } => {
                                        self.out.push_str(&format!("const {} = {{ tag: {} }};\n", cname, Self::json_string(cname)));
                                        public_variants.push(cname.clone());
                                    }
                                    VariantCase::Tuple { name: cname, fields } => {
                                        let params: Vec<String> = fields.iter().enumerate()
                                            .map(|(i, _)| format!("_{}", i))
                                            .collect();
                                        let obj_fields: Vec<String> = fields.iter().enumerate()
                                            .map(|(i, _)| format!("_{}: _{}", i, i))
                                            .collect();
                                        self.out.push_str(&format!("function {}({}) {{ return {{ tag: {}, {} }}; }}\n",
                                            cname, params.join(", "), Self::json_string(cname), obj_fields.join(", ")));
                                        public_variants.push(cname.clone());
                                    }
                                    VariantCase::Record { name: cname, fields } => {
                                        let params: Vec<String> = fields.iter()
                                            .map(|f| f.name.clone())
                                            .collect();
                                        let obj_fields: Vec<String> = fields.iter()
                                            .map(|f| format!("{}: {}", f.name, f.name))
                                            .collect();
                                        self.out.push_str(&format!("function {}({}) {{ return {{ tag: {}, {} }}; }}\n",
                                            cname, params.join(", "), Self::json_string(cname), obj_fields.join(", ")));
                                        public_variants.push(cname.clone());
                                    }
                                }
                            }
                            self.out.push('\n');
                        } else {
                            self.out.push_str(&self.gen_decl(decl));
                            self.out.push_str("\n\n");
                        }
                    } else {
                        self.out.push_str(&self.gen_decl(decl));
                        self.out.push_str("\n\n");
                    }
                }
                _ => {
                    self.out.push_str(&self.gen_decl(decl));
                    self.out.push_str("\n\n");
                }
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
    pub(crate) fn generate_dts(&self, prog: &Program) -> String {
        let mut dts = String::new();
        for decl in &prog.decls {
            match decl {
                Decl::Fn { name, params, return_type, visibility, r#async, .. } => {
                    if *visibility != Visibility::Public {
                        continue;
                    }
                    let sname = Self::sanitize(name);
                    let clean = crate::emit_common::to_clean_export_name(&sname);
                    let ps: Vec<String> = params.iter()
                        .filter(|p| p.name != "self")
                        .map(|p| {
                            let pname = crate::emit_common::to_clean_export_name(&Self::sanitize(&p.name));
                            format!("{}: {}", pname, self.gen_type_expr(&p.ty))
                        })
                        .collect();
                    let ret = self.gen_type_expr(return_type);
                    let ret_str = if r#async.unwrap_or(false) {
                        format!("Promise<{}>", ret)
                    } else {
                        ret
                    };
                    dts.push_str(&format!("export declare function {}({}): {};\n", clean, ps.join(", "), ret_str));
                }
                Decl::Type { name, ty, visibility, .. } => {
                    if *visibility != Visibility::Public {
                        continue;
                    }
                    match ty {
                        TypeExpr::Record { fields } => {
                            let fs: Vec<String> = fields.iter()
                                .map(|f| format!("  {}: {};", f.name, self.gen_type_expr(&f.ty)))
                                .collect();
                            dts.push_str(&format!("export interface {} {{\n{}\n}}\n", name, fs.join("\n")));
                        }
                        TypeExpr::Variant { cases } => {
                            // Export variant constructor type declarations
                            for case in cases {
                                match case {
                                    VariantCase::Unit { name: cname } => {
                                        dts.push_str(&format!("export declare const {}: {{ tag: \"{}\" }};\n", cname, cname));
                                    }
                                    VariantCase::Tuple { name: cname, fields } => {
                                        let ps: Vec<String> = fields.iter().enumerate()
                                            .map(|(i, f)| format!("_{}: {}", i, self.gen_type_expr(f)))
                                            .collect();
                                        let ret_fields: Vec<String> = std::iter::once(format!("tag: \"{}\"", cname))
                                            .chain(fields.iter().enumerate().map(|(i, f)| format!("_{}: {}", i, self.gen_type_expr(f))))
                                            .collect();
                                        dts.push_str(&format!("export declare function {}({}): {{ {} }};\n", cname, ps.join(", "), ret_fields.join(", ")));
                                    }
                                    VariantCase::Record { name: cname, fields } => {
                                        let ps: Vec<String> = fields.iter()
                                            .map(|f| format!("{}: {}", f.name, self.gen_type_expr(&f.ty)))
                                            .collect();
                                        let ret_fields: Vec<String> = std::iter::once(format!("tag: \"{}\"", cname))
                                            .chain(fields.iter().map(|f| format!("{}: {}", f.name, self.gen_type_expr(&f.ty))))
                                            .collect();
                                        dts.push_str(&format!("export declare function {}({}): {{ {} }};\n", cname, ps.join(", "), ret_fields.join(", ")));
                                    }
                                }
                            }
                        }
                        _ => {
                            dts.push_str(&format!("export type {} = {};\n", name, self.gen_type_expr(ty)));
                        }
                    }
                }
                _ => {}
            }
        }
        dts
    }
}
