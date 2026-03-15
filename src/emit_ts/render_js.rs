/// TsIR → JavaScript source (Node.js target).
///
/// Input:    &Program (TsIR)
/// Output:   String (JavaScript source for Node.js)
/// Owns:     program-level structure, Node.js entry point, console-based test format
/// Does NOT: expr/stmt/pattern rendering (render_common.rs)

use super::ts_ir::*;
use super::render_common::*;

pub fn render(p: &Program) -> String {
    let mut o = String::new();
    if !p.runtime.is_empty() { o.push_str(&p.runtime); o.push('\n'); }
    namespace_decls(&mut o, &p.namespace_decls);
    for m in &p.modules { module(&mut o, m, false); }
    o.push('\n');
    for td in &p.type_decls { type_decl(&mut o, td, false); o.push_str("\n\n"); }
    for s in &p.top_lets { stmt(&mut o, s, 0); o.push('\n'); }
    for f in &p.functions { function(&mut o, f, 0, false); o.push_str("\n\n"); }
    for t in &p.tests {
        let escaped = t.name.replace('\\', "\\\\").replace('"', "\\\"");
        o.push_str("try { (() => ");
        expr(&mut o, &t.body, 0);
        o.push_str(&format!(
            ")(); console.log(\"  test {} ... ok\"); }} catch(__e) {{ console.log(\"  test {} ... FAILED\"); console.log(\"    \" + __e.message.split(\"\\n\").join(\"\\n    \")); __node_process.exitCode = 1; }}\n\n",
            escaped, escaped
        ));
    }
    if p.entry_point.is_some() {
        o.push_str("// ---- Entry Point ----\n");
        o.push_str("try { main([\"app\", ...__node_process.argv.slice(2)]); } catch (e) { if (e instanceof Error) { console.error(e.message); __node_process.exit(1); } throw e; }\n");
    }
    o
}
