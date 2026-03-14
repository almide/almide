/// TypeScript/JavaScript code generation — entry point.
///
/// Input:    &IrProgram
/// Output:   String (TS/JS source), or NpmOutput (npm package)
/// Owns:     pipeline orchestration (lower → render), npm packaging, .d.ts generation
/// Does NOT: codegen decisions (lower_ts.rs), rendering (render_ts.rs)
///
/// Architecture: 2-stage pipeline mirroring Rust codegen:
///   IR → TsIR (lower_ts.rs) → String (render_ts.rs)

pub mod ts_ir;
pub mod lower_decls;
pub mod lower_ts;
pub mod render_ts;

use crate::ir::{IrProgram, IrVisibility, IrTypeDeclKind, IrVariantKind};
use crate::types::Ty;

// ── Public API (same signatures as before) ───────────────────────

/// Emit TypeScript source (Deno target).
pub fn emit_with_modules(ir: &IrProgram) -> String {
    let opts = lower_ts::LowerOpts { js_mode: false, npm_mode: false };
    let prog = lower_ts::lower(ir, &opts);
    render_ts::program(&prog)
}

/// Emit JavaScript source (Node.js target).
pub fn emit_js_with_modules(ir: &IrProgram) -> String {
    let opts = lower_ts::LowerOpts { js_mode: true, npm_mode: false };
    let prog = lower_ts::lower(ir, &opts);
    render_ts::program(&prog)
}

/// Output from the npm package emitter.
pub struct NpmOutput {
    pub index_js: String,
    pub index_dts: String,
    pub runtime_files: Vec<(String, String)>,
    pub package_json: String,
}

/// Configuration for npm package generation.
pub struct NpmConfig {
    pub name: String,
    pub version: String,
}

/// Emit an npm-publishable package from an Almide program.
pub fn emit_npm_package(ir: &IrProgram, config: &NpmConfig) -> NpmOutput {
    use crate::emit_ts_runtime;

    let opts = lower_ts::LowerOpts { js_mode: true, npm_mode: true };
    let prog = lower_ts::lower(ir, &opts);
    let rendered = render_ts::npm_program(&prog);

    // Post-process: rename __almd_X → __X for cleaner output
    let mut clean_code = rendered.code;
    // Detect used stdlib modules from __almd_ references in the output
    let used_modules = detect_used_stdlib(&clean_code);
    for mod_name in &used_modules {
        clean_code = clean_code.replace(
            &format!("__almd_{}", mod_name),
            &format!("__{}", mod_name),
        );
    }

    // Build import preamble
    let mut imports = String::new();
    imports.push_str("import { __bigop, __div, __deep_eq, __concat, __throw, println, eprintln, assert_eq, assert_ne, assert, unwrap_or, __assert_throws } from \"./_runtime/helpers.js\";\n");
    for mod_name in &used_modules {
        imports.push_str(&format!(
            "import {{ __almd_{name} as __{name} }} from \"./_runtime/{name}.js\";\n",
            name = mod_name
        ));
    }
    imports.push('\n');

    let index_js_body = format!("{}{}", imports, clean_code);

    // Exports
    let mut exports = Vec::new();
    for name in &rendered.public_fns {
        let clean = crate::emit_common::to_clean_export_name(name);
        if clean == *name { exports.push(name.clone()); }
        else { exports.push(format!("{} as {}", name, clean)); }
    }
    for name in &rendered.public_variants { exports.push(name.clone()); }
    let export_line = if exports.is_empty() { String::new() }
        else { format!("export {{ {} }};\n", exports.join(", ")) };

    let index_js = format!("{}{}", index_js_body, export_line);

    // Generate .d.ts
    let index_dts = generate_dts(ir);

    // Runtime files
    let mut runtime_files = Vec::new();
    let helpers_src = emit_ts_runtime::get_helpers_source(true);
    runtime_files.push(("_runtime/helpers.js".to_string(), convert_helpers_to_esm(helpers_src)));
    for mod_name in &used_modules {
        if let Some(src) = emit_ts_runtime::get_module_source(mod_name, true) {
            runtime_files.push((format!("_runtime/{}.js", mod_name), convert_module_to_esm(mod_name, src)));
        }
    }

    // package.json
    let package_json = format!(
        r#"{{
  "name": "{}",
  "version": "{}",
  "type": "module",
  "main": "index.js",
  "types": "index.d.ts",
  "exports": {{
    ".": {{
      "import": "./index.js",
      "types": "./index.d.ts"
    }}
  }},
  "engines": {{
    "node": ">=22"
  }}
}}
"#,
        config.name, config.version
    );

    NpmOutput { index_js, index_dts, runtime_files, package_json }
}

// ── .d.ts generation ─────────────────────────────────────────────

fn generate_dts(ir: &IrProgram) -> String {
    let mut dts = String::new();
    for func in &ir.functions {
        if func.visibility != IrVisibility::Public { continue; }
        let sname = crate::emit_common::sanitize(&func.name);
        let clean = crate::emit_common::to_clean_export_name(&sname);
        let ps: Vec<String> = func.params.iter()
            .filter(|p| p.name != "self")
            .map(|p| {
                let pname = crate::emit_common::to_clean_export_name(&crate::emit_common::sanitize(&p.name));
                format!("{}: {}", pname, ty_to_ts(&p.ty))
            }).collect();
        let ret = ty_to_ts(&func.ret_ty);
        let ret_str = if func.is_async { format!("Promise<{}>", ret) } else { ret };
        dts.push_str(&format!("export declare function {}({}): {};\n", clean, ps.join(", "), ret_str));
    }
    for td in &ir.type_decls {
        if td.visibility != IrVisibility::Public { continue; }
        match &td.kind {
            IrTypeDeclKind::Record { fields } => {
                let fs: Vec<String> = fields.iter().map(|f| format!("  {}: {};", f.name, ty_to_ts(&f.ty))).collect();
                dts.push_str(&format!("export interface {} {{\n{}\n}}\n", td.name, fs.join("\n")));
            }
            IrTypeDeclKind::Alias { target } => {
                if let Ty::OpenRecord { fields, .. } = target {
                    let fs: Vec<String> = fields.iter().map(|(n, t)| format!("  {}: {};", n, ty_to_ts(t))).collect();
                    dts.push_str(&format!("export interface {} {{\n{}\n}}\n", td.name, fs.join("\n")));
                } else {
                    dts.push_str(&format!("export type {} = {};\n", td.name, ty_to_ts(target)));
                }
            }
            IrTypeDeclKind::Variant { cases, .. } => {
                for case in cases {
                    let tag_str = format!("tag: \"{}\"", case.name);
                    match &case.kind {
                        IrVariantKind::Unit => {
                            dts.push_str(&format!("export declare const {}: {{ {} }};\n", case.name, tag_str));
                        }
                        IrVariantKind::Tuple { fields } => {
                            let ps: Vec<String> = fields.iter().enumerate().map(|(i, f)| format!("_{}: {}", i, ty_to_ts(f))).collect();
                            let ret_fields: Vec<String> = std::iter::once(tag_str.clone())
                                .chain(fields.iter().enumerate().map(|(i, f)| format!("_{}: {}", i, ty_to_ts(f)))).collect();
                            dts.push_str(&format!("export declare function {}({}): {{ {} }};\n", case.name, ps.join(", "), ret_fields.join(", ")));
                        }
                        IrVariantKind::Record { fields } => {
                            let ps: Vec<String> = fields.iter().map(|f| format!("{}: {}", f.name, ty_to_ts(&f.ty))).collect();
                            let ret_fields: Vec<String> = std::iter::once(tag_str.clone())
                                .chain(fields.iter().map(|f| format!("{}: {}", f.name, ty_to_ts(&f.ty)))).collect();
                            dts.push_str(&format!("export declare function {}({}): {{ {} }};\n", case.name, ps.join(", "), ret_fields.join(", ")));
                        }
                    }
                }
            }
        }
    }
    dts
}

fn ty_to_ts(ty: &Ty) -> String {
    match ty {
        Ty::Int | Ty::Float => "number".into(),
        Ty::String => "string".into(),
        Ty::Bool => "boolean".into(),
        Ty::Unit => "void".into(),
        Ty::List(inner) => format!("{}[]", ty_to_ts(inner)),
        Ty::Map(k, v) => format!("Map<{}, {}>", ty_to_ts(k), ty_to_ts(v)),
        Ty::Option(inner) => format!("{} | null", ty_to_ts(inner)),
        Ty::Result(ok, _) => ty_to_ts(ok),
        Ty::Tuple(elems) => {
            let ts: Vec<String> = elems.iter().map(|e| ty_to_ts(e)).collect();
            format!("[{}]", ts.join(", "))
        }
        Ty::Fn { params, ret } => {
            let ps: Vec<String> = params.iter().enumerate().map(|(i, p)| format!("_{}: {}", i, ty_to_ts(p))).collect();
            format!("({}) => {}", ps.join(", "), ty_to_ts(ret))
        }
        Ty::Record { fields } | Ty::OpenRecord { fields, .. } => {
            let fs: Vec<String> = fields.iter().map(|(n, t)| format!("{}: {}", n, ty_to_ts(t))).collect();
            format!("{{ {} }}", fs.join(", "))
        }
        Ty::Named(name, _) => match name.as_str() { "Path" => "string".into(), other => other.into() },
        Ty::TypeVar(name) => name.clone(),
        Ty::Union(members) => members.iter().map(|m| ty_to_ts(m)).collect::<Vec<_>>().join(" | "),
        Ty::Variant { name, .. } => name.clone(),
        Ty::Unknown => "any".into(),
    }
}

// ── Utilities ────────────────────────────────────────────────────

fn detect_used_stdlib(code: &str) -> Vec<String> {
    let mut used = std::collections::HashSet::new();
    // Scan for __almd_XXX patterns — each XXX is a stdlib module name
    let prefix = "__almd_";
    let mut start = 0;
    while let Some(pos) = code[start..].find(prefix) {
        let abs = start + pos + prefix.len();
        let end = code[abs..].find(|c: char| !c.is_alphanumeric() && c != '_').map(|e| abs + e).unwrap_or(code.len());
        let name = &code[abs..end];
        if !name.is_empty() { used.insert(name.to_string()); }
        start = end;
    }
    let mut sorted: Vec<String> = used.into_iter().collect();
    sorted.sort();
    sorted
}

fn convert_module_to_esm(name: &str, src: &str) -> String {
    let prefix = format!("const __almd_{}", name);
    if let Some(rest) = src.strip_prefix(&prefix) {
        format!("export const __almd_{}{}", name, rest)
    } else {
        format!("export {}", src)
    }
}

fn convert_helpers_to_esm(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 200);
    for line in src.lines() {
        if line.starts_with("function ") { out.push_str("export "); }
        out.push_str(line);
        out.push('\n');
    }
    out
}
