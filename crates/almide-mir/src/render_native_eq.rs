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
            // The runtime types' own PartialEq is the language's `==`: Value's is
            // `value_eq_ref`, a Map's and a Set's compare members independently of
            // insertion order. It holds only over members that compare exactly.
            t if crate::lower::is_value_ty(t) => true,
            Ty::Applied(C::Map | C::Set, args) => args.iter().all(|t| self.exact_eq_type(t, visiting)),
            Ty::Named(_, _) => self.exact_eq_decl(&block_key(t), visiting),
            _ => false,
        }
    }

    /// A struct or enum Rust cannot derive equality for, but whose every
    /// field has a source comparison, gets those comparisons (#4687): a Float
    /// lane compares as the float it holds, a generated field type by its own
    /// derived or generated equality. A greatest fixpoint, so a recursive type
    /// keeps its comparison; a field without one removes the type.
    fn source_equalities(&self, decls: &mut [RecDecl], names: &[String]) {
        // Each case's source field types and native field kinds, in tag order.
        type Case = (Vec<almide_lang::types::Ty>, Vec<NTy>);
        let source = |i: usize| -> Option<Vec<Case>> {
            let d = &decls[i];
            let plain = !d.eq_safe && d.alias.is_none() && d.list_of.is_none() && d.option_of.is_none() && d.result_of.is_none()
                && d.map_of.is_none() && d.set_of.is_none() && d.closure_of.is_none() && d.cell_of.is_none()
                && !REGION_NAMES.with(|r| r.borrow().contains(&names[i]));
            let (is_enum, cases) = self.decls.get(&names[i]).filter(|_| plain)?;
            let native: Vec<(u32, Vec<NTy>)> = match (&d.cases, *is_enum) {
                (Some(cs), true) => cs.clone(),
                (None, false) if cases.len() == 1 => vec![(cases[0].0, d.fields.clone())],
                _ => return None,
            };
            (native.len() == cases.len()).then_some(())?;
            cases.iter().zip(native).map(|((tag, ts), (t2, ks))| (*tag == t2 && ts.len() == ks.len()).then(|| (ts.clone(), ks))).collect()
        };
        let mut kept: Vec<bool> = (0..decls.len()).map(|i| source(i).is_some()).collect();
        let field_eqs = |i: usize, kept: &[bool]| -> Option<Vec<Vec<FieldEq>>> {
            source(i)?.iter().map(|(ts, ks)| ts.iter().zip(ks).map(|(t, k)| self.field_eq(t, *k, decls, kept)).collect()).collect()
        };
        loop {
            let drop: Vec<usize> = (0..decls.len()).filter(|&i| kept[i] && field_eqs(i, &kept).is_none()).collect();
            if drop.is_empty() { break; }
            for i in drop { kept[i] = false; }
        }
        let eqs: Vec<Option<Vec<Vec<FieldEq>>>> = (0..decls.len()).map(|i| if kept[i] { field_eqs(i, &kept) } else { None }).collect();
        for (d, e) in decls.iter_mut().zip(eqs) { d.eq_fields = e; }
    }

    fn field_eq(&self, t: &almide_lang::types::Ty, k: NTy, decls: &[RecDecl], kept: &[bool]) -> Option<FieldEq> {
        use almide_lang::types::{constructor::TypeConstructorId as C, Ty};
        let float_list = |w: fn(&Ty) -> bool| matches!(t, Ty::Applied(C::List, a) if a.len() == 1 && w(&a[0]));
        let wide = |t: &Ty| matches!(t, Ty::Float | Ty::Float64);
        let narrow = |t: &Ty| matches!(t, Ty::Float32);
        if self.exact_eq_type(t, &mut Default::default()) { return Some(FieldEq::Exact); }
        match k {
            NTy::F64 if wide(t) => Some(FieldEq::Exact),
            NTy::I64 if wide(t) => Some(FieldEq::F64Bits),
            NTy::I64 if narrow(t) => Some(FieldEq::F32Bits),
            NTy::Vec if float_list(wide) => Some(FieldEq::VecF64Bits),
            NTy::Vec if float_list(narrow) => Some(FieldEq::VecF32Bits),
            NTy::Rec(j) => compares(j as usize, decls, kept, 0).then_some(FieldEq::Exact),
            _ => None,
        }
    }
}

/// How one field of a generated struct or enum case compares by its source type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldEq { Exact, F64Bits, F32Bits, VecF64Bits, VecF32Bits }

/// Generated type `j`'s Rust `==` is its language equality: derived, generated
/// by [`Derive::source_equalities`], or a list/option of such elements.
fn compares(j: usize, decls: &[RecDecl], kept: &[bool], depth: u8) -> bool {
    let Some(d) = decls.get(j) else { return false };
    d.eq_safe || kept[j] || (depth < 8 && match d.list_of.or(d.option_of) {
        Some(NTy::Rec(m)) => compares(m as usize, decls, kept, depth + 1),
        _ => false,
    })
}

/// One field's comparison of `a` with `b`; `d` dereferences a bound field
/// (an enum case's bindings are references, a struct's fields are places).
fn field_eq_expr(f: FieldEq, a: &str, b: &str, d: &str) -> String {
    let bits = |w: &str, u: &str, x: &str, y: &str| format!("{w}::from_bits({x} as {u}) == {w}::from_bits({y} as {u})");
    match f {
        FieldEq::Exact => format!("{a} == {b}"),
        FieldEq::F64Bits => bits("f64", "u64", &format!("{d}{a}"), &format!("{d}{b}")),
        FieldEq::F32Bits => bits("f32", "u32", &format!("{d}{a}"), &format!("{d}{b}")),
        FieldEq::VecF64Bits | FieldEq::VecF32Bits => {
            let cmp = if f == FieldEq::VecF64Bits { bits("f64", "u64", "*x", "*y") } else { bits("f32", "u32", "*x", "*y") };
            format!("{a}.len() == {b}.len() && {a}.iter().zip({b}.iter()).all(|(x, y)| {cmp})")
        }
    }
}

/// The `impl PartialEq` of a generated type with [`RecDecl::eq_fields`]: a
/// struct compares its fields, an enum its tags and then the case's fields.
fn source_eq_impl(i: u16, d: &RecDecl) -> String {
    let Some(eqs) = &d.eq_fields else { return String::new() };
    let all = |fs: &[FieldEq], a: &dyn Fn(usize) -> String, b: &dyn Fn(usize) -> String, deref: &str| -> String {
        let parts: Vec<String> = fs.iter().enumerate().map(|(k, f)| field_eq_expr(*f, &a(k), &b(k), deref)).collect();
        if parts.is_empty() { "true".to_string() } else { parts.join("\n            && ") }
    };
    let n = struct_name(i);
    let body = match &d.cases {
        None => all(&eqs[0], &|k| format!("self.f{k}"), &|k| format!("o.f{k}"), ""),
        Some(cases) => {
            let arms: Vec<String> = cases.iter().zip(eqs).map(|((t, fs), e)| {
                let bind = |p: &str| if fs.is_empty() { String::new() } else { format!("({})", (0..fs.len()).map(|k| format!("{p}{k}")).collect::<Vec<_>>().join(", ")) };
                format!("({n}::C{t}{}, {n}::C{t}{}) => {},", bind("a"), bind("b"), all(e, &|k| format!("a{k}"), &|k| format!("b{k}"), "*"))
            }).collect();
            format!("match (self, o) {{\n            {}\n            _ => false,\n        }}", arms.join("\n            "))
        }
    };
    format!("impl PartialEq for {n} {{\n    fn eq(&self, o: &Self) -> bool {{\n        {body}\n    }}\n}}\n\n")
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
            r.borrow().iter().any(|d| d.name == block_key(t) && (d.eq_safe || d.eq_fields.is_some()))
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
            let i = rec_index(&block_key(&ty)).expect("the block's record is registered");
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

    /// #4687: a comparison over a struct holding a Value, a Map or a Float has
    /// the language's equality — derived where Rust's agrees, generated per
    /// field where a Float rides its bit lane — or none at all.
    #[test]
    fn value_map_and_float_fields_get_their_source_equality() {
        use almide_lang::capability_bounds::Cap;
        let layouts = crate::lower::RecordLayouts::from([
            ("Reading".into(), (vec![], vec![(sym("s"), Ty::String), (sym("f"), Ty::Float)])),
        ]);
        let value = Ty::Named(sym("Value"), Default::default());
        let tuple = |ts: Vec<Ty>| Ty::Tuple(ts.into());
        let reading = Ty::Named(sym("Reading"), Default::default());
        let with_value = tuple(vec![Ty::String, value.clone()]);
        let with_map = tuple(vec![Ty::String, Ty::Applied(C::Map, vec![Ty::String, Ty::Int].into())]);
        let with_float = tuple(vec![Ty::String, Ty::Float]);
        let floats = tuple(vec![Ty::String, Ty::list(Ty::Float)]);
        let nested = tuple(vec![reading.clone(), Ty::Int]);
        let float_map = tuple(vec![Ty::String, Ty::Applied(C::Map, vec![Ty::String, Ty::Float].into())]);
        let mut roots = vec![value];
        for t in [&with_value, &with_map, &with_float, &floats, &nested, &float_map, &reading] {
            collect_tuples(&Ty::list(t.clone()), &mut roots);
        }
        let _guard = install(derive_records(&layouts, &Default::default(), &roots));
        let decl = |t: &Ty| RECS.with(|r| r.borrow()[rec_index(&block_key(t)).expect("registered") as usize].clone());
        for t in [&with_value, &with_map] {
            assert!(decl(t).eq_safe, "{t:?}");
        }
        use FieldEq::*;
        for (t, eqs) in [(&with_float, vec![Exact, F64Bits]), (&floats, vec![Exact, VecF64Bits]), (&reading, vec![Exact, F64Bits]), (&nested, vec![Exact, Exact])] {
            let d = decl(t);
            assert!(!d.eq_safe, "{t:?}");
            assert_eq!(d.eq_fields, Some(vec![eqs]), "{t:?}");
        }
        for t in [&with_value, &with_map, &with_float, &floats, &nested, &reading] {
            assert!(source_comparison_agrees(t, Cap::Eq, false), "{t:?}");
        }
        assert!(!source_comparison_agrees(&with_float, Cap::Ord, false));
        let d = decl(&float_map);
        assert!(!d.eq_safe && d.eq_fields.is_none());
        assert!(!source_comparison_agrees(&float_map, Cap::Eq, false));
        let rendered = struct_defs();
        let i = rec_index(&block_key(&with_float)).unwrap();
        assert!(rendered.contains(&format!("impl PartialEq for {}", struct_name(i))), "{rendered}");
        assert!(rendered.contains("f64::from_bits(self.f1 as u64) == f64::from_bits(o.f1 as u64)"), "{rendered}");
    }
}
