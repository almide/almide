// Structural Rust equality is available only where the declared language
// types establish the same relation. In particular an i64 lane may contain
// float bits or an address: its machine spelling is not that evidence.
impl Derive<'_> {
    fn exact_eq_decl(&self, n: &str, visiting: &mut std::collections::BTreeSet<String>) -> bool {
        if REGION_NAMES.with(|r| r.borrow().contains(n)) { return false; }
        let Some((_, cases)) = self.decls.get(n) else { return false };
        // Recursive ordinary enums compare their boxed values structurally.
        // This is a greatest fixpoint over the source fields; any unsupported
        // leaf still rejects the entire connected comparison.
        if !visiting.insert(n.to_string()) { return true; }
        let ok = cases.iter().all(|(_, fs)| fs.iter().all(|t| self.exact_eq_type(t, visiting)));
        visiting.remove(n);
        ok
    }

    fn exact_eq_type(&self, t: &almide_lang::types::Ty, visiting: &mut std::collections::BTreeSet<String>) -> bool {
        use almide_lang::types::{constructor::TypeConstructorId as C, Ty};
        match t {
            Ty::Float | Ty::Float32 | Ty::Float64 | Ty::RawPtr | Ty::Fn { .. } => false,
            Ty::Unit | Ty::String | Ty::Bytes => true,
            t if is_lane_scalar(t) => true,
            Ty::Tuple(es) => es.iter().all(|t| self.exact_eq_type(t, visiting)),
            Ty::Record { fields } => fields.iter().all(|(_, t)| self.exact_eq_type(t, visiting)),
            Ty::Applied(C::List | C::Option | C::Result, args) => args.iter().all(|t| self.exact_eq_type(t, visiting)),
            Ty::Named(_, _) => self.exact_eq_decl(&block_key(t), visiting),
            // Map/Set/Value equality has its own runtime contract. Do not
            // infer it from a container's Rust representation here.
            _ => false,
        }
    }
}

/// A generic comparison's actual source type. Float is native f64 only at
/// the scalar boundary; nested Float/Float32 still uses integer bit lanes.
pub(crate) fn source_comparison_agrees(t: &almide_lang::types::Ty, cap: almide_lang::capability_bounds::Cap, scalar: bool) -> bool {
    use almide_lang::{capability_bounds::Cap, types::{constructor::TypeConstructorId as C, Ty}};
    match t {
        Ty::Float | Ty::Float64 => scalar && cap == Cap::Eq,
        Ty::Float32 | Ty::RawPtr | Ty::Fn { .. } => false,
        Ty::Unit | Ty::String | Ty::Bytes | Ty::Int | Ty::Bool => true,
        t if is_lane_scalar(t) => cap == Cap::Eq,
        Ty::Applied(C::List | C::Option | C::Result, args) => args.iter().all(|t| source_comparison_agrees(t, cap, false)),
        Ty::Tuple(_) | Ty::Record { .. } | Ty::Named(_, _) => cap == Cap::Eq && RECS.with(|r| {
            r.borrow().iter().any(|d| d.name == block_key(t) && d.eq_safe)
        }),
        _ => false,
    }
}

#[cfg(test)]
mod equality_tests {
    use super::*;
    use almide_lang::{intern::sym, types::{constructor::TypeConstructorId as C, Ty}};

    #[test]
    fn equality_admission_uses_source_fields_not_erased_lanes() {
        let layouts = crate::lower::RecordLayouts::from([
            ("Good".into(), (vec![], vec![(sym("s"), Ty::String), (sym("n"), Ty::Int)])),
            ("Bad".into(), (vec![], vec![(sym("s"), Ty::String), (sym("f"), Ty::Float)])),
            ("SmallFloat".into(), (vec![], vec![(sym("s"), Ty::String), (sym("f"), Ty::Float32)])),
            ("Deep".into(), (vec![], vec![(sym("xs"), Ty::Applied(C::List, vec![Ty::Named(sym("Bad"), Default::default())].into()))])),
        ]);
        let variants = crate::lower::VariantLayouts::default();
        let ts = vec![Ty::Applied(C::List, vec![Ty::Named(sym("Bad"), Default::default())].into())];
        let records = derive_records(&layouts, &variants, &ts);
        assert!(records.iter().find(|d| d.name == "Good").unwrap().eq_safe);
        for n in ["Bad", "Deep", "SmallFloat"] {
            assert!(!records.iter().find(|d| d.name == n).unwrap().eq_safe, "{n}");
        }
    }
}
