// Included by render_native_rec: the shared typed closure environment.
thread_local! {
    static CAPTURE_KINDS: RefCell<Vec<NTy>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn reset_capture_kinds() { CAPTURE_KINDS.with(|k| k.borrow_mut().clear()); }

pub(crate) fn closure_sig(i: u16) -> Option<(Vec<NTy>, Option<NTy>)> {
    RECS.with(|r| r.borrow().get(i as usize).and_then(|d| d.closure_of.clone()))
}

pub(crate) fn is_closure(t: NTy) -> bool {
    matches!(t, NTy::Rec(i) | NTy::RecRef(i) if closure_sig(i).is_some())
}

/// Resolve an already checked source type against this render's declarations.
/// This does not infer a type from a raw address or a stored integer.
pub(crate) fn checked_kind(t: &almide_lang::types::Ty) -> Option<NTy> {
    INPUTS.with(|i| {
        let i = i.borrow();
        let (layouts, variants, _) = i.as_ref()?;
        crate::pipeline::native_sig_kind(t, layouts, variants).map(crate::render_native_closure::owned_nty)
    })
}

pub(crate) fn closure_slots(name: &str) -> Option<Vec<NTy>> {
    let captures = crate::lambda_sigs::captures(name)?;
    let mut slots = vec![NTy::I64, NTy::I64];
    for cap in captures {
        if cap.cell {
            let key = if cap.rich { crate::lambda_sigs::cell_wrapper_key(&cap.ty) } else { crate::lambda_sigs::cell_key(&cap.ty) };
            slots.push(NTy::Rec(rec_index(&key)?));
            continue;
        }
        let t = cap.slot_ty()?;
        slots.push(if is_lane_scalar(&t) { NTy::I64 } else { checked_kind(&t)? });
    }
    Some(slots)
}

pub(crate) fn capture_variant(t: NTy) -> String {
    let t = match t {
        NTy::StrRef => NTy::Str, NTy::VecRef => NTy::Vec, NTy::VecStrRef => NTy::VecStr,
        NTy::RecRef(i) => NTy::Rec(i), t => t,
    };
    let i = CAPTURE_KINDS.with(|ks| {
        let mut ks = ks.borrow_mut();
        if let Some(i) = ks.iter().position(|k| crate::native_value_repr::same_owned(*k, t)) { i } else { ks.push(t); ks.len() - 1 }
    });
    format!("K{i}")
}

pub(crate) fn closure_env_defs() -> String {
    if !RECS.with(|r| r.borrow().iter().any(|d| d.closure_of.is_some())) { return String::new(); }
    let mut out = String::from("#[derive(Clone, Default)]\nstruct __AlmdClosure { words: __AlmdWords, heaps: Option<Vec<Option<__AlmdCapture>>> }\n#[derive(Clone)]\nenum __AlmdWords { Inline(u8, [i64; 4]), Heap(Vec<i64>) }\nimpl Default for __AlmdWords { fn default() -> Self { __AlmdWords::Inline(0, [0; 4]) } }\nimpl __AlmdWords { #[inline] fn from_slice(s: &[i64]) -> Self { if s.len() <= 4 { let mut a = [0i64; 4]; a[..s.len()].copy_from_slice(s); __AlmdWords::Inline(s.len() as u8, a) } else { __AlmdWords::Heap(s.to_vec()) } } fn zeroed(n: usize) -> Self { if n <= 4 { __AlmdWords::Inline(n as u8, [0; 4]) } else { __AlmdWords::Heap(vec![0; n]) } } }\nimpl std::ops::Deref for __AlmdWords { type Target = [i64]; #[inline] fn deref(&self) -> &[i64] { match self { __AlmdWords::Inline(n, a) => &a[..*n as usize], __AlmdWords::Heap(v) => v } } }\nimpl std::ops::DerefMut for __AlmdWords { #[inline] fn deref_mut(&mut self) -> &mut [i64] { match self { __AlmdWords::Inline(n, a) => &mut a[..*n as usize], __AlmdWords::Heap(v) => v } } }\nimpl __AlmdClosure { fn new(n: usize) -> Self { Self { words: __AlmdWords::zeroed(n), heaps: None } } fn put(&mut self, k: usize, v: __AlmdCapture) { let n = self.words.len(); let h = self.heaps.get_or_insert_with(Vec::new); if h.len() < n { h.resize_with(n, || None); } h[k] = Some(v); } }\ntrait __AlmdEnv { fn words(&self) -> &[i64]; fn len(&self) -> usize { self.words().len() } fn heap(&self, _: usize) -> Option<&__AlmdCapture> { None } }\nimpl __AlmdEnv for __AlmdClosure { fn words(&self) -> &[i64] { &*self.words } fn heap(&self, k: usize) -> Option<&__AlmdCapture> { self.heaps.as_ref().and_then(|h| h.get(k)).and_then(Option::as_ref) } }\nimpl<T: __AlmdEnv + ?Sized> __AlmdEnv for &T { fn words(&self) -> &[i64] { (**self).words() } fn heap(&self, k: usize) -> Option<&__AlmdCapture> { (**self).heap(k) } }\nimpl<const N: usize> __AlmdEnv for [i64; N] { fn words(&self) -> &[i64] { self } }\n#[derive(Clone)]\nenum __AlmdCapture {\n");
    CAPTURE_KINDS.with(|ks| {
        for (i, t) in ks.borrow().iter().enumerate() {
            out.push_str(&format!("    K{i}({}),\n", owned_ty(*t)));
        }
    });
    out.push_str("}\n\n");
    out
}

pub(crate) fn closure_source(i: u16) -> Option<(Vec<almide_lang::types::Ty>, almide_lang::types::Ty)> {
    let name = RECS.with(|r| r.borrow().get(i as usize).map(|d| d.name.clone()))?;
    INPUTS.with(|inputs| {
        let inputs = inputs.borrow();
        let (_, _, ts) = inputs.as_ref()?;
        ts.iter().find_map(|t| match t {
            almide_lang::types::Ty::Fn { params, ret, .. } if tuple_key(t) == name => Some((params.clone(), (**ret).clone())),
            _ => None,
        })
    })
}

pub(crate) fn cell_inner(i: u16) -> Option<NTy> {
    RECS.with(|r| r.borrow().get(i as usize).and_then(|d| d.cell_of))
}

/// Install checked cell payloads after ordinary source types are fixed. No
/// existing type index moves, and the mutable slot is not a value-copy tuple.
pub(crate) fn install_cell_types() {
    for t in crate::lambda_sigs::cell_types() {
        let key = crate::lambda_sigs::cell_key(&t);
        if rec_index(&key).is_some() { continue; }
        let Some(inner) = checked_kind(&t) else { continue };
        let inner = if is_lane_scalar(&t) { NTy::I64 } else { inner };
        RECS.with(|r| {
            let mut r = r.borrow_mut();
            let i = r.len() as u16;
            r.push(RecDecl { name: key, cell_of: Some(inner), ..Default::default() });
            r.push(RecDecl { name: crate::lambda_sigs::cell_wrapper_key(&t), fields: vec![NTy::I64, NTy::Rec(i)], ..Default::default() });
        });
    }
}
