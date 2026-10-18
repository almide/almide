//! The v1 native header's Almide String (ADR-0013): one alias, as the v0
//! prelude spells it, and the shared buffer it points at.

/// The Almide String, as the v0 prelude spells it (ADR-0013): the shared
/// `AlmideStr` block, spliced in from `almide-str`.
pub(crate) fn almide_string_alias() -> String {
    format!(
        "// ADR-0013: the shared string buffer.\n\
         #[allow(dead_code, unused_imports)]\nmod almide_str_block {{\n{}\n}}\n\
         use almide_str_block::AlmideStr;\ntype AlmideString = AlmideStr;\n\n",
        almide_str::SOURCE
    )
}
