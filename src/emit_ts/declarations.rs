use crate::ast::*;
use crate::emit_ts_runtime::{RUNTIME, RUNTIME_JS};
use super::TsEmitter;

impl TsEmitter {
    pub(crate) fn emit_program(&mut self, prog: &Program, modules: &[(String, Program)]) {
        if self.js_mode {
            self.out.push_str(RUNTIME_JS);
        } else {
            self.out.push_str(RUNTIME);
        }
        self.out.push('\n');

        // Register and emit imported modules as namespace objects
        for (mod_name, _) in modules {
            self.user_modules.push(mod_name.clone());
        }
        for (mod_name, mod_prog) in modules {
            self.emit_user_module(mod_name, mod_prog);
        }

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
                self.out.push_str("try { main([\"app\", ...process.argv.slice(2)]); } catch (e) { if (e instanceof Error) { console.error(e.message); process.exit(1); } throw e; }\n");
            } else {
                self.out.push_str("try { main([\"minigit\", ...Deno.args]); } catch (e) { if (e instanceof Error) { eprintln(e.message); Deno.exit(1); } throw e; }\n");
            }
        }
    }

    fn emit_user_module(&mut self, name: &str, prog: &Program) {
        self.out.push_str(&format!("// module: {}\n", name));
        self.out.push_str(&format!("const {} = (() => {{\n", name));

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
            Decl::Type { name, ty, .. } => {
                if self.js_mode {
                    // In JS mode, skip pure type decls but still generate variant constructors
                    if matches!(ty, TypeExpr::Variant { .. }) {
                        self.gen_type_decl(name, ty)
                    } else {
                        format!("// type: {}", name)
                    }
                } else {
                    self.gen_type_decl(name, ty)
                }
            }
            Decl::Fn { name, params, return_type, body, r#async, .. } => {
                self.gen_fn_decl(name, params, return_type, body, r#async.unwrap_or(false))
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
                        "try {{ (() => {})(); console.log(\"  test {} ... ok\"); }} catch(__e) {{ console.log(\"  test {} ... FAILED\"); console.log(\"    \" + __e.message.split(\"\\n\").join(\"\\n    \")); process.exitCode = 1; }}",
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

    pub(crate) fn gen_type_decl(&self, name: &str, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Record { fields } => {
                let fs: Vec<String> = fields.iter()
                    .map(|f| format!("  {}: {};", f.name, self.gen_type_expr(&f.ty)))
                    .collect();
                format!("interface {} {{\n{}\n}}", name, fs.join("\n"))
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
                format!("type {} = {} & {{ readonly __brand: \"{}\" }};", name, self.gen_type_expr(inner), name)
            }
            _ => format!("type {} = {};", name, self.gen_type_expr(ty)),
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

    fn gen_fn_decl(&self, name: &str, params: &[Param], ret_type: &TypeExpr, body: &Expr, is_async: bool) -> String {
        let async_ = if is_async { "async " } else { "" };
        let sname = Self::sanitize(name);
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
        let body_str = self.gen_expr(body);

        match body {
            Expr::Block { .. } => {
                format!("{}function {}({}){} {}", async_, sname, params_str.join(", "), ret_str, body_str)
            }
            Expr::DoBlock { .. } => {
                format!("{}function {}({}){} {{\n{}\n}}", async_, sname, params_str.join(", "), ret_str, body_str)
            }
            _ => {
                format!("{}function {}({}){} {{\n  return {};\n}}", async_, sname, params_str.join(", "), ret_str, body_str)
            }
        }
    }
}
