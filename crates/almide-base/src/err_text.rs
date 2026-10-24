// err_text (C-419, #4126): the ONE escaper for user data a runtime error text
// embeds — a path, a command, a socket path, a host name. Without it a path
// holding `\n` forges a second log line and one holding ESC drives the
// reader's terminal.
//
// The escape set is the characters that move or hide text when printed:
//
// - C0 controls U+0000..=U+001F and DEL U+007F;
// - C1 controls U+0080..=U+009F (U+0085 is a line break to some readers);
// - the bidirectional controls (Trojan Source, #4092): U+061C, U+200E,
//   U+200F, U+202A..=U+202E and U+2066..=U+2069.
//
// Each is written the way Rust's `{:?}` writes it: `\n` `\r` `\t` `\0`, else
// `\u{<lowercase hex, no leading zeros>}` (ESC is `\u{1b}`). EVERY OTHER
// character is kept as is — the backslash and the quote included, so an
// ordinary path (a Windows one too) reads back exactly as written, and every
// leg (native, the embedded host, the stock artifact's fs service in WAT, the
// interp VFS) can reproduce the rule from this short list.
//
// ONE source, every consumer: `almide_base::err_text` is this file as a
// module; runtime/rs/src/errtext.rs `include!`s it into the generated crate
// (once — the modules that call it depend on that module by its
// `almide_rt_errtext_` prefix); almide-rt-core's cores see it through
// lib.rs. Splice discipline: no `crate::` paths, no `mod`, no `use` lines,
// every name prefixed `almide_rt_errtext_`.

/// True for a character C-419 escapes in an error text.
pub fn almide_rt_errtext_is_escaped(c: char) -> bool {
    matches!(
        c as u32,
        0x00..=0x1f | 0x7f..=0x9f | 0x061c | 0x200e | 0x200f | 0x202a..=0x202e | 0x2066..=0x2069
    )
}

/// `s` with every C-419 character escaped (see the file header).
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
