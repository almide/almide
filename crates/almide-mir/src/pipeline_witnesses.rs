// The certificate PRODUCER for `almide verify` (#2152) — include!-spliced into
// `pipeline.rs` after pipeline_native_rungs.rs, sharing its imports.
//
// This is the untrusted side of the seam: it lowers a program through the same
// MIR the v1 pipeline certifies and projects each function to the witness
// strings `crate::certificate` emits. It judges nothing. The judge is the
// independent `almide-verify` binary, which reads these strings and shares no
// code with this crate.

/// One lowered function's per-function witnesses.
pub struct FunctionWitnesses {
    /// The MIR function name (lifted lambdas appear under their own names).
    pub name: String,
    /// Ownership certificate (format v5) — `proofs/OwnershipChecker.v`.
    pub ownership: String,
    /// `<defined>|<used>` — `proofs/NameTotality.v`.
    pub names: String,
    /// `<declared>|<used>` capabilities — `proofs/CapabilityBound.v`.
    pub caps: String,
}

/// Every witness the producer emits for one program.
pub struct ProgramWitnesses {
    /// Per-function witnesses, in lowering order.
    pub functions: Vec<FunctionWitnesses>,
    /// The whole-program call-mode witness over every lowered function
    /// (`proofs/CallModes.v`); `None` when nothing lowered.
    pub call_modes: Option<String>,
    /// Functions outside the lowering subset, each with its wall. They are
    /// NOT certified, and the bundle says so by name — never a silent skip.
    pub walled: Vec<(String, String)>,
}

/// The program's top-level `let` globals (declared types and initializers),
/// entry file and modules alike — the maps every lowering entry is threaded
/// with (the emit_cert_from_source example's `collect_top_level_globals`).
#[allow(clippy::type_complexity)]
fn witness_globals(
    ir: &almide_ir::IrProgram,
) -> (HashMap<almide_ir::VarId, almide_lang::types::Ty>, HashMap<almide_ir::VarId, almide_ir::IrExpr>) {
    let mut types = HashMap::new();
    let mut inits = HashMap::new();
    let module_lets = ir.modules.iter().flat_map(|m| m.top_lets.iter());
    for tl in ir.top_lets.iter().chain(module_lets) {
        types.insert(tl.var, tl.ty.clone());
        inits.insert(tl.var, tl.value.clone());
    }
    (types, inits)
}

/// Record and variant layouts over the entry file's AND every module's type
/// declarations, so a cross-module type lowers instead of walling.
fn witness_layouts(ir: &almide_ir::IrProgram) -> (crate::lower::RecordLayouts, crate::lower::VariantLayouts) {
    let mut records = crate::lower::build_record_layouts(&ir.type_decls);
    let mut variants = crate::lower::build_variant_layouts(&ir.type_decls);
    for m in &ir.modules {
        records.extend(crate::lower::build_record_layouts(&m.type_decls));
        let vl = crate::lower::build_variant_layouts(&m.type_decls);
        variants.by_type.extend(vl.by_type);
        variants.ctor_to_type.extend(vl.ctor_to_type);
        variants.ctor_field_defaults.extend(vl.ctor_field_defaults);
    }
    (records, variants)
}

/// Lower `source` (with its resolved cross-module siblings) and emit every
/// witness the untrusted producer can: ownership, names and caps per lowered
/// function, plus the whole-program call-mode witness. `manifest_allow` is an
/// `almide.toml [permissions].allow` list; when given, each effect fn's
/// declared capability bound becomes that manifest (a pure fn keeps ∅), which
/// is what makes the caps witness able to reject.
///
/// `Err` is a whole-program wall (a parse or type error): there is nothing to
/// certify. A wall on ONE function lands in `walled` instead.
pub fn program_witnesses(
    source: &str,
    self_modules: &[(String, almide_lang::ast::Program, bool)],
    manifest_allow: Option<&[String]>,
) -> Result<ProgramWitnesses, LowerError> {
    let _strict = crate::lower::StrictValuesGuard::set(true);
    let ir = source_to_ir_with(source, self_modules)?;
    let (globals, global_inits) = witness_globals(&ir);
    let (records, variants) = witness_layouts(&ir);
    let mut lowered: Vec<crate::MirFunction> = Vec::new();
    let mut walled = Vec::new();
    for func in &ir.functions {
        match crate::lower::lower_function_all_with_globals(func, &globals, &global_inits, &records, &variants) {
            Ok(all) => lowered.extend(all),
            Err(e) => walled.push((func.name.to_string(), e.to_string())),
        }
    }
    if let Some(allow) = manifest_allow {
        lowered.iter_mut().for_each(|f| crate::certificate::apply_manifest_caps(f, allow));
    }
    let functions = lowered
        .iter()
        .map(|f| FunctionWitnesses {
            name: f.name.clone(),
            ownership: crate::certificate::ownership_certificate(f),
            names: crate::certificate::name_witness_string(f),
            caps: crate::certificate::cap_witness_string(f),
        })
        .collect();
    // The call-mode witness indexes functions by name. Dotted callees are
    // purity-gated self-hosted stdlib calls and `__`-prefixed out-of-program
    // names are render-linked runtime helpers: both borrow their heap args by
    // the renderer contract (the emit_cert_from_source policy). Any other
    // unknown callee gets the out-of-range index the checker rejects.
    let program: std::collections::BTreeMap<String, crate::MirFunction> =
        lowered.into_iter().map(|f| (f.name.clone(), f)).collect();
    let call_modes = (!program.is_empty()).then(|| {
        crate::certificate::call_modes_witness(&program, &|n: &str| n.contains('.') || n.starts_with("__"))
    });
    Ok(ProgramWitnesses { functions, call_modes, walled })
}
