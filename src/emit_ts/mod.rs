mod declarations;
mod ir_expressions;
mod ir_blocks;

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use crate::ast::*;

pub(crate) struct TsEmitter {
    pub(crate) out: String,
    pub(crate) js_mode: bool,
    pub(crate) npm_mode: bool,
    pub(crate) user_modules: Vec<String>,
    /// Tracks which stdlib modules (`__almd_*`) are referenced during codegen.
    pub(crate) used_stdlib: RefCell<HashSet<String>>,
    /// Generic variant unit constructors — need `()` when used as standalone expressions
    pub(crate) generic_variant_unit_ctors: HashSet<String>,
    /// All unit variant names (no payload) — emitted as bare identifiers, not function calls
    pub(crate) unit_variant_names: HashSet<String>,
    /// Variant record constructor names — need `tag` field in TS output
    pub(crate) variant_constructors: HashSet<String>,
    /// True when inside an effect fn body (err() should throw for auto-? propagation)
    pub(crate) in_effect: Cell<bool>,
    /// True when inside a test block (effect fn calls should be caught and wrapped as __Err)
    pub(crate) in_test: Cell<bool>,
    /// Typed IR program (available when type checking succeeded)
    pub(crate) ir_program: Option<crate::ir::IrProgram>,
}

impl TsEmitter {
    fn new() -> Self {
        Self {
            out: String::new(),
            js_mode: false,
            npm_mode: false,
            user_modules: Vec::new(),
            used_stdlib: RefCell::new(HashSet::new()),
            generic_variant_unit_ctors: HashSet::new(),
            unit_variant_names: HashSet::new(),
            variant_constructors: HashSet::new(),
            in_effect: Cell::new(false),
            in_test: Cell::new(false),
            ir_program: None,
        }
    }

    // Helpers

    pub(crate) fn sanitize(name: &str) -> String {
        crate::emit_common::sanitize(name)
    }

    pub(crate) fn map_module(&self, name: &str) -> String {
        // User modules take priority over stdlib
        if self.user_modules.contains(&name.to_string()) {
            name.to_string()
        } else if crate::stdlib::is_stdlib_module(name) {
            self.used_stdlib.borrow_mut().insert(name.to_string());
            format!("__almd_{}", name)
        } else {
            name.to_string()
        }
    }

    pub(crate) fn json_string(s: &str) -> String {
        serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s))
    }

    pub(crate) fn pascal_to_message(name: &str) -> String {
        let mut result = String::new();
        for (i, c) in name.chars().enumerate() {
            if i > 0 && c.is_uppercase() {
                result.push(' ');
                result.push(c.to_lowercase().next().unwrap_or(c));
            } else if i == 0 {
                result.push(c.to_uppercase().next().unwrap_or(c));
            } else {
                result.push(c);
            }
        }
        result
    }
}

pub fn emit_with_modules(program: &Program, modules: &[(String, Program)], ir: Option<&crate::ir::IrProgram>) -> String {
    let mut emitter = TsEmitter::new();
    emitter.ir_program = ir.cloned();
    emitter.emit_program(program, modules);
    emitter.out
}

pub fn emit_js_with_modules(program: &Program, modules: &[(String, Program)], ir: Option<&crate::ir::IrProgram>) -> String {
    let mut emitter = TsEmitter::new();
    emitter.js_mode = true;
    emitter.ir_program = ir.cloned();
    emitter.emit_program(program, modules);
    emitter.out
}

/// Output from the npm package emitter.
pub struct NpmOutput {
    /// `index.js` — ESM user code with runtime imports and exports.
    pub index_js: String,
    /// `index.d.ts` — TypeScript type declarations for public functions.
    pub index_dts: String,
    /// Runtime files: `(relative_path, content)` pairs (e.g. `("_runtime/list.js", "...")`)
    pub runtime_files: Vec<(String, String)>,
    /// `package.json` content.
    pub package_json: String,
}

/// Configuration for npm package generation.
pub struct NpmConfig {
    pub name: String,
    pub version: String,
}

/// Emit an npm-publishable package from an Almide program.
pub fn emit_npm_package(program: &Program, modules: &[(String, Program)], config: &NpmConfig) -> NpmOutput {
    use crate::emit_ts_runtime;

    let mut emitter = TsEmitter::new();
    emitter.js_mode = true;
    emitter.npm_mode = true;
    emitter.emit_npm_program(program, modules);
    let user_code = std::mem::take(&mut emitter.out);

    let used = emitter.used_stdlib.borrow();

    // Post-process: rename __almd_X → __X in user code for cleaner output.
    // Safe because __almd_ prefix is compiler-generated and never appears in user identifiers.
    let mut clean_code = user_code;
    for mod_name in used.iter() {
        clean_code = clean_code.replace(
            &format!("__almd_{}", mod_name),
            &format!("__{}", mod_name),
        );
    }

    // Build import preamble with aliased imports: __almd_X as __X
    let mut imports = String::new();
    imports.push_str("import { __bigop, __div, __deep_eq, __concat, __throw, println, eprintln, assert_eq, assert_ne, assert, unwrap_or, __assert_throws } from \"./_runtime/helpers.js\";\n");
    let mut sorted_modules: Vec<&String> = used.iter().collect();
    sorted_modules.sort();
    for mod_name in &sorted_modules {
        imports.push_str(&format!(
            "import {{ __almd_{name} as __{name} }} from \"./_runtime/{name}.js\";\n",
            name = mod_name
        ));
    }
    imports.push('\n');

    let index_js = format!("{}{}", imports, clean_code);

    // Generate index.d.ts
    let index_dts = emitter.generate_dts(program);

    // Generate runtime files
    let mut runtime_files = Vec::new();

    // helpers.js
    let helpers_src = emit_ts_runtime::get_helpers_source(true);
    let helpers_esm = convert_helpers_to_esm(helpers_src);
    runtime_files.push(("_runtime/helpers.js".to_string(), helpers_esm));

    // Individual stdlib modules
    for mod_name in &sorted_modules {
        if let Some(src) = emit_ts_runtime::get_module_source(mod_name, true) {
            let esm_src = convert_module_to_esm(mod_name, src);
            runtime_files.push((format!("_runtime/{}.js", mod_name), esm_src));
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

    NpmOutput {
        index_js,
        index_dts,
        runtime_files,
        package_json,
    }
}

/// Convert a `const __almd_X = { ... };` block to an ESM export.
fn convert_module_to_esm(name: &str, src: &str) -> String {
    // The source starts with `const __almd_X = {` — replace `const` with `export const`
    let prefix = format!("const __almd_{}", name);
    if let Some(rest) = src.strip_prefix(&prefix) {
        format!("export const __almd_{}{}", name, rest)
    } else {
        format!("export {}", src)
    }
}

/// Convert helper functions to ESM exports.
fn convert_helpers_to_esm(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 200);
    for line in src.lines() {
        if line.starts_with("function ") {
            out.push_str("export ");
            out.push_str(line);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}
