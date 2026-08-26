
/// EAGER GLOBAL-INIT semantics (C-007): v0 evaluates every ABORTABLE
/// top-let initializer at startup. Synthesize `__global_init` binding each
/// CALL-FREE SCALAR initializer and have `_start` call it before `$main`.
/// Call-bearing/heap inits keep per-use/wall handling.
fn synthesize_global_init(
    ir: &almide_ir::IrProgram,
    layouts: &PipelineLayouts,
    functions: &mut Vec<crate::MirFunction>,
) {
    fn has_call(e: &almide_ir::IrExpr) -> bool {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct C(bool);
        impl IrVisitor for C {
            fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                if matches!(
                    e.kind,
                    almide_ir::IrExprKind::Call { .. } | almide_ir::IrExprKind::RuntimeCall { .. }
                ) {
                    self.0 = true;
                }
                walk_expr(self, e);
            }
        }
        let mut c = C(false);
        c.visit_expr(e);
        c.0
    }
    let mut max_var = 0u32;
    for (v, _) in &layouts.globals {
        max_var = max_var.max(v.0);
    }
    {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct M(u32);
        impl IrVisitor for M {
            fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                if let almide_ir::IrExprKind::Var { id } = &e.kind {
                    self.0 = self.0.max(id.0);
                }
                walk_expr(self, e);
            }
        }
        let mut m = M(max_var);
        for f in &ir.functions {
            m.visit_expr(&f.body);
        }
        max_var = m.0;
    }
    let mut stmts: Vec<almide_ir::IrStmt> = Vec::new();
    let mut ordered: Vec<_> = ir
        .top_lets
        .iter()
        .chain(ir.modules.iter().flat_map(|m| m.top_lets.iter()))
        .collect();
    ordered.sort_by_key(|tl| tl.var.0);
    // A later initializer may READ an earlier global — INLINE each processed init into its
    // dependents (declaration order; all call-free ⇒ pure, finite substitution).
    let mut subst: std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr> =
        std::collections::HashMap::new();
    for tl in ordered {
        let scalar = !crate::lower::is_heap_ty(&tl.ty);
        if scalar && !has_call(&tl.value) {
            let mut value = tl.value.clone();
            for (gv, ge) in &subst {
                value = almide_ir::substitute::substitute_var_in_expr(&value, *gv, ge);
            }
            subst.insert(tl.var, value.clone());
            max_var += 1;
            stmts.push(almide_ir::IrStmt {
                kind: almide_ir::IrStmtKind::Bind {
                    var: almide_ir::VarId(max_var),
                    mutability: almide_ir::Mutability::Let,
                    ty: tl.ty.clone(),
                    value,
                },
                span: None,
            });
        }
    }
    if !stmts.is_empty() {
        let body = almide_ir::IrExpr {
            kind: almide_ir::IrExprKind::Block { stmts, expr: None },
            ty: almide_lang::types::Ty::Unit,
            span: Default::default(),
            def_id: None,
        };
        let init_fn = almide_ir::IrFunction {
            name: almide_lang::intern::sym("__global_init"),
            params: vec![],
            ret_ty: almide_lang::types::Ty::Unit,
            body,
            is_effect: false,
                    is_test: false,
            generics: None,
            extern_attrs: vec![],
            export_attrs: vec![],
            attrs: vec![],
            visibility: almide_ir::IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            module_origin: None,
            mutated_params: vec![], // fresh-fn: synthesized zero-param runner
        };
        if let Ok(mir) = crate::lower::lower_function(&init_fn, &layouts.globals) {
            functions.push(mir);
        }
    }

}

fn try_render_wasm_source_impl_rest(
    ir: &mut almide_ir::IrProgram,
    verbose: bool,
) -> Result<String, LowerError> {
    // This is where MIR lowering runs, so this is where STRICT value mode has to
    // still be live. Asserting it here — rather than trusting the caller to hold
    // the guard long enough — is the structural half of the fix: when the mode was
    // a process-global that the IR phase set and never reset, lowering inherited it
    // by leak, and scoping that flag to a guard silently moved the boundary so every
    // deferred `Op::Const` ZERO rendered as an executable 0 (nightly fuzz: wasm
    // printed 0 where native printed 100). Nothing in the types said the guard had
    // to outlive the IR phase; this says it. It cannot fire on the permissive
    // caps-counting path, which never reaches this function.
    assert!(
        crate::lower::strict_values(),
        "wasm render reached MIR lowering with STRICT value mode off — a deferred \
         Const-0 would render as an executable 0. The mode must be held for the \
         WHOLE render, not just the IR phase (see try_render_wasm_source_impl)."
    );
    let layouts = collect_pipeline_layouts(ir);

    let CrossModuleFns { mut module_fn_sibs, mut inlined_fns, all_fns } =
        inline_and_classify_cross_module_fns(ir, &layouts.main_globals, &layouts.record_layouts);

    bridge_cross_module_derived_methods(ir, &mut inlined_fns, &mut module_fn_sibs);

    let mutable_tls = assign_mutable_global_slots(ir, &layouts.mutable_toplet_aliases)?;
    crate::trace::trace("ALMIDE_MG_DEBUG", || {
        format!(
            "[mg] mutable_tls={:?} aliases={:?}",
            mutable_tls.iter().map(|tl| tl.var).collect::<Vec<_>>(),
            layouts.mutable_toplet_aliases,
        )
    });
    let mutable_global_count = mutable_tls.len() as u32;

    repair_and_substitute_globals(ir, &mut inlined_fns, &mut module_fn_sibs, &layouts, &all_fns);

    let mut fn_walls: std::collections::HashMap<String, crate::lower::LowerError> = std::collections::HashMap::new();
    let mut functions = lower_main_and_sibling_fns(
        &inlined_fns,
        &module_fn_sibs,
        &layouts,
        ir.functions.len(),
        verbose,
        &mut fn_walls,
    );
    let main_wall = fn_walls.get("main").cloned();

    // Stage 1 probe: charge insertion on the SHARED user-fn MIR, before any
    // leg-specific pass. The native leg calls the same pass at the same point.
    crate::charge_probe::insert_probe_charges(&mut functions);

    // Self-append windows (`x = x + [e]`, incl. the `list.push` desugar) →
    // the amortized-O(1) `__list_append1` (self-hosted in list_concat.almd —
    // §4.1: the hand-written WAT floor must not grow). MUST run BEFORE the
    // runtime link: the linker's fixpoint scans CallFn names, and this rewrite
    // is what introduces the `__list_append1` calls it needs to link.
    crate::concat_to_append::rewrite_self_append(&mut functions);

    synthesize_and_link_runtime_fns(&mut functions, &mutable_tls, &layouts, verbose)?;

    synthesize_global_init(ir, &layouts, &mut functions);
    // If `main` itself was WALLED, there is no `$main` — yet the renderer emits
    // `(func (export "_start") (call $main))`. Wall the WHOLE program cleanly instead of a
    // main-less (invalid) module.
    if !functions.iter().any(|f| f.name == "main") {
        // Name the CAUSE, not just the absence. `main` walling is the single
        // most common wall, and reporting it as "main is outside the subset"
        // collapsed every distinct reason into one bucket that no burn-down
        // could act on (#812).
        // The wrapper PRESERVES the inner wall's span (#931): nesting adds
        // context to the reason, never strips the location.
        return Err(match &main_wall {
            Some(inner) => LowerError::shaped(
                inner.span(),
                inner.shape(),
                format!("main is outside the MIR-lowering subset: {inner}"),
            ),
            None => LowerError::Unsupported(
                "main is outside the MIR-lowering subset (no main in the IR)".into(),
            ),
        });
    }

    // `pub fn` EXPORT roots (#457): a Public non-test MAIN-program fn must be a named wasm
    // export (host-invocable, the v0 emitter's export contract). One that LOWERED gets an
    // `(export …)` directive; one that WALLED cannot be exported — decline the WHOLE module
    // so the `--verified` pipeline falls back to v0 (which exports it) rather than shipping
    // an artifact silently missing a public entry point.
    let mut exports: Vec<(String, String, Vec<bool>, Option<bool>)> = Vec::new();
    let is_float_ty = |t: &almide_lang::types::Ty| {
        matches!(
            t,
            almide_lang::types::Ty::Float
                | almide_lang::types::Ty::Float32
                | almide_lang::types::Ty::Float64
        )
    };
    for func in &ir.functions {
        if !func.is_test
            && func.name.as_str() != "main"
            && !func.generics.as_ref().map_or(false, |g| !g.is_empty())
            && matches!(func.visibility, almide_ir::IrVisibility::Public)
        {
            let n = func.name.as_str();
            if functions.iter().any(|f| f.name == n) {
                // `@export(wasm, "sym")` overrides the export name (v0's criterion,
                // mod_p3.rs). v1's internal value model carries a Float as raw i64 BITS;
                // the renderer emits a reinterpret wrapper for Float-bearing signatures
                // so the public ABI presents real f64s (v0 parity).
                let export_name = func
                    .export_attrs
                    .iter()
                    .find(|a| a.target.as_str() == "wasm")
                    .map(|a| a.symbol.to_string())
                    .unwrap_or_else(|| n.to_string());
                let param_floats: Vec<bool> =
                    func.params.iter().map(|p| is_float_ty(&p.ty)).collect();
                let ret_float = match &func.ret_ty {
                    almide_lang::types::Ty::Unit => None,
                    t => Some(is_float_ty(t)),
                };
                // A wasm export NAME must be unique across the module — a second
                // `(export "x" …)` makes the artifact unparseable, which every
                // consumer hits before a single Almide semantic runs. Emitting one
                // is never a legal outcome, so this is a WALL (a named build-time
                // refusal), never a silent dedup: the duplicate always means an
                // upstream generator keyed a helper by SITE instead of by
                // instantiation (#1357), and a quiet dedup would hide the next one
                // instead of reporting it. A wall is recoverable; a broken artifact
                // is exactly what the trust spine exists to make impossible.
                if exports.iter().any(|(e, _, _, _)| e == &export_name) {
                    return Err(LowerError::Unsupported(format!(
                        "duplicate wasm export name `{export_name}` — two functions \
                         claim it, which is an invalid module. A generated per-type \
                         helper must be emitted once per INSTANTIATION, never once \
                         per use site"
                    )));
                }
                exports.push((export_name, n.to_string(), param_floats, ret_float));
            } else {
                // Name the CAUSE, not just the enclosing construct. The bare
                // "must carry its export" text described the CONSEQUENCE while the
                // real decline sat one level down in the fn body, so every
                // reconstruction of the reported shape compiled and the reader
                // burned the search on the export machinery — the same
                // mis-attribution class as #904, reported as #906. `main` has
                // carried its inner reason since #812; an export now does too.
                // Like the main wrapper: the nesting adds context, the inner
                // wall's SPAN survives (#931).
                return Err(match fn_walls.get(n) {
                    Some(inner) => LowerError::shaped(
                        inner.span(),
                        inner.shape(),
                        format!(
                            "exported `pub fn {n}` is outside the MIR-lowering subset \
                             (the wasm module must carry its export): {inner}"
                        ),
                    ),
                    None => LowerError::Unsupported(format!(
                        "exported `pub fn {n}` is outside the MIR-lowering subset (the wasm \
                         module must carry its export; no per-function wall was recorded — \
                         it was dropped before lowering)"
                    )),
                });
            }
        }
    }

    // #826 ②: rewrite `CallFn math.sqrt`-style single-prim wrapper calls to the
    // prim itself — kills the per-call overhead AND stops the call boundary from
    // poisoning the f64-local classification of the surrounding arithmetic. Runs
    // after the runtime link (the wrappers must exist as MirFunctions); on the
    // native leg the map is empty (no self-host link), so it is wasm-only in effect.
    crate::scalar_call_inline::inline_scalar_prim_wrappers(&mut functions);

    // #824: drop MakeUnique guards that are provably dead (the value they'd guard
    // is never aliased anywhere in its own function) — see alias_safety.rs's doc
    // comment for the soundness argument. Target-agnostic (applies before either
    // renderer runs), so the native leg gets the identical benefit below too.
    crate::alias_safety::elide_unaliased_make_unique(&mut functions);

    // Any UNLINKED stdlib/runtime call would render a dangling `(call $name)` (invalid wasm) — the
    // renderer rejects it cleanly. Returns the WAT on success.
    try_render_wasm_program(&MirProgram { functions, exports, mutable_global_count })
        .map_err(|e| attribute_unlinked_calls(e, &fn_walls))
}

/// Turn the unlinked-call gate's name list into the reasons those names are
/// missing.
///
/// The gate can only report that a definition is absent. When the callee is a
/// module sibling that WALLED during lowering, its own reason was recorded and
/// is the actual diagnosis — without it the message names a mangled symbol
/// (`almide_rt_lib_write_many`) and says to "add the callee to the self-host
/// registry", which is advice for a stdlib gap and actively misleading for a
/// user module that is sitting right there in the package (#943).
fn attribute_unlinked_calls(
    e: LowerError,
    fn_walls: &std::collections::HashMap<String, crate::lower::LowerError>,
) -> LowerError {
    let LowerError::Unsupported(msg) = &e else { return e };
    let Some(rest) = msg.strip_prefix("unlinked stdlib/runtime call(s) with no wasm definition: ")
    else {
        return e;
    };
    let names: Vec<&str> = rest.split(" — ").next().unwrap_or("").split(", ").collect();
    let attributed: Vec<String> = names
        .iter()
        .filter_map(|n| fn_walls.get(*n).map(|r| format!("`{n}` walled while lowering: {r}")))
        .collect();
    if attributed.is_empty() {
        return e;
    }
    LowerError::Unsupported(format!(
        "unlinked call(s) with no wasm definition — the callee is present in the package but \
         did not survive lowering: {}",
        attributed.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // `Parser::parse()` is a recovery parser: an unparseable top-level bare
    // statement (never valid Almide grammar — only fn/effect fn/type/let/
    // var/trait/impl/test are top-level declarations) is dropped via
    // `skip_to_next_decl()`, but `parse()` still returns `Ok` as long as some
    // decl survived. Before this fix, `source_to_ir_with` only checked the
    // `Result` and never inspected `Parser::errors`, so a source like this
    // would silently compile with the bare `println` call missing from the
    /// #946: the RECEIVER of an in-place mutator over a mutable global borrows the
    /// post-COW handle — it must NOT `Dup` the slot's handle, because that owned
    /// reference (released only at scope end) is what made every SECOND write in a
    /// body see rc == 2 at its COW and full-copy the buffer (O(|dst|) per write —
    /// 23 s for nendo's VRM loop). Pinned structurally: the rendered `$main` of a
    /// two-write body contains NO `$rc_inc` call at all (the writes' args are
    /// scalars; the only rc_inc candidate was the receiver Dup).
    #[test]
    fn an_inplace_mutator_receiver_does_not_dup_the_global_slot() {
        let source = r#"
var g = bytes.new(8)
fn main() -> Unit = {
  bytes.set_at(g, 0, 1)
  bytes.set_at(g, 1, 2)
  println(int.to_string(bytes.get_or(g, 1, 0 - 1)))
}
"#;
        let wat = try_render_wasm_source(source, &[], false).expect("two-write body renders");
        let main_start = wat.find("(func $main").expect("main present");
        let main_end = wat[main_start + 1..].find("\n  (func ").map(|i| main_start + 1 + i).unwrap_or(wat.len());
        let main_body = &wat[main_start..main_end];
        // `bytes.get_or` READS the global — that path legitimately Dups, and its
        // argument ops (the Dup included) precede its call text. Assert on the two
        // writes' region only: everything up to the LAST `$bytes.set_at` call.
        let writes_end = main_body.rfind("(call $bytes.set_at").unwrap_or(main_body.len());
        let writes_region = &main_body[..writes_end];
        assert!(
            !writes_region.contains("(call $rc_inc"),
            "the in-place writes must not Dup the slot handle — an owned receiver \
             reference makes the next write's MakeUnique copy the whole buffer:\n{writes_region}"
        );
    }

    /// #939: a heap-element `list.push` accumulator loop must render through the
    /// amortized `__list_append1_rc`, not the full-copy `__list_concat_rc` — the
    /// copy is O(len) per element, O(n²) for the loop, and it took json.parse
    /// from 5 ms to minutes on a 103 KiB document. Pinned STRUCTURALLY (the
    /// rendered module names its callees) rather than by wall-clock, in both
    /// places the window has to fire: a user-written loop, and the REGISTRY-
    /// LINKED stdlib body (`__jp_array` — linked after the user-function pass
    /// ran, which is exactly how the parser's own loop was missed).
    #[test]
    fn a_heap_element_push_loop_renders_the_amortized_append() {
        let user_loop = r#"
fn build(n: Int) -> Int = {
  var acc: List[String] = []
  var i = 0
  while i < n {
    list.push(acc, int.to_string(i))
    i = i + 1
  }
  list.len(acc)
}
fn main() -> Unit = println(int.to_string(build(5)))
"#;
        let wat = try_render_wasm_source(user_loop, &[], false)
            .expect("the heap-element push loop renders");
        assert!(
            wat.contains("__list_append1_rc"),
            "a user heap-element push loop must call the amortized append"
        );

        // The STRING accumulator (#910): `acc = acc + "x"` must render through
        // `__str_append1` — the whole-copy concat peaked at 1.27 GB over 100k
        // appends and 400k died, while the amortized form runs flat at ~20 MB
        // over 4M.
        let string_loop = r#"
fn main() -> Unit = {
  var acc = ""
  var i = 0
  while i < 5 {
    acc = acc + "x"
    i = i + 1
  }
  println(int.to_string(string.len(acc)))
}
"#;
        let wat = try_render_wasm_source(string_loop, &[], false)
            .expect("the string accumulator loop renders");
        assert!(
            wat.contains("__str_append1"),
            "a string self-append loop must call the amortized append"
        );

        // #1229: the TCO'd MULTI-concat accumulator (`acc + c0 + c1` — a
        // left-spine chain through a fresh intermediate) must ALSO reach the
        // amortized append. `match_str_window` alone never fired on it (the
        // drop/rebind target is the chain HEAD's left operand, not the last
        // call's), so both links stayed whole-copy `__str_concat`s: O(n²)
        // bytes, which the 0.57.0 release-gate fuzz read as a hang at
        // n = 65535 (21.7 s wasm vs instant native, run 31486558420). Two
        // appends per iteration means TWO `$__str_append1` call sites in the
        // loop body — pinned structurally, like the rest of this family.
        let multi_concat_tco = r#"
local fn build(n: Int, pos: Int, acc: String) -> String = if pos >= n then acc
else {
  let c0 = "a"
  let c1 = "b"
  build(n, pos + 1, acc + c0 + c1)
}
fn main() -> Unit = println(build(3, 0, ""))
"#;
        let wat = try_render_wasm_source(multi_concat_tco, &[], false)
            .expect("the TCO multi-concat accumulator renders");
        assert!(
            wat.matches("(call $__str_append1").count() >= 2,
            "a TCO'd multi-concat string accumulator must chain through the \
             amortized append (both links), not the whole-copy concat:\n{wat}"
        );

        let parser_loop = r#"
import json
fn main() -> Unit = {
  match json.parse("[1, 2, 3]") {
    ok(v) => println(int.to_string(list.len(value.as_array(v) ?? [])))
    err(e) => println(e)
  }
}
"#;
        let wat = try_render_wasm_source(parser_loop, &[], false)
            .expect("the json.parse program renders");
        assert!(
            wat.contains("__list_append1_rc"),
            "the registry-linked json parser's accumulator loops must be rewritten too — \
             the fixpoint linker runs after the user-function pass, so the rewrite has to \
             ride the linker batch"
        );
    }

    // output — a wall was expected instead.
    #[test]
    fn dropped_top_level_statement_walls_instead_of_silently_compiling() {
        let source = r#"
let x = 1
let y = 2
println("top-level-statement-should-not-be-silently-dropped")
fn main() -> Unit = {
  println("from-main")
}
"#;
        let result = try_render_wasm_source(source, &[], false);
        assert!(
            result.is_err(),
            "a source with a dropped top-level statement must wall, not silently compile"
        );
    }

    #[test]
    fn a_module_siblings_parameter_is_not_the_consumers_top_let() {
        // #943: `cow_inplace_receiver` consulted `self.globals` BEFORE resolving the
        // receiver locally, inverting the precedence `value_or_global` documents (a
        // function-local — parameter included — is in `value_of`; only a MISS can be
        // a global). The two are not in one numbering space: a module sibling lowers
        // against whichever globals map its region resolves to while its own VarIds
        // are numbered independently, so `VarId(0)` was this fn's first PARAMETER and
        // the consumer's first top-let at once. The parameter was reported as an
        // "IMMUTABLE module-level `let`", the sibling was dropped, and — because a
        // dropped sibling is not fatal — the only thing the user saw was the caller's
        // "unlinked call ... no wasm definition". ONE unrelated `let` in the consumer
        // was the whole trigger; without it the same program built.
        let lib = r#"
pub fn write_many(m: Bytes, o: Int, v: Float) -> Unit = {
  var i = 0
  while i < 16 {
    bytes.set_f32_le(m, o + i * 4, v + int.to_float(i))
    i = i + 1
  }
}
"#;
        let consumer = r#"
import self.lib as lib

let UNRELATED = 1

var g = bytes.new(256)

@export(wasm, "u")
fn u(v: Float) -> Unit = lib.write_many(g, 0, v)
"#;
        let tokens = almide_lang::lexer::Lexer::tokenize(lib);
        let prog = almide_lang::parser::Parser::new(tokens).parse().expect("sibling parses");
        let mods = vec![("lib".to_string(), prog, false)];
        let out = try_render_wasm_source_library(consumer, &mods, false);
        assert!(
            out.is_ok(),
            "a sibling writing through its own Bytes PARAMETER must link even when the \
             consumer has a top-let occupying the same VarId: {out:?}"
        );
    }

    #[test]
    fn a_real_immutable_top_let_receiver_still_walls() {
        // The other side of the precedence fix: resolving locally first must not
        // disarm the #906 diagnostic. A genuine module-level `let` buffer written in
        // place has no storage slot, so the write would vanish — it must still decline,
        // and still name the fix.
        let source = r#"
let g = bytes.new(64)

@export(wasm, "u")
fn u(v: Float) -> Unit = bytes.set_f32_le(g, 0, v)
"#;
        match try_render_wasm_source_library(source, &[], false) {
            Err(LowerError::Unsupported(r)) => assert!(
                r.contains("IMMUTABLE module-level") && r.contains("Declare the buffer `var`"),
                "the immutable-top-let decline must survive and keep naming the fix, got: {r}"
            ),
            other => panic!("expected the immutable-top-let wall, got {other:?}"),
        }
    }

    #[test]
    fn a_scalar_if_arm_admits_a_global_write_but_the_linearization_still_walls() {
        // C-188 / #907: a SCALAR `if` arm runs under real IfThen/Else/EndIf markers —
        // exactly one arm executes — so an in-place mutable-global write inside the
        // ordinary early-return guard shape is a real conditional effect and must
        // lower. `lower_scalar_arm` raised only `in_frame`, so C-187's modeled-frame
        // fence misfired, the scalar-if rolled back, and the whole fn fell to the
        // both-arms linearization wall naming the CONDITION as unresolvable (nendo's
        // load_vrm_data — the cond had lowered fine; the arm's fence was the
        // decliner). Needs the full pipeline: the mutable-global storage slots are
        // assigned here, not in the render_wasm test feeder.
        let guard = r#"
var g = bytes.new(4)
fn f(data: Bytes) -> Int = {
  if bytes.len(data) < 12 then 0
  else {
    bytes.set_i32_le(g, 0, 7)
    1
  }
}
fn main() -> Unit = {
  println(int.to_string(f(bytes.new(16))))
  println(int.to_string(bytes.read_i32_le(g, 0)))
}
"#;
        let ok = try_render_wasm_source(guard, &[], false);
        assert!(
            ok.is_ok(),
            "the guard-shaped global write must lower (real markers, one arm runs): {:?}",
            ok.err()
        );
        // The OTHER direction — a global write inside a genuinely LINEARIZED (both
        // arms run) frame must keep walling — holds structurally rather than by a
        // source-level pin: `lower_branch_arm` raises only `in_frame`, never
        // `unit_arm_depth`, so any write reaching it still trips the fence. A source
        // pin was attempted and abandoned: every natural call-free-armed shape
        // (closure-param cond, `?? 0` MapAccess cond, effect-fn cond, nested-list
        // eq cond) now real-branches and correctly ADMITS the write — the
        // linearization is only reachable through conds with no executable lowering,
        // which the fuzz corpus exercises (its call-bearing-arm walls), not stable
        // hand-written source.
    }
}
