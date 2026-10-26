// err_text (C-418, #4126): the ONE escaper for user data a runtime error text
// embeds — a path, a command, a socket path, a host name. Without it a path
// holding `\n` forges a second log line and one holding ESC drives the
// reader's terminal.
//
// The escape set is the characters that move, hide or break text when
// printed, declared ONCE as [`ALMIDE_RT_ERRTEXT_ESCAPED`]:
//
// - C0 controls U+0000..=U+001F and DEL U+007F;
// - C1 controls U+0080..=U+009F (U+0085 is a line break to some readers);
// - every character of General Category Cf (format), Zl (line separator)
//   and Zp (paragraph separator), Unicode 16.0, read from the UCD's
//   DerivedGeneralCategory: the bidirectional controls (Trojan Source,
//   #4092), U+2028 / U+2029 (a line break to JS log viewers), the zero-width
//   and invisible characters (U+200B..=U+200D, U+2060..=U+2064, U+FEFF), the
//   soft hyphen U+00AD and the rest of Cf (#4265);
// - the whole Tags block U+E0000..=U+E007F (its assigned members are Cf):
//   invisible ASCII a model reads as text, an instruction smuggled to the
//   agent that reads the error (#4265).
//
// Each is written the way Rust's `{:?}` writes it: `\n` `\r` `\t` `\0`, else
// `\u{<lowercase hex, no leading zeros>}` (ESC is `\u{1b}`). EVERY OTHER
// character is kept as is — the backslash and the quote included, so an
// ordinary path (a Windows one too) reads back exactly as written. A leg that
// cannot call this file (the stock artifact's fs service in WAT, the
// self-hosted `string.__err_text`) carries the same ranges, and
// `crates/almide-base/tests/err_text.rs` reads each copy back and fails on any
// difference from this table.
//
// ONE source, every consumer: `almide_base::err_text` is this file as a
// module; runtime/rs/src/errtext.rs `include!`s it into the generated crate
// (once — the modules that call it depend on that module by its
// `almide_rt_errtext_` prefix); almide-rt-core's cores see it through
// lib.rs. Splice discipline: no `crate::` paths, no `mod`, no `use` lines,
// every name prefixed `almide_rt_errtext_`.

/// The characters C-418 escapes, as sorted, disjoint, inclusive ranges (see
/// the file header): C0, DEL and C1, then Cf + Zl + Zp of Unicode 16.0 (two
/// adjacent runs merged: U+2028..=U+2029 Zl/Zp with U+202A..=U+202E Cf), then
/// the Tags block whole.
pub const ALMIDE_RT_ERRTEXT_ESCAPED: &[(u32, u32)] = &[
    (0x0000, 0x001f),
    (0x007f, 0x009f),
    (0x00ad, 0x00ad),
    (0x0600, 0x0605),
    (0x061c, 0x061c),
    (0x06dd, 0x06dd),
    (0x070f, 0x070f),
    (0x0890, 0x0891),
    (0x08e2, 0x08e2),
    (0x180e, 0x180e),
    (0x200b, 0x200f),
    (0x2028, 0x202e),
    (0x2060, 0x2064),
    (0x2066, 0x206f),
    (0xfeff, 0xfeff),
    (0xfff9, 0xfffb),
    (0x110bd, 0x110bd),
    (0x110cd, 0x110cd),
    (0x13430, 0x1343f),
    (0x1bca0, 0x1bca3),
    (0x1d173, 0x1d17a),
    (0xe0000, 0xe007f),
];

/// True for a character C-418 escapes in an error text.
pub fn almide_rt_errtext_is_escaped(c: char) -> bool {
    let n = c as u32;
    ALMIDE_RT_ERRTEXT_ESCAPED.iter().any(|&(lo, hi)| lo <= n && n <= hi)
}

/// `s` with every C-418 character escaped (see the file header).
pub fn almide_rt_errtext_escape(s: &str) -> String {
    if !s.chars().any(almide_rt_errtext_is_escaped) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if almide_rt_errtext_is_escaped(c) => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// `"s"` with `s` escaped — the operand form of `call("operand"): reason`.
pub fn almide_rt_errtext_quote(s: &str) -> String {
    format!("\"{}\"", almide_rt_errtext_escape(s))
}
