// The CROSS-MODULE top-let NAME BRIDGE family: alias main-side var-table ids
// onto module top-lets by NAME + TYPE (per-module VarId regions, no IR-level
// flatten), plus the region-aware per-module bridge and the apply half. Split
// out of lower/mod.rs (max-lines, #852); moved verbatim.

/// The module identity the cross-module top-let bridge keys on: the VERSIONED name when
/// the module carries one (`snaidhm_v0.web.gpu`), else its plain name, with dots turned
/// into underscores. This is byte-for-byte the `origin` the frontend's `module_top_let_var`
/// writes into a synthesized reference's `VarInfo::module_origin`, so a lookup by that
/// field hits — the single spelling both sides of the bridge agree on.
pub(crate) fn module_origin_key(m: &almide_ir::IrModule) -> String {
    m.versioned_name
        .map(|v| v.as_str().to_string())
        .unwrap_or_else(|| m.name.as_str().to_string())
        .replace('.', "_")
}

/// Extracted from `bridge_cross_module_toplets` (codopsy8 complexity sweep, phase 1 of
/// 2): the by-name/by-bare lookup maps of every module top-let. Verbatim.
///
/// The main-side reference entry is SYNTHESIZED by the frontend with an UPPERCASED
/// name (`m.count` → a main var named "COUNT", `module_origin` set — the v0 Rust-const
/// naming convention, expressions.rs's cross-module top-let path). So the bridge keys
/// BOTH maps by the UPPERCASED module-side name: an all-caps `let SYSTEM` matched
/// before by accident; a lowercase `let title`/`var count` silently MISSED the bridge
/// and fell through to the raw numeric-id collision below (reading an UNRELATED
/// top-let's init — a confirmed silent wrong value, `let N = 7; var count = 0` printed
/// 7 for `m.count`; a heap-typed collider surfaced as invalid i64/i32 wasm instead).
/// MUTABILITY: only immutable `let`s are bridged — aliasing a `var` reference to its
/// INIT would const-fold reads across mutations (read-after-`bump()` returning 0).
/// A `var` reference instead has its collided raw entry REMOVED below, so it is
/// honestly UNBOUND → the reference site walls → `--verified` falls back to v0.
/// Keyed by (SOURCE MODULE, UPPERCASED NAME): the ref entry's `module_origin`
/// names which module it points at, so a name defined in TWO modules (view.ROW
/// and layout.ROW — the ceangal zip class) resolves per-module instead of
/// dropping as ambiguous. A bare-name fallback map keeps the pre-existing
/// behavior for refs whose module_origin the frontend left unset.
///
/// The module key is the MANGLED ident ([`module_origin_key`] — dots become
/// underscores, versioned name preferred), which is exactly the spelling
/// `module_top_let_var` stamps into `VarInfo::module_origin`. Keying it by the
/// DOTTED name made every multi-segment module (`ceangal.view`) miss `by_name`
/// unconditionally and fall through to `by_bare`, where `ROW` — defined in BOTH
/// `ceangal.view` (2) and `ceangal.layout` (0) — was dropped as ambiguous (#904).
#[allow(clippy::type_complexity)]
pub(crate) fn bridge_cross_module_toplets_build_lookup(
    ir: &almide_ir::IrProgram,
) -> (
    std::collections::HashMap<(String, String), Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>>,
    std::collections::HashMap<String, Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>>,
) {
    use std::collections::HashMap;
    let mut by_name: HashMap<(String, String), Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>> =
        HashMap::new();
    let mut by_bare: HashMap<String, Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>> = HashMap::new();
    for m in &ir.modules {
        // In-module alias chains (`let white = _white`) leave the alias tl's ty
        // UN-INFERRED — chase to the referent so the bridge carries the REAL
        // (ty, init) and the reader materializes the record directly (the ceangal
        // theme `v.white` class). Bounded hops; a non-Var / cross-module init stops.
        let local: HashMap<u32, (&Ty, &almide_ir::IrExpr)> =
            m.top_lets.iter().map(|t| (t.var.0, (&t.ty, &t.value))).collect();
        for tl in &m.top_lets {
            let Some(info) = m.var_table.entries.get(tl.var.0 as usize) else { continue };
            let mutable = matches!(info.mutability, almide_ir::Mutability::Var);
            let (mut ty, mut init) = (&tl.ty, &tl.value);
            let mut hops = 0;
            // Chase Var inits REGARDLESS of the alias's own ty — the init expr is
            // about to cross regions, and any surviving REGION-LOCAL Var id inside
            // it would capture an unrelated main-side id (a silent wrong-global
            // read when that id's init is const; probe-confirmed as VarId(7)).
            while hops < 4 {
                let almide_ir::IrExprKind::Var { id } = &init.kind else { break };
                let Some((t2, i2)) = local.get(&id.0) else { break };
                if matches!(ty, Ty::Unknown) {
                    ty = t2;
                }
                init = i2;
                hops += 1;
            }
            // An UNANNOTATED module top-let leaves tl.ty Unknown even after the
            // alias chase — the INIT expression's checker-inferred ty is the
            // referent's real type (`let _white = { r: 1.0, … }` infers the record).
            if matches!(ty, Ty::Unknown) && !matches!(init.ty, Ty::Unknown) {
                ty = &init.ty;
            }
            // An OPTION-ctor init whose OWN node ty is also un-inferred (`let MAYBE =
            // some(Cfg { .. })` — the crossmod option_record_toplet): synthesize
            // `Option[payload.ty]` from the payload's inferred type.
            let refined_opt;
            if let Some(r) = refine_option_toplet_ty(ty, init) {
                refined_opt = r;
                ty = &refined_opt;
            }
            // A chased init that STILL references region-local vars (a call init
            // over a sibling const, a nested alias past the hop bound) must NOT
            // cross: the ids would misresolve in the main region. Drop the name
            // (honest unbound wall) rather than ship a capturing expr.
            fn expr_has_var(e: &almide_ir::IrExpr) -> bool {
                use almide_ir::visit::{walk_expr, IrVisitor};
                struct V(bool);
                impl IrVisitor for V {
                    fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                        if matches!(e.kind, almide_ir::IrExprKind::Var { .. }) {
                            self.0 = true;
                        }
                        walk_expr(self, e);
                    }
                }
                let mut v = V(false);
                v.visit_expr(e);
                v.0
            }
            let entry = if !mutable && expr_has_var(init) {
                Option::None
            } else {
                Some((ty.clone(), init, mutable, tl.var))
            };
            by_name
                .entry((module_origin_key(m), info.name.as_str().to_uppercase()))
                .and_modify(|e| *e = Option::None) // second definition ⇒ ambiguous, drop
                .or_insert(entry.clone());
            by_bare
                .entry(info.name.as_str().to_uppercase())
                .and_modify(|e| *e = Option::None) // cross-module name collision ⇒ ambiguous
                .or_insert(entry);
        }
    }
    (by_name, by_bare)
}
