/// TsIR → TypeScript source (Deno target).
///
/// Input:    &Program (TsIR)
/// Output:   String (TypeScript source for Deno)
/// Owns:     program-level structure, Deno entry point, Deno.test format
/// Does NOT: expr/stmt/pattern rendering (render_common.rs)

use super::ts_ir::*;
use super::render_common::*;

pub fn render(p: &Program) -> String {
    let mut o = String::new();
    if !p.runtime.is_empty() { o.push_str(&p.runtime); o.push('\n'); }
    namespace_decls(&mut o, &p.namespace_decls);
    for m in &p.modules { module(&mut o, m, true); }
    o.push('\n');
    for td in &p.type_decls { type_decl(&mut o, td, true); o.push_str("\n\n"); }
    for s in &p.top_lets { stmt(&mut o, s, 0); o.push('\n'); }
    for f in &p.functions { function(&mut o, f, 0, true); o.push_str("\n\n"); }
    for t in &p.tests {
        o.push_str(&format!("Deno.test({}, async () => ", json_string(&t.name)));
        expr(&mut o, &t.body, 0);
        o.push_str(");\n\n");
    }
    if p.entry_point.is_some() {
        o.push_str("// ---- Entry Point ----\n");
        o.push_str("try { main([\"app\", ...Deno.args]); } catch (e) { if (e instanceof Error) { eprintln(e.message); Deno.exit(1); } throw e; }\n");
    }
    o
}
