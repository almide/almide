//! The v1 native header's Almide String (ADR-0013) and Almide List
//! (ADR-0037): the aliases, as the v0 prelude spells them, and the shared
//! blocks they point at.

/// The Almide String, as the v0 prelude spells it (ADR-0013): the shared
/// `AlmideStr` block, spliced in from `almide-str`. ADR-0037: the list block
/// follows it: every list the program and the runtime items it carries hold
/// is a counted block, copied only when written while shared.
pub(crate) fn almide_string_alias() -> String {
    format!(
        "// ADR-0013: the shared string buffer. ADR-0037: the shared list block.\n\
         #[allow(dead_code, unused_imports, unused_macros)]\npub mod almide_str_block {{\n{}\n{}\n}}\n\
         pub use almide_str_block::AlmideStr;\npub use almide_str_block::almide_str_mark_reachable;\npub type AlmideString = AlmideStr;\n\
         {}",
        almide_str::SOURCE,
        almide_str::LIST_SOURCE,
        almide_list_alias()
    )
}

/// ADR-0037 D1: a native `List[T]` is `AlmideList<T>`, the counted block,
/// spelled by that name in the program and in the runtime items it carries
/// (host plumbing that is not an Almide list stays a plain `Vec`).
pub(crate) fn almide_list_alias() -> String {
    format!("pub use almide_str_block::{{AlmideList, AlmideToList, AlmideListElem, AlmideListFill, AlmideListFillZeroed, AlmideZeroValid}};\n{}", almide_list_macro())
}

/// `almide_list![..]`: the `vec!` forms, building an `AlmideList` (which must
/// be in scope). A macro does not travel through a glob, so the program part
/// of a split build defines it again.
pub(crate) fn almide_list_macro() -> &'static str {
    "#[allow(unused_macros)]\nmacro_rules! almide_list {\n    \
     () => { AlmideList::new() };\n    \
     ($x:expr; $n:expr) => { (&&AlmideListElem::new($x)).almide_list_fill($n) };\n    \
     ($($x:expr),+ $(,)?) => { AlmideList::from(::std::vec![$($x),+]) };\n}\n\n"
}
