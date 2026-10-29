//! The Almide `String` in the runtime prelude (ADR-0013).
//!
//! Generated code spells the Almide String `AlmideString`, and it is
//! `AlmideStr`: the shared `[rc, spare, len | bytes]` block of the
//! `almide-str` crate, spliced in here (docs/roadmap/active/adr-0013-string-sharing.md).
//! A clone shares the block; an edit goes through a unique view.

/// The alias, the boundary conversion trait, the thread boundary, the
/// `AlmideConcat` trait and its impls.
pub(crate) fn string_prelude(vis: &str) -> String {
    let mut s = String::new();
    s.push_str("#[allow(dead_code, unused_imports)]\n");
    s.push_str(&format!("{vis}mod almide_str_block {{\n{}\n}}\n", almide_str::SOURCE));
    // Only `almide`-prefixed names reach the top level: a user fn of the
    // same name would collide (E0428). The unique view stays in its module.
    s.push_str(&format!("{vis}use almide_str_block::{{almide_str_mark_reachable, AlmideStr}};\n"));
    s.push_str(&format!("{vis}type AlmideString = AlmideStr;\n"));
    // A runtime function converts what it returns once, at its boundary.
    s.push_str(&format!("{vis}trait AlmideIntoStr {{ type Out; fn into_almide(self) -> Self::Out; }}\n"));
    s.push_str(&shared_thread_boundary(vis));
    // `x.to_almide()`: an owned Almide String copied from any string-typed value.
    s.push_str(&format!("{vis}trait AlmideToStr {{ fn to_almide(&self) -> AlmideString; }}\nimpl AlmideToStr for str {{ #[inline(always)] fn to_almide(&self) -> AlmideStr {{ AlmideStr::from(self) }} }}\n"));
    s.push_str("impl AlmideRepr for AlmideStr { fn almide_repr(&self) -> String { almide_repr_str(self) } }\n");
    // Built straight into one exact block. Rendering a `String` and converting
    // it pays a second allocation, copy and free per result, which cost
    // strchurn 41% on the step 7 A/B.
    s.push_str(&format!("#[inline] {vis}fn almide_str_from_i64(n: i64) -> AlmideStr {{ AlmideStr::from_i64(n) }}\n"));
    s.push_str(&format!("#[inline] {vis}fn almide_str_join(parts: &[AlmideStr], sep: &str) -> AlmideStr {{ AlmideStr::join(parts, sep) }}\n"));
    s.push_str(INTO_IMPLS);
    s.push_str(&concat_trait(vis));
    s.push_str(CONCAT_IMPLS);
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

/// `into_almide`: `String` converts, containers map their elements, and
/// every other leaf is the identity.
const INTO_IMPLS: &str = "\
impl AlmideIntoStr for String { type Out = AlmideStr; #[inline(always)] fn into_almide(self) -> AlmideStr { AlmideStr::from(self) } }
impl AlmideIntoStr for AlmideStr { type Out = AlmideStr; #[inline(always)] fn into_almide(self) -> AlmideStr { self } }
impl<T: AlmideIntoStr> AlmideIntoStr for Vec<T> { type Out = Vec<T::Out>; #[inline(always)] fn into_almide(self) -> Self::Out { self.into_iter().map(AlmideIntoStr::into_almide).collect() } }
impl<T: AlmideIntoStr> AlmideIntoStr for Option<T> { type Out = Option<T::Out>; #[inline(always)] fn into_almide(self) -> Self::Out { self.map(AlmideIntoStr::into_almide) } }
impl<A: AlmideIntoStr, B: AlmideIntoStr> AlmideIntoStr for Result<A, B> { type Out = Result<A::Out, B::Out>; #[inline(always)] fn into_almide(self) -> Self::Out { match self { Ok(a) => Ok(a.into_almide()), Err(b) => Err(b.into_almide()) } } }
impl<A: AlmideIntoStr, B: AlmideIntoStr> AlmideIntoStr for (A, B) { type Out = (A::Out, B::Out); #[inline(always)] fn into_almide(self) -> Self::Out { (self.0.into_almide(), self.1.into_almide()) } }
impl<A: AlmideIntoStr, B: AlmideIntoStr, C: AlmideIntoStr> AlmideIntoStr for (A, B, C) { type Out = (A::Out, B::Out, C::Out); #[inline(always)] fn into_almide(self) -> Self::Out { (self.0.into_almide(), self.1.into_almide(), self.2.into_almide()) } }
impl<K: AlmideIntoStr, V: AlmideIntoStr> AlmideIntoStr for std::collections::HashMap<K, V> where K::Out: std::hash::Hash + Eq { type Out = std::collections::HashMap<K::Out, V::Out>; fn into_almide(self) -> Self::Out { self.into_iter().map(|(k, v)| (k.into_almide(), v.into_almide())).collect() } }
macro_rules! almide_into_str_leaf { ($($t:ty),*) => { $(impl AlmideIntoStr for $t { type Out = $t; #[inline(always)] fn into_almide(self) -> $t { self } })* } }
almide_into_str_leaf!((), bool, char, u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64);
";

/// `+` on the Almide String. The left operand is consumed: when it is the
/// only owner it grows in place, otherwise a fresh block is built.
/// A literal on the left prepends into an owned right operand's block the
/// same way (`AlmideStr::prepend`).
const CONCAT_IMPLS: &str = "\
impl AlmideConcat<AlmideStr> for AlmideStr { type Output = AlmideStr; #[inline(always)] fn concat(self, rhs: AlmideStr) -> AlmideStr { AlmideStr::concat(self, &rhs) } }
impl AlmideConcat<&AlmideStr> for AlmideStr { type Output = AlmideStr; #[inline(always)] fn concat(self, rhs: &AlmideStr) -> AlmideStr { AlmideStr::concat(self, rhs) } }
impl AlmideConcat<&str> for AlmideStr { type Output = AlmideStr; #[inline(always)] fn concat(self, rhs: &str) -> AlmideStr { AlmideStr::concat(self, rhs) } }
impl AlmideConcat<String> for AlmideStr { type Output = AlmideStr; #[inline(always)] fn concat(self, rhs: String) -> AlmideStr { AlmideStr::concat(self, &rhs) } }
impl AlmideConcat<AlmideStr> for &str { type Output = AlmideStr; #[inline(always)] fn concat(self, rhs: AlmideStr) -> AlmideStr { rhs.prepend(self) } }
impl AlmideConcat<&str> for &str { type Output = AlmideStr; #[inline(always)] fn concat(self, rhs: &str) -> AlmideStr { let mut s = AlmideStr::with_capacity(self.len() + rhs.len()); s.push_str(self); s.push_str(rhs); s } }
impl AlmideConcat<String> for String { type Output = String; #[inline(always)] fn concat(self, rhs: String) -> String { let mut s = self; s.push_str(&rhs); s } }
impl AlmideConcat<&str> for String { type Output = String; #[inline(always)] fn concat(self, rhs: &str) -> String { let mut s = self; s.push_str(rhs); s } }
";

#[cfg(test)]
mod tests {
    use super::*;

    /// The runtime crate (`--release`'s rlib fast path) renders the prelude
    /// with `vis = "pub "`. An attribute after the visibility (`pub #[inline]
    /// fn`) does not parse, so the rlib build fails and every
    /// release build silently falls back to the monolithic cargo build.
    #[test]
    fn a_pub_prelude_puts_attributes_before_the_visibility() {
        for line in string_prelude("pub ").lines() {
            assert!(!line.contains("pub #["), "a prelude line does not parse as an item: {line}");
        }
    }
}
