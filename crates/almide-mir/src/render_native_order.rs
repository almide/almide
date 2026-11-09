// Source-declared order for native aggregate runtime calls. A machine lane
// alone is never evidence of Float or unsigned order.
thread_local! {
    static ORDER_NAMES: RefCell<std::collections::BTreeSet<String>> = const { RefCell::new(std::collections::BTreeSet::new()) };
}
pub(crate) struct OrderGuard(std::collections::BTreeSet<String>);
impl Drop for OrderGuard {
    fn drop(&mut self) { ORDER_NAMES.with(|n| *n.borrow_mut() = std::mem::take(&mut self.0)); }
}
pub(crate) fn install_declared_order(types: &[almide_ir::IrTypeDecl]) -> OrderGuard {
    let names = types.iter().filter(|t| t.deriving.as_ref().is_some_and(|ds| ds.iter().any(|d| d.as_str() == "Ord")))
        .map(|t| t.name.to_string()).collect();
    OrderGuard(ORDER_NAMES.with(|n| std::mem::replace(&mut *n.borrow_mut(), names)))
}
impl Derive<'_> {
    fn exact_order_decl(&self, n: &str, visiting: &mut std::collections::BTreeSet<String>) -> bool {
        if REGION_NAMES.with(|r| r.borrow().contains(n)) { return false; }
        if !n.starts_with("Tuple(") && !self.order_names.contains(n) { return false; }
        let Some((_, cases)) = self.decls.get(n) else { return false };
        if !visiting.insert(n.to_string()) { return true; }
        let yes = cases.iter().all(|(_, fs)| fs.iter().all(|t| self.exact_order_type(t, visiting)));
        visiting.remove(n);
        yes
    }
    /// This order matches the native runtime without interpreting erased lanes.
    /// Unsigned/narrow integer and floating orders need their own adapters.
    fn exact_order_type(&self, t: &almide_lang::types::Ty, visiting: &mut std::collections::BTreeSet<String>) -> bool {
        use almide_lang::types::{constructor::TypeConstructorId as C, Ty};
        match t {
            Ty::Int | Ty::Bool | Ty::String | Ty::Bytes | Ty::Unit => true,
            Ty::Tuple(es) => es.iter().all(|t| self.exact_order_type(t, visiting)),
            Ty::Applied(C::List | C::Option | C::Result, es) => es.iter().all(|t| self.exact_order_type(t, visiting)),
            Ty::Named(_, _) => self.exact_order_decl(&block_key(t), visiting),
            _ => false,
        }
    }

}

/// Check the generated representation as well as the source order witness.
pub(crate) fn native_order_safe(i: u16) -> bool {
    fn kind(t: NTy, ds: &[RecDecl], visiting: &mut Vec<u16>) -> bool {
        match t {
            NTy::I64 | NTy::Str | NTy::Vec | NTy::VecStr | NTy::Opt | NTy::Res | NTy::ResStr => true,
            NTy::Rec(i) => {
                let Some(d) = ds.get(i as usize) else { return false };
                if visiting.contains(&i) { return true; }
                visiting.push(i);
                let yes = if d.name == BYTES_KEY { true }
                    else if let Some(e) = d.list_of.or(d.option_of) { kind(e, ds, visiting) }
                    else if let Some((a, b)) = d.result_of { kind(a, ds, visiting) && kind(b, ds, visiting) }
                    else { d.ord_safe && d.fields.iter().all(|t| kind(*t, ds, visiting))
                        && d.cases.iter().flatten().all(|(_, fs)| fs.iter().all(|t| kind(*t, ds, visiting))) };
                visiting.pop();
                yes
            }
            _ => false,
        }
    }
    RECS.with(|r| kind(NTy::Rec(i), &r.borrow(), &mut Vec::new()))
}


#[cfg(test)]
mod order_tests {
    use super::*;
    use almide_lang::{intern::sym, types::Ty};
    #[test]
    fn nominal_order_requires_derivation_and_source_field_order() {
        let names = ["Good", "BadFloat", "BadUnsigned"].map(String::from).into_iter().collect();
        let _order = OrderGuard(ORDER_NAMES.with(|n| std::mem::replace(&mut *n.borrow_mut(), names)));
        let layouts = crate::lower::RecordLayouts::from([
            ("Good".into(), (vec![], vec![(sym("key"), Ty::Int), (sym("name"), Ty::String)])),
            ("BadFloat".into(), (vec![], vec![(sym("key"), Ty::Float), (sym("name"), Ty::String)])),
            ("BadUnsigned".into(), (vec![], vec![(sym("key"), Ty::UInt64), (sym("name"), Ty::String)])),
            ("NoDerive".into(), (vec![], vec![(sym("key"), Ty::Int), (sym("name"), Ty::String)])),
        ]);
        let _records = install(derive_records(&layouts, &Default::default(), &[]));
        for (name, expected) in [("Good", true), ("BadFloat", false), ("BadUnsigned", false), ("NoDerive", false)] {
            let ty = Ty::Named(sym(name), Default::default());
            assert_eq!(source_comparison_agrees(&ty, almide_lang::capability_bounds::Cap::Ord, true), expected, "{name}");
            assert_eq!(native_order_safe(rec_index(name).unwrap()), expected, "{name}");
        }
    }
}
