// Included by render_native_rec.rs: actual Rust Default support, recursively.
pub(crate) fn has_default(t: NTy) -> bool {
    fn check(t: NTy, decls: &[RecDecl], visiting: &mut Vec<u16>) -> bool {
        match t {
            NTy::Res | NTy::ResStr => false,
            NTy::Rec(i) | NTy::RecRef(i) => {
                let Some(d) = decls.get(i as usize) else {
                    return false;
                };
                if d.result_of.is_some() {
                    return false;
                }
                // Empty containers do not need their elements to be Default.
                if d.cell_of.is_some() || d.alias.is_some()
                    || d.list_of.is_some()
                    || d.option_of.is_some()
                    || d.map_of.is_some()
                    || d.set_of.is_some()
                {
                    return true;
                }
                if visiting.contains(&i) {
                    return false;
                }
                visiting.push(i);
                let fields = match &d.cases {
                    Some(cases) => d
                        .default_tag
                        .and_then(|tag| cases.iter().find(|(t, _)| *t == tag))
                        .map(|(_, fs)| fs.as_slice()),
                    None => Some(d.fields.as_slice()),
                };
                let result = fields.is_some_and(|fs| fs.iter().all(|t| check(*t, decls, visiting)));
                visiting.pop();
                result
            }
            _ => true,
        }
    }
    RECS.with(|r| check(t, &r.borrow(), &mut Vec::new()))
}
