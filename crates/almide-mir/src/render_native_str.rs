//! The v1 native header's Almide String (ADR-0013 stage 2): one alias, as
//! the v0 prelude spells it, and under `ALMIDE_SHARED_STR=1` the shared
//! buffer it points at.

/// The Almide String, as the v0 prelude spells it (ADR-0013 stage 2): `String`
/// by default; under `ALMIDE_SHARED_STR=1` the shared `AlmideStr` block,
/// spliced in from `almide-str`.
pub(crate) fn almide_string_alias() -> String {
    if !std::env::var_os("ALMIDE_SHARED_STR").map_or(true, |v| almide_base::env::is_on(&v)) {
        return "// ADR-0013 stage 2, step 1: the Almide String, as the v0 prelude spells it.\n\
                type AlmideString = String;\n\n".to_string();
    }
    format!(
        "// ADR-0013 stage 2: the shared string buffer (ALMIDE_SHARED_STR=1).\n\
         #[allow(dead_code, unused_imports)]\nmod almide_str_block {{\n{}\n}}\n\
         use almide_str_block::AlmideStr;\ntype AlmideString = AlmideStr;\n\n",
        almide_str::SOURCE
    )
}
