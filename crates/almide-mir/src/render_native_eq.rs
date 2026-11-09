// Structural Rust equality is available only where the declared language
// types establish the same relation. In particular an i64 lane may contain
// float bits or an address: its machine spelling is not that evidence.
impl Derive<'_> {
    /// Aggregate Set members need the same source-field equality check as
    /// Map keys. Do this during derivation, before the table is installed.
    fn set_element_type(&self, t: &almide_lang::types::Ty) -> bool {
        use almide_lang::types::{constructor::TypeConstructorId as C, Ty};
        match t {
            Ty::Int | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64 | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 | Ty::String | Ty::Bytes => true,
            Ty::Applied(C::List | C::Set, a) if a.len() == 1 => self.set_element_type(&a[0]),
            Ty::Named(..) | Ty::Tuple(_) | Ty::Record { .. } => self.exact_eq_type(t, &mut Default::default()),
            _ => false,
        }
    }
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
        Ty::Tuple(_) | Ty::Record { .. } | Ty::Named(_, _) if cap == Cap::Ord => rec_index(&block_key(t)).is_some_and(native_order_safe),
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
    fn tuple_order_checks_source_numeric_types_before_erasing_lanes() {
        let pair = |t| Ty::Tuple(vec![t, Ty::String].into());
        let good = pair(Ty::Int);
        let negative = [pair(Ty::Float), pair(Ty::Float32), pair(Ty::UInt64)];
        let nested = pair(Ty::list(good.clone()));
        let mut roots = Vec::new();
        for t in [&good, &nested].into_iter().chain(negative.iter()) { collect_tuples(t, &mut roots); }
        let _guard = install(derive_records(&Default::default(), &Default::default(), &roots));
        for t in [good, nested] {
            assert!(source_comparison_agrees(&t, almide_lang::capability_bounds::Cap::Ord, true));
            assert!(native_order_safe(rec_index(&block_key(&t)).unwrap()));
        }
        for t in negative {
            assert!(!source_comparison_agrees(&t, almide_lang::capability_bounds::Cap::Ord, true));
            assert!(!native_order_safe(rec_index(&block_key(&t)).unwrap()));
        }
    }

    #[test]
    fn aggregate_set_members_require_source_field_equality() {
        let layouts = crate::lower::RecordLayouts::from([
            ("Good".into(), (vec![], vec![(sym("name"), Ty::String), (sym("count"), Ty::Int)])),
            ("Bad".into(), (vec![], vec![(sym("value"), Ty::Float)])),
        ]);
        let good = Ty::Named(sym("Good"), Default::default());
        let bad = Ty::Named(sym("Bad"), Default::default());
        let pair = Ty::Tuple(vec![Ty::Int, Ty::String].into());
        let mut roots = Vec::new();
        for ty in [&good, &bad, &pair] { collect_tuples(&Ty::set_of(ty.clone()), &mut roots); }
        let decls = derive_records(&layouts, &Default::default(), &roots);
        assert!(!decls.iter().any(|d| d.name == block_key(&Ty::set_of(bad.clone()))));
        let _guard = install(decls);
        for ty in [good, pair] {
            assert!(is_native_set(&Ty::set_of(ty.clone())));
            let i = rec_index(&block_key(&ty)).unwrap();
            assert!(native_set_element_kind(NTy::Rec(i)));
        }
        assert!(!is_native_set(&Ty::set_of(bad)));
    }

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
