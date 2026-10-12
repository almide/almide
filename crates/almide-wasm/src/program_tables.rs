//! The program's global and callable-fn tables, built once before emission:
//! top-lets as wasm globals with their dependency init order, and the
//! flattened callable fns (entry + module, Hole-bodied surfaces excluded).
//! Split from lib.rs for the file budget.

use crate::*;

/// Top-lets (root + module) as wasm globals + the dependency init order
/// (split from emit_program for the complexity budget).
#[allow(clippy::type_complexity)]
pub(crate) fn build_globals(
    ir: &IrProgram,
    types: &TypeTable,
) -> (HashMap<GVar, (u32, SliceTy)>, Vec<(VarId, SliceTy)>, Vec<InitLet>) {
    // Top-lets (root + module) become wasm globals — zero-initialized,
    // then set by main's prelude in DEPENDENCY order (C-077: the same
    // `dependency_init_order` the interp uses, so the order matches by
    // construction). Identities are SPACED (#1596): separately-lowered
    // modules each carry their own VarTable starting at VarId 0, so a
    // bare-VarId key collides across tables — the (space, var) pair is
    // the unambiguous name, and every use-site alias (the entry reading
    // `m.SYSTEM`) maps to its declaration's slot.
    use almide_ir::top_let_storage::{
        build_global_tables_spaced, dependency_init_order_spaced,
    };
    let mut global_map: HashMap<GVar, (u32, SliceTy)> = HashMap::new();
    let mut global_decls: Vec<(VarId, SliceTy)> = Vec::new();
    {
        let mut next = G_FIXED_COUNT;
        let mut add = |space: u32,
                       tl: &IrTopLet,
                       map: &mut HashMap<GVar, (u32, SliceTy)>,
                       decls: &mut Vec<(VarId, SliceTy)>| {
            if let Some(sty) = slice_ty_of(&tl.ty, types) {
                map.insert((space, tl.var), (next, sty));
                decls.push((tl.var, sty));
                next += 1;
            }
            // An unsliceable top-let stays out of the pool; reading it
            // refuses at the var site with its own honest reason.
        };
        for tl in &ir.top_lets {
            add(0, tl, &mut global_map, &mut global_decls);
        }
        for (i, m) in ir.modules.iter().enumerate() {
            for tl in &m.top_lets {
                add(i as u32 + 1, tl, &mut global_map, &mut global_decls);
            }
        }
    }
    let (_info, alias, _off) = build_global_tables_spaced(ir);
    // Use-site aliases resolve to the declaration's slot — one lookup at
    // the Var site whatever space the read happens in.
    for (&use_g, &decl_g) in &alias {
        if let Some(&slot) = global_map.get(&decl_g) {
            global_map.insert(use_g, slot);
        }
    }
    let init_order = dependency_init_order_spaced(ir, &alias);
    let mut init_by_var: HashMap<GVar, InitLet> = HashMap::new();
    for tl in &ir.top_lets {
        init_by_var.insert((0, tl.var), InitLet { space: 0, module: None, tl: tl.clone() });
    }
    for (i, m) in ir.modules.iter().enumerate() {
        for tl in &m.top_lets {
            init_by_var.insert(
                (i as u32 + 1, tl.var),
                InitLet { space: i as u32 + 1, module: Some(m.name.as_str().to_string()), tl: tl.clone() },
            );
        }
    }
    let init_lets: Vec<InitLet> =
        init_order.iter().filter_map(|g| init_by_var.get(g).cloned()).collect();
    (global_map, global_decls, init_lets)
}

/// The callable-fn flattening (entry fns + module fns under qualified
/// names, Hole-bodied surfaces excluded) — split from emit_program. The
/// third field is the fn's variable SPACE (0 = entry program, i+1 =
/// `ir.modules[i]` — #1596): module fn bodies index their own VarTable.
pub(crate) fn collect_program_fns(ir: &IrProgram) -> Vec<(&IrFunction, Option<String>, u32)> {
    let mut program_fns: Vec<(&IrFunction, Option<String>, u32)> = ir
        .functions
        .iter()
        .filter(|f| !f.is_test && f.name.as_str() != "main")
        .map(|f| (f, None, 0))
        .collect();
    for (i, m) in ir.modules.iter().enumerate() {
        for f in &m.functions {
            // A Hole body without `@extern` is a bridge SURFACE (`= _`): registering
            // it would shadow the self-host registry's implementation (expr:Hole ×70).
            // An `@extern` Hole is a host binding and registers (#2876).
            let is_surface = matches!(f.body.kind, IrExprKind::Hole) && f.extern_attrs.is_empty();
            if !f.is_test && !is_surface {
                program_fns.push((
                    f,
                    Some(format!("{}.{}", m.name.as_str(), f.name.as_str())),
                    i as u32 + 1,
                ));
            }
        }
    }

    // Signature table first: call sites need indices and types up front.
    program_fns
}
