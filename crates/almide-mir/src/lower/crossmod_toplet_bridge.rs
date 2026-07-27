// The CROSS-MODULE top-let NAME BRIDGE family: alias main-side var-table ids
// onto module top-lets by NAME + TYPE (per-module VarId regions, no IR-level
// flatten), plus the region-aware per-module bridge and the apply half. Split
// out of lower/mod.rs (max-lines, #852); moved verbatim.

/// CROSS-MODULE top-let NAME BRIDGE: the main program references `toplib.SYSTEM` through a
/// MAIN-side VarId (per-module VarId regions — no IR-level flatten), while the globals union
/// keys the MODULE-side id — so the reference was "unbound" (or COLLIDED with an unrelated
/// module id, resolving to the wrong init). Alias every main-side var-table id whose NAME +
/// TYPE match a module top-let's onto that top-let's (ty, init). By-NAME, so an AMBIGUOUS
/// name (a top-let in two modules) is skipped — those references stay walled; a same-named
/// function LOCAL is harmless (locals resolve through `value_of` first — the globals map is
/// only consulted for otherwise-unbound ids). Registration only: the reference site still
/// materializes through the CONST-init machinery (count-exact, unchanged certs).
pub fn bridge_cross_module_toplets(
    ir: &almide_ir::IrProgram,
    globals: &mut std::collections::HashMap<almide_ir::VarId, Ty>,
    global_inits: &mut std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr>,
    // #782: main-side synthesized ref VarId → module-side MUTABLE var VarId.
    // With the v0 fallback retired, a mutable cross-module reference must LOWER
    // instead of walling: the caller aliases the main-side id onto the module
    // var's linear-memory slot, so reads and assigns route through the SAME
    // storage the owning module's fns use (no const-fold hazard — the slot is
    // real storage, not an init alias).
    mutable_aliases: &mut std::collections::HashMap<almide_ir::VarId, almide_ir::VarId>,
) {
    // Sequential-phase split (codopsy8 complexity sweep): phase 1 builds the by-name/
    // by-bare lookup maps (self-contained — never touches globals/global_inits/
    // mutable_aliases); phase 2 reads those FINISHED, read-only maps to populate the 3
    // output maps. Pure text-move, no logic change.
    let (by_name, by_bare) = bridge_cross_module_toplets_build_lookup(ir);
    bridge_cross_module_toplets_apply(ir, &by_name, &by_bare, globals, global_inits, mutable_aliases);
}

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

/// The MODULE-region twin of [`bridge_cross_module_toplets`] (#904): a `mod.NAME` reference
/// written INSIDE another module's function (ceangal's `render.almd` reading `v.ROW`) gets a
/// synthesized reference VarId in the REFERENCING module's own numbering region — which no
/// top-let ever owns, so the shared module-region globals union left it unbound and every
/// function using it walled ("use of unbound var"). Main's references have been bridged since
/// #500; module-to-module ones never were.
///
/// Only entries whose `module_origin` names a DIFFERENT module participate. That single test
/// is what keeps the walk safe: `disambiguate_module_global_regions` PADS a module's var table
/// up to its remapped top-let ids with clones of that module's OWN top-let info, so every pad
/// entry carries this module's own ident and is skipped — bridging one would bind an unrelated
/// id to a wrong init. Immutable `let`s only, and never overriding an existing binding: this
/// adds resolutions, it never rewrites or removes one (the mutable-`var` alias story stays
/// main-only, where the slot map's raw-u32 keying is unambiguous).
/// Returns, per module NAME, only the entries the bridge ADDS (a module that resolves
/// nothing is absent) — the caller overlays them on the shared union it already holds.
#[allow(clippy::type_complexity)]
pub fn module_region_toplet_bridges(
    ir: &almide_ir::IrProgram,
    globals: &std::collections::HashMap<almide_ir::VarId, Ty>,
) -> std::collections::HashMap<
    String,
    (
        std::collections::HashMap<almide_ir::VarId, Ty>,
        std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr>,
    ),
> {
    // ONE lookup build for the whole program — the map is keyed by (module ident, NAME),
    // so every module's walk reads the same finished table.
    let (by_name, by_bare) = bridge_cross_module_toplets_build_lookup(ir);
    let mut out = std::collections::HashMap::new();
    for module in &ir.modules {
        let own = module_origin_key(module);
        let mut g = std::collections::HashMap::new();
        let mut gi = std::collections::HashMap::new();
        for (i, info) in module.var_table.entries.iter().enumerate() {
            let Some(origin) = info.module_origin.as_deref() else { continue };
            if origin == own {
                continue;
            }
            let id = almide_ir::VarId(i as u32);
            if globals.contains_key(&id) {
                continue;
            }
            let looked = by_name
                .get(&(origin.to_string(), info.name.as_str().to_uppercase()))
                .or_else(|| by_bare.get(&info.name.as_str().to_uppercase()));
            let Some(Some((ty, init, false, _))) = looked else { continue };
            if !bridged_ref_ty_agrees(ty, &info.ty) {
                continue;
            }
            crate::trace::trace("ALMIDE_MG_DEBUG", || {
                format!(
                    "[bridge-mod] {} {id:?} {}@{origin} -> {ty:?}",
                    module.name.as_str(),
                    info.name.as_str()
                )
            });
            g.insert(id, ty.clone());
            gi.insert(id, (*init).clone());
        }
        if !g.is_empty() {
            out.insert(module.name.as_str().to_string(), (g, gi));
        }
    }
    out
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

/// Extracted from `bridge_cross_module_toplets` (codopsy8 complexity sweep, phase 2 of
/// 2): reads the (already-finished, read-only) `by_name`/`by_bare` lookup maps from
/// phase 1 to populate `globals`/`global_inits`/`mutable_aliases`. OVERRIDES an existing
/// (module-raw, possibly colliding) entry — callers order the composition as: module
/// union → this bridge → main top-lets re-inserted last, so the precedence is main >
/// bridged-name > raw module id. Verbatim.
/// The three accumulator maps [`bridge_cross_module_toplets_apply_one`] writes into,
/// bundled so the per-entry helper stays under the max-params budget (7 → 5 — `id`,
/// `info`, `by_name`, `by_bare`, `targets`).
struct BridgeApplyTargets<'a> {
    globals: &'a mut std::collections::HashMap<almide_ir::VarId, Ty>,
    global_inits: &'a mut std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr>,
    mutable_aliases: &'a mut std::collections::HashMap<almide_ir::VarId, almide_ir::VarId>,
}

#[allow(clippy::type_complexity)]
fn bridge_cross_module_toplets_apply(
    ir: &almide_ir::IrProgram,
    by_name: &std::collections::HashMap<(String, String), Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>>,
    by_bare: &std::collections::HashMap<String, Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>>,
    globals: &mut std::collections::HashMap<almide_ir::VarId, Ty>,
    global_inits: &mut std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr>,
    mutable_aliases: &mut std::collections::HashMap<almide_ir::VarId, almide_ir::VarId>,
) {
    let mut targets = BridgeApplyTargets { globals, global_inits, mutable_aliases };
    // Each iteration writes only its OWN entry's keys (`id`) into the three maps — no
    // iteration reads back another iteration's write — so the loop body is a fold of
    // independent writes and factoring it into a per-entry helper changes nothing
    // observable (same split family as the fold-independent-writes passes elsewhere).
    for (i, info) in ir.var_table.entries.iter().enumerate() {
        let id = almide_ir::VarId(i as u32);
        // Only the frontend-synthesized cross-module reference entries participate
        // (module_origin set) — a main-local name that happens to match a module
        // top-let must not be rebound.
        if info.module_origin.is_none() {
            continue;
        }
        crate::trace::trace("ALMIDE_MG_DEBUG", || {
            format!(
                "[bridge] id={:?} name={:?} origin={:?} ty={:?} by_name_hit={} by_bare_hit={}",
                id,
                info.name.as_str(),
                info.module_origin,
                info.ty,
                by_name.contains_key(&(
                    info.module_origin.clone().unwrap_or_default(),
                    info.name.as_str().to_uppercase()
                )),
                by_bare.contains_key(&info.name.as_str().to_uppercase()),
            )
        });
        bridge_cross_module_toplets_apply_one(id, info, by_name, by_bare, &mut targets);
    }
}

#[allow(clippy::type_complexity)]
fn bridge_cross_module_toplets_apply_one(
    id: almide_ir::VarId,
    info: &almide_ir::VarInfo,
    by_name: &std::collections::HashMap<(String, String), Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>>,
    by_bare: &std::collections::HashMap<String, Option<(Ty, &almide_ir::IrExpr, bool, almide_ir::VarId)>>,
    targets: &mut BridgeApplyTargets<'_>,
) {
    let looked_up = info
        .module_origin
        .as_ref()
        .and_then(|mo| by_name.get(&(mo.clone(), info.name.as_str().to_uppercase())))
        .or_else(|| by_bare.get(&info.name.as_str().to_uppercase()));
    match looked_up {
        // An UNKNOWN-typed reference entry (the frontend leaves an alias-let's
        // synthesized ref un-inferred — `let white = _white` read as `v.white`,
        // the ceangal theme class) takes the MODULE side's type: the name is
        // unique (the ambiguity arm below dropped collisions), so the module
        // top-let IS the referent. A concretely-typed ref still must agree.
        Some(Some((ty, init, mutable, _mod_id))) if !mutable && bridged_ref_ty_agrees(ty, &info.ty) => {
            targets.globals.insert(id, ty.clone());
            targets.global_inits.insert(id, (*init).clone());
        }
        // #782: a MUTABLE cross-module reference aliases onto the module
        // var's storage slot instead of walling (the v0 fallback that used
        // to absorb it is retired). The init is never shipped — only the
        // slot identity — so the const-fold hazard that justified the old
        // exclusion cannot occur.
        Some(Some((ty, _init, true, mod_id))) if bridged_ref_ty_agrees(ty, &info.ty) => {
            targets.mutable_aliases.insert(id, *mod_id);
            targets.globals.remove(&id);
            targets.global_inits.remove(&id);
        }
        _ => {
            // Unmatched reference: purge any raw module-id numeric collision
            // so the reference is honestly unbound (a diagnosed wall), never
            // an unrelated init.
            targets.globals.remove(&id);
            targets.global_inits.remove(&id);
        }
    }
}
