// Included by render_native_rec: the shared typed closure environment.
thread_local! {
    static CAPTURE_KINDS: RefCell<Vec<NTy>> = const { RefCell::new(Vec::new()) };
    /// Some closure of the render holds more words than the inline four, or
    /// a number known only at run time.
    static WIDE_CLOSURES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn reset_capture_kinds() { CAPTURE_KINDS.with(|k| k.borrow_mut().clear()); WIDE_CLOSURES.with(|w| w.set(false)); }

/// The closure construction of `words` (render_native_env_slots): `n` words
/// padded to at least the inline four, so the array's length `m` picks the
/// storage at compile time (`__AlmdWords::of`): inline where the words fit,
/// built in place.
pub(crate) fn closure_words(words: &[String]) -> String {
    let (n, m) = (words.len(), words.len().max(4));
    WIDE_CLOSURES.with(|w| w.set(w.get() | (m > 4)));
    let padded = words.iter().cloned().chain(std::iter::repeat_n("0".to_string(), m - n));
    format!("__AlmdWords::of::<{n}, {m}>([{}])", padded.collect::<Vec<_>>().join(", "))
}

/// A closure whose length is known only at run time was built.
pub(crate) fn note_wide_closure() { WIDE_CLOSURES.with(|w| w.set(true)); }

/// The closure environment of a render whose every closure keeps its words
/// inline and captures no heap value: nothing in it to release, so a call
/// that builds one can still end in a tail call.
const LEAN_CLOSURE_ENV: &str = "#[derive(Clone, Default)]\nstruct __AlmdClosure { words: __AlmdWords, heaps: Option<__AlmdCapture> }\n#[derive(Clone)]\nenum __AlmdWords { Inline(u8, [i64; 4]) }\nimpl Default for __AlmdWords { fn default() -> Self { __AlmdWords::Inline(0, [0; 4]) } }\nimpl __AlmdWords { #[inline(always)] fn of<const N: usize, const M: usize>(w: [i64; 4]) -> Self { __AlmdWords::Inline(N as u8, w) } fn zeroed(n: usize) -> Self { __AlmdWords::Inline(n as u8, [0; 4]) } }\nimpl std::ops::Deref for __AlmdWords { type Target = [i64]; #[inline] fn deref(&self) -> &[i64] { match self { __AlmdWords::Inline(n, a) => &a[..*n as usize] } } }\nimpl std::ops::DerefMut for __AlmdWords { #[inline] fn deref_mut(&mut self) -> &mut [i64] { match self { __AlmdWords::Inline(n, a) => &mut a[..*n as usize] } } }\nimpl __AlmdClosure { fn new(n: usize) -> Self { Self { words: __AlmdWords::zeroed(n), heaps: None } } }\ntrait __AlmdEnv { fn words(&self) -> &[i64]; fn len(&self) -> usize { self.words().len() } fn heap(&self, _: usize) -> Option<&__AlmdCapture> { None } }\nimpl __AlmdEnv for __AlmdClosure { fn words(&self) -> &[i64] { &*self.words } fn heap(&self, _: usize) -> Option<&__AlmdCapture> { self.heaps.as_ref() } }\nimpl<T: __AlmdEnv + ?Sized> __AlmdEnv for &T { fn words(&self) -> &[i64] { (**self).words() } fn heap(&self, k: usize) -> Option<&__AlmdCapture> { (**self).heap(k) } }\nimpl<const N: usize> __AlmdEnv for [i64; N] { fn words(&self) -> &[i64] { self } }\n#[derive(Clone)]\nenum __AlmdCapture {\n}\n\n";

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
    if !WIDE_CLOSURES.with(|w| w.get()) && CAPTURE_KINDS.with(|ks| ks.borrow().is_empty()) {
        return LEAN_CLOSURE_ENV.to_string();
    }
    let mut out = String::from("#[derive(Clone, Default)]\nstruct __AlmdClosure { words: __AlmdWords, heaps: __AlmdHeaps }\n#[derive(Clone, Default)]\nenum __AlmdHeaps { #[default] Empty, One(usize, __AlmdCapture), Many(Vec<Option<__AlmdCapture>>) }\n#[derive(Clone)]\nenum __AlmdWords { Inline(u8, [i64; 4]), Heap(Vec<i64>) }\nimpl Default for __AlmdWords { fn default() -> Self { __AlmdWords::Inline(0, [0; 4]) } }\nimpl __AlmdWords { #[inline(always)] fn of<const N: usize, const M: usize>(w: [i64; M]) -> Self { if M == 4 { __AlmdWords::Inline(N as u8, std::array::from_fn(|i| w[i])) } else { __AlmdWords::Heap(w.to_vec()) } } fn zeroed(n: usize) -> Self { if n <= 4 { __AlmdWords::Inline(n as u8, [0; 4]) } else { __AlmdWords::Heap(vec![0; n]) } } }\nimpl std::ops::Deref for __AlmdWords { type Target = [i64]; #[inline] fn deref(&self) -> &[i64] { match self { __AlmdWords::Inline(n, a) => &a[..*n as usize], __AlmdWords::Heap(v) => v } } }\nimpl std::ops::DerefMut for __AlmdWords { #[inline] fn deref_mut(&mut self) -> &mut [i64] { match self { __AlmdWords::Inline(n, a) => &mut a[..*n as usize], __AlmdWords::Heap(v) => v } } }\nimpl __AlmdClosure { fn new(n: usize) -> Self { Self { words: __AlmdWords::zeroed(n), heaps: __AlmdHeaps::Empty } } fn put(&mut self, k: usize, v: __AlmdCapture) { self.heaps = match std::mem::take(&mut self.heaps) { __AlmdHeaps::Empty => __AlmdHeaps::One(k, v), __AlmdHeaps::One(j, _) if j == k => __AlmdHeaps::One(k, v), __AlmdHeaps::One(j, w) => { let mut h = Vec::new(); h.resize_with(self.words.len().max(j.max(k) + 1), || None); h[j] = Some(w); h[k] = Some(v); __AlmdHeaps::Many(h) } __AlmdHeaps::Many(mut h) => { if h.len() <= k { h.resize_with(k + 1, || None); } h[k] = Some(v); __AlmdHeaps::Many(h) } }; } }\ntrait __AlmdEnv { fn words(&self) -> &[i64]; fn len(&self) -> usize { self.words().len() } fn heap(&self, _: usize) -> Option<&__AlmdCapture> { None } }\nimpl __AlmdEnv for __AlmdClosure { fn words(&self) -> &[i64] { &*self.words } fn heap(&self, k: usize) -> Option<&__AlmdCapture> { match &self.heaps { __AlmdHeaps::One(j, w) if *j == k => Some(w), __AlmdHeaps::Many(h) => h.get(k).and_then(Option::as_ref), _ => None } } }\nimpl<T: __AlmdEnv + ?Sized> __AlmdEnv for &T { fn words(&self) -> &[i64] { (**self).words() } fn heap(&self, k: usize) -> Option<&__AlmdCapture> { (**self).heap(k) } }\nimpl<const N: usize> __AlmdEnv for [i64; N] { fn words(&self) -> &[i64] { self } }\n#[derive(Clone)]\nenum __AlmdCapture {\n");
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
