// ── Record field defaults at the construction site (#3167) ──────
//
// A record literal inside the module that declares its type, which leaves out
// a defaulted field, gets the default lowered INTO the literal here. The
// default used to stay on the type declaration, and codegen pasted that one
// lowered copy into every literal. The copy never ran through the passes a
// function body runs through. A call to a sibling fn (`f: Int = base()`)
// never got the module prefix the module's own calls get, and the native
// build named a function that does not exist (rustc E0425).
//
// The default is lowered in MODULE scope: the literal's locals and const
// params are hidden, so `e: Int = step` names the module's `step` even beside
// a local `step`, exactly as the declaration means it. A literal in ANOTHER
// module never reaches here with a field missing: the checker writes those
// defaults in, qualified (check/call_defaults.rs, #3165).

use std::collections::HashMap;
use crate::ast;
use almide_ir::*;
use crate::types::Ty;
use crate::intern::{Sym, sym};
use super::LowerCtx;
use super::expressions::lower_expr;

/// The defaulted fields of every record type and record-payload case the
/// module being lowered declares, keyed like the type table: `mod.Type` and
/// `mod.Type.Case`. The entry program keeps its spliced defaults (its bare
/// names need no prefix).
pub(super) fn collect_field_defaults(ctx: &mut LowerCtx, prog: &ast::Program) {
    let Some(module) = ctx.current_module else { return };
    let mut out: HashMap<Sym, Vec<(Sym, ast::Expr)>> = HashMap::new();
    let mut put = |key: String, fields: &[ast::FieldType]| {
        let defs: Vec<(Sym, ast::Expr)> = fields.iter()
            .filter_map(|f| f.default.as_ref().map(|d| (f.name, d.clone())))
            .collect();
        if !defs.is_empty() {
            out.insert(sym(&key), defs);
        }
    };
    for decl in &prog.decls {
        let ast::Decl::Type { name, ty, .. } = decl else { continue };
        let key = format!("{}.{}", module, name);
        match ty {
            ast::TypeExpr::Record { fields } | ast::TypeExpr::OpenRecord { fields } => put(key, fields),
            ast::TypeExpr::Variant { cases, .. } => {
                for c in cases {
                    if let ast::VariantCase::Record { name: case, fields } = c {
                        put(format!("{}.{}", key, case), fields);
                    }
                }
            }
            _ => {}
        }
    }
    ctx.field_defaults = out;
}

/// Lower the defaults of the fields `fields` leaves out, when the literal's
/// type is one this module declares.
pub(super) fn fill_field_defaults(ctx: &mut LowerCtx, ctor: Option<Sym>, ty: &Ty, fields: &mut Vec<(Sym, IrExpr)>) {
    let Some(ctor) = ctor else { return };
    let case_key = match ty {
        Ty::Named(t, _) => Some(sym(&format!("{}.{}", t, ctor))),
        _ => None,
    };
    let Some(defaults) = ctx.field_defaults.get(&ctor)
        .or_else(|| case_key.and_then(|k| ctx.field_defaults.get(&k)))
        .cloned()
    else { return };
    let missing: Vec<(Sym, ast::Expr)> = defaults.into_iter()
        .filter(|(f, _)| !fields.iter().any(|(n, _)| n == f))
        .collect();
    if missing.is_empty() {
        return;
    }
    // Module scope: only the top-level scope (the module's top-lets) stays.
    let locals = ctx.scopes.split_off(1);
    let const_params = std::mem::take(&mut ctx.const_param_vars);
    for (field, default) in &missing {
        let value = lower_expr(ctx, default);
        fields.push((*field, value));
    }
    ctx.scopes.extend(locals);
    ctx.const_param_vars = const_params;
}
