// The affine-shape census entry (#4236) — include!-spliced into `pipeline.rs`
// after pipeline_witnesses.rs, sharing its imports and `witness_lowered`.
//
// Kept out of pipeline_witnesses.rs on purpose: that file is in the safety
// coverage set (proofs/coverage-safety-baseline.txt) because the `almide
// verify` witness sweep reaches it; this entry is a measurement, reached by
// the census and its tests, not by the witness producer.

/// Every lowered function's [`crate::affine_shape::AffineShape`] (#4236), in
/// lowering order, plus the walled functions — which are NOT classified, and
/// are named so a census never counts them silently.
#[allow(clippy::type_complexity)]
pub fn program_affine_shapes(
    source: &str,
    self_modules: &[(String, almide_lang::ast::Program, bool)],
) -> Result<(Vec<(String, crate::affine_shape::AffineShape)>, Vec<(String, String)>), LowerError> {
    let (lowered, walled) = witness_lowered(source, self_modules)?;
    let shapes = lowered.iter().map(|f| (f.name.clone(), crate::affine_shape::affine_shape(f))).collect();
    Ok((shapes, walled))
}
