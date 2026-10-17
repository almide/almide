//! The Almide `String` in the runtime prelude (ADR-0013 stage 2).
//!
//! Generated code spells the Almide String `AlmideString`. By default it is
//! `String` itself, so the runtime and every `@extern(rust)` host compile
//! unchanged. `ALMIDE_SHARED_STR=1` points it at `AlmideStr`, the shared
//! `[rc, len, cap | bytes]` block of the `almide-str` crate, spliced in here
//! (docs/roadmap/active/adr-0013-string-sharing.md, step 2). The flag stays
//! off until step 7; every construction site is spelled so that it is the
//! identity under `String` and a conversion under `AlmideStr`.

/// Whether this compile renders the shared buffer (`ALMIDE_SHARED_STR=1`).
pub fn shared_str_enabled() -> bool {
    almide_base::env::flag("ALMIDE_SHARED_STR")
}

/// The alias, the boundary conversion trait, the `AlmideConcat` trait and
/// its String impls.
pub(crate) fn string_prelude(vis: &str) -> String {
    if shared_str_enabled() { shared_prelude(vis) } else { owned_prelude(vis) }
}

fn owned_prelude(vis: &str) -> String {
    let mut s = String::new();
    s.push_str(&format!("{vis}type AlmideString = String;\n"));
    // A runtime function converts what it returns once, at its boundary:
    // the identity while the alias is `String`.
    s.push_str(&format!("{vis}trait AlmideIntoStr {{ type Out; fn into_almide(self) -> Self::Out; }}\nimpl<T> AlmideIntoStr for T {{ type Out = T; #[inline(always)] fn into_almide(self) -> T {{ self }} }}\n"));
    s.push_str(SHARED_ONLY_OFF);
    // The thread boundary marks nothing while the alias is `String`.
    s.push_str(&format!("{vis}trait AlmideShare {{ #[inline(always)] fn almide_share(&self) {{}} }}\nimpl<T: ?Sized> AlmideShare for T {{}}\n"));
    s.push_str(&format!("{vis}fn almide_shared_value<T: AlmideShare>(v: T) -> T {{ v.almide_share(); v }}\n"));
    // `x.to_almide()`: an owned Almide String copied from any string-typed value.
    s.push_str(&format!("{vis}trait AlmideToStr {{ fn to_almide(&self) -> AlmideString; }}\nimpl AlmideToStr for str {{ #[inline(always)] fn to_almide(&self) -> String {{ self.to_string() }} }}\n"));
    s.push_str(&concat_trait(vis));
    // The owned-String impls extend in place (`push_str`) instead of
    // `format!`: byte-identical output, but an owned accumulator keeps its
    // capacity — with the TCO accumulator move this makes a tail-recursive
    // string builder amortized O(n) instead of reallocating every step.
    s.push_str("impl AlmideConcat<String> for String { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { let mut s = self; s.push_str(&rhs); s } }\n");
    s.push_str("impl AlmideConcat<&str> for String { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { let mut s = self; s.push_str(rhs); s } }\n");
    // A literal left operand (`"k" + x`) prepends into the owned right operand's buffer: no allocation
    // when its capacity covers both (`int.to_string` reserves 19 bytes), else one grow — never a fresh String.
    s.push_str("impl AlmideConcat<String> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { let mut r = rhs; r.insert_str(0, self); r } }\n");
    s.push_str("impl AlmideConcat<&str> for &str { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { format!(\"{}{}\", self, rhs) } }\n");
    s
}

/// The shared buffer: the `almide-str` source in a module of its own (its
/// `use` lines must not meet the runtime's), the alias, a conversion trait
/// that walks the shapes runtime functions return, and `AlmideConcat` impls
/// that append in place only through the unique view (`AlmideStr::concat`).
fn shared_prelude(vis: &str) -> String {
    let mut s = String::new();
    s.push_str("#[allow(dead_code, unused_imports)]\n");
    s.push_str(&format!("{vis}mod almide_str_block {{\n{}\n}}\n", almide_str::SOURCE));
    s.push_str(&format!("{vis}use almide_str_block::{{almide_str_mark_reachable, AlmideStr, UniqueStr}};\n"));
    s.push_str(&format!("{vis}type AlmideString = AlmideStr;\n"));
    s.push_str(&format!("{vis}trait AlmideIntoStr {{ type Out; fn into_almide(self) -> Self::Out; }}\n"));
    s.push_str(SHARED_ONLY_ON);
    s.push_str(&shared_thread_boundary(vis));
    s.push_str(&format!("{vis}trait AlmideToStr {{ fn to_almide(&self) -> AlmideString; }}\nimpl AlmideToStr for str {{ #[inline(always)] fn to_almide(&self) -> AlmideStr {{ AlmideStr::from(self) }} }}\n"));
    s.push_str("impl AlmideRepr for AlmideStr { fn almide_repr(&self) -> String { almide_repr_str(self) } }\n");
    s.push_str(SHARED_INTO_IMPLS);
    s.push_str(&concat_trait(vis));
    s.push_str(SHARED_CONCAT_IMPLS);
    s
}

fn concat_trait(vis: &str) -> String {
    format!("{vis}trait AlmideConcat<Rhs> {{ type Output; fn concat(self, rhs: Rhs) -> Self::Output; }}\n")
}

/// `x.almide_share()` marks every block a value reaches thread-shared (ADR-0013
/// design point 3): a value is `Clone`, a slice marks its elements, a `str` holds no block.
fn shared_thread_boundary(vis: &str) -> String {
    let mut s = format!("{vis}trait AlmideShare {{ fn almide_share(&self); }}\n");
    s.push_str("impl<T: Clone> AlmideShare for T { #[inline] fn almide_share(&self) { almide_str_mark_reachable(self) } }\n");
    s.push_str("impl<T: AlmideShare> AlmideShare for [T] { fn almide_share(&self) { for e in self { e.almide_share() } } }\n");
    s.push_str("impl AlmideShare for str { #[inline(always)] fn almide_share(&self) {} }\n");
    s.push_str(&format!("{vis}fn almide_shared_value<T: AlmideShare>(v: T) -> T {{ v.almide_share(); v }}\n"));
    s
}

/// `almide_into_str_impls! { .. }` wraps the conversion impls a runtime
/// module adds for its own types (`AlmideMap`, `AlmideValue`): they exist only
/// under the shared buffer, where the blanket identity impl is absent.
const SHARED_ONLY_OFF: &str = "#[allow(unused_macros)]\nmacro_rules! almide_into_str_impls { ($($t:tt)*) => {} }\n";
const SHARED_ONLY_ON: &str = "#[allow(unused_macros)]\nmacro_rules! almide_into_str_impls { ($($t:tt)*) => { $($t)* } }\n";

/// `into_almide` under the shared buffer: `String` converts, containers map
/// their elements, and every other leaf is the identity.
const SHARED_INTO_IMPLS: &str = "\
impl AlmideIntoStr for String { type Out = AlmideStr; #[inline(always)] fn into_almide(self) -> AlmideStr { AlmideStr::from(self) } }
impl AlmideIntoStr for AlmideStr { type Out = AlmideStr; #[inline(always)] fn into_almide(self) -> AlmideStr { self } }
impl<T: AlmideIntoStr> AlmideIntoStr for Vec<T> { type Out = Vec<T::Out>; #[inline(always)] fn into_almide(self) -> Self::Out { self.into_iter().map(AlmideIntoStr::into_almide).collect() } }
impl<T: AlmideIntoStr> AlmideIntoStr for Option<T> { type Out = Option<T::Out>; #[inline(always)] fn into_almide(self) -> Self::Out { self.map(AlmideIntoStr::into_almide) } }
impl<A: AlmideIntoStr, B: AlmideIntoStr> AlmideIntoStr for Result<A, B> { type Out = Result<A::Out, B::Out>; #[inline(always)] fn into_almide(self) -> Self::Out { match self { Ok(a) => Ok(a.into_almide()), Err(b) => Err(b.into_almide()) } } }
impl<A: AlmideIntoStr, B: AlmideIntoStr> AlmideIntoStr for (A, B) { type Out = (A::Out, B::Out); #[inline(always)] fn into_almide(self) -> Self::Out { (self.0.into_almide(), self.1.into_almide()) } }
impl<A: AlmideIntoStr, B: AlmideIntoStr, C: AlmideIntoStr> AlmideIntoStr for (A, B, C) { type Out = (A::Out, B::Out, C::Out); #[inline(always)] fn into_almide(self) -> Self::Out { (self.0.into_almide(), self.1.into_almide(), self.2.into_almide()) } }
impl<K: AlmideIntoStr, V: AlmideIntoStr> AlmideIntoStr for HashMap<K, V> where K::Out: std::hash::Hash + Eq { type Out = HashMap<K::Out, V::Out>; fn into_almide(self) -> Self::Out { self.into_iter().map(|(k, v)| (k.into_almide(), v.into_almide())).collect() } }
macro_rules! almide_into_str_leaf { ($($t:ty),*) => { $(impl AlmideIntoStr for $t { type Out = $t; #[inline(always)] fn into_almide(self) -> $t { self } })* } }
almide_into_str_leaf!((), bool, char, u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64);
";

/// `+` on the shared buffer. The left operand is consumed: when it is the
/// only owner it grows in place, otherwise a fresh block is built.
const SHARED_CONCAT_IMPLS: &str = "\
impl AlmideConcat<AlmideStr> for AlmideStr { type Output = AlmideStr; #[inline(always)] fn concat(self, rhs: AlmideStr) -> AlmideStr { AlmideStr::concat(self, &rhs) } }
impl AlmideConcat<&AlmideStr> for AlmideStr { type Output = AlmideStr; #[inline(always)] fn concat(self, rhs: &AlmideStr) -> AlmideStr { AlmideStr::concat(self, rhs) } }
impl AlmideConcat<&str> for AlmideStr { type Output = AlmideStr; #[inline(always)] fn concat(self, rhs: &str) -> AlmideStr { AlmideStr::concat(self, rhs) } }
impl AlmideConcat<String> for AlmideStr { type Output = AlmideStr; #[inline(always)] fn concat(self, rhs: String) -> AlmideStr { AlmideStr::concat(self, &rhs) } }
impl AlmideConcat<AlmideStr> for &str { type Output = AlmideStr; #[inline(always)] fn concat(self, rhs: AlmideStr) -> AlmideStr { AlmideStr::from(self).concat(&rhs) } }
impl AlmideConcat<&str> for &str { type Output = AlmideStr; #[inline(always)] fn concat(self, rhs: &str) -> AlmideStr { AlmideStr::from(self).concat(rhs) } }
impl AlmideConcat<String> for String { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { let mut s = self; s.push_str(&rhs); s } }
impl AlmideConcat<&str> for String { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { let mut s = self; s.push_str(rhs); s } }
";
