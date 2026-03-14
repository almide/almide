/// TsIR → npm package output (ESM JavaScript).
///
/// Input:    &Program (TsIR)
/// Output:   NpmRender (code + public function/variant names for export)
/// Owns:     npm-specific program structure, export collection
/// Does NOT: expr/stmt/pattern rendering (render_common.rs),
///           packaging/imports/.d.ts (mod.rs orchestrates those)

use super::ts_ir::*;
use super::render_common::*;

pub struct NpmRender {
    pub code: String,
    pub public_fns: Vec<String>,
    pub public_variants: Vec<String>,
}

pub fn render(p: &Program) -> NpmRender {
    let mut o = String::new();
    namespace_decls(&mut o, &p.namespace_decls);
    for m in &p.modules { module(&mut o, m, false); }
    o.push('\n');
    for td in &p.type_decls { type_decl(&mut o, td, false); o.push_str("\n\n"); }
    for s in &p.top_lets { stmt(&mut o, s, 0); o.push('\n'); }

    let mut public_fns = Vec::new();
    let mut public_variants = Vec::new();
    for f in &p.functions {
        function(&mut o, f, 0, false);
        o.push_str("\n\n");
        if f.is_export { public_fns.push(f.name.clone()); }
    }
    for td in &p.type_decls {
        if let TypeDecl::VariantCtors(ctors) = td {
            for c in ctors { public_variants.push(c.name.clone()); }
        }
    }

    NpmRender { code: o, public_fns, public_variants }
}
