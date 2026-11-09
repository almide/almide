//! The v1 native header's Almide String (ADR-0013): one alias, as the v0
//! prelude spells it, and the shared buffer it points at.

/// The Almide String, as the v0 prelude spells it (ADR-0013): the shared
/// `AlmideStr` block, spliced in from `almide-str`.
pub(crate) fn almide_string_alias() -> String {
    format!(
        "// ADR-0013: the shared string buffer.\n\
         #[allow(dead_code, unused_imports)]\npub mod almide_str_block {{\n{}\n}}\n\
         pub use almide_str_block::AlmideStr;\npub use almide_str_block::almide_str_mark_reachable;\npub type AlmideString = AlmideStr;\n{}\n",
        almide_str::SOURCE,
        ALMIDE_BYTES
    )
}

/// The program's `Bytes` (#4623): one shared buffer behind an `Rc`, as v0's
/// `AlmideRcCow<Vec<u8>>`. A copy (`Dup`, `.clone()`) takes a reference
/// instead of copying the bytes; a write goes through `make_mut`, which
/// copies only while another holder shares the buffer, so every value keeps
/// the bytes it had (value semantics). Reads deref to the runtime's
/// `&Vec<u8>` / `&[u8]`; a runtime result converts with `From` at the call.
const ALMIDE_BYTES: &str = "#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]\npub struct AlmideBytes(std::rc::Rc<Vec<u8>>);\nimpl Clone for AlmideBytes { #[inline] fn clone(&self) -> Self { AlmideBytes(std::rc::Rc::clone(&self.0)) } }\nimpl std::ops::Deref for AlmideBytes { type Target = Vec<u8>; #[inline] fn deref(&self) -> &Vec<u8> { &self.0 } }\nimpl std::ops::DerefMut for AlmideBytes { #[inline] fn deref_mut(&mut self) -> &mut Vec<u8> { std::rc::Rc::make_mut(&mut self.0) } }\nimpl From<Vec<u8>> for AlmideBytes { #[inline] fn from(v: Vec<u8>) -> Self { AlmideBytes(std::rc::Rc::new(v)) } }\nimpl From<AlmideBytes> for Vec<u8> { #[inline] fn from(b: AlmideBytes) -> Vec<u8> { std::rc::Rc::try_unwrap(b.0).unwrap_or_else(|r| (*r).clone()) } }\n";
