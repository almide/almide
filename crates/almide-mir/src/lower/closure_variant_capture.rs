// The single-variant capture class of the closure env (#2739 family C2),
// include!-spliced at module level from tail_c.rs (binds.rs sits at the
// 800-line ceiling).

impl LowerCtx {
    /// The single user-VARIANT twin of [`Self::record_capture_name`]: `Some(<V>)`
    /// iff `ty` is a non-generic named user variant — a closure capturing a
    /// recursive-variant payload binder (`Pair(a, b) => some(0) |> option.map((_)
    /// => Pair(b, a))`). It rides the same RICH env class as a captured record:
    /// the env slot holds a `[tag][handle]` wrapper that CO-OWNS the variant
    /// (`Dup` + move-in), so the binder's own owner — the matched subject — keeps
    /// its reference and the closure's release never reaches past its own.
    pub(crate) fn variant_capture_name(&self, ty: &Ty) -> Option<String> {
        if !matches!(ty, Ty::Named(_, args) if args.is_empty()) {
            return None;
        }
        self.custom_variant_type_name(ty)
    }
}
