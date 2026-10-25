//! #4126: the escaper every leg's error text runs. The escape set is
//! exactly C0, DEL, C1 and the bidi controls; every other character, quote
//! and backslash included, is kept.
use almide_base::err_text::{
    almide_rt_errtext_escape as esc, almide_rt_errtext_is_escaped, almide_rt_errtext_quote,
};

#[test]
fn escapes_the_named_set_in_rust_debug_form() {
    assert_eq!(esc("a\nb\rc\td\0e"), "a\\nb\\rc\\td\\0e");
    assert_eq!(esc("\u{1b}[31m"), "\\u{1b}[31m");
    assert_eq!(
        esc("\u{7f}\u{80}\u{85}\u{9f}"),
        "\\u{7f}\\u{80}\\u{85}\\u{9f}"
    );
    assert_eq!(esc("\u{61c}\u{200e}\u{200f}"), "\\u{61c}\\u{200e}\\u{200f}");
    assert_eq!(
        esc("\u{202a}\u{202e}\u{2066}\u{2069}"),
        "\\u{202a}\\u{202e}\\u{2066}\\u{2069}"
    );
    assert_eq!(almide_rt_errtext_quote("x\ny"), "\"x\\ny\"");
}

#[test]
fn keeps_every_other_character() {
    let plain = "C:\\dir\\file \"q\".txt \u{a0}\u{200d}\u{2029}\u{2070}日本 🎉";
    assert_eq!(esc(plain), plain);
}

#[test]
fn the_set_is_exactly_the_listed_ranges_and_matches_debug_spelling() {
    let listed = |c: u32| {
        c <= 0x1f
            || (0x7f..=0x9f).contains(&c)
            || matches!(c, 0x61c | 0x200e | 0x200f)
            || (0x202a..=0x202e).contains(&c)
            || (0x2066..=0x2069).contains(&c)
    };
    for c in (0..=0x10ffffu32).filter_map(char::from_u32) {
        assert_eq!(
            almide_rt_errtext_is_escaped(c),
            listed(c as u32),
            "U+{:04X}",
            c as u32
        );
        if listed(c as u32) && !matches!(c, '\n' | '\r' | '\t' | '\0') {
            // The same spelling Rust's `{:?}` uses for a char it escapes.
            assert_eq!(esc(&c.to_string()), format!("\\u{{{:x}}}", c as u32));
        }
    }
}
