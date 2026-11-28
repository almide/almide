//! The compiler's ONE output sink escaper (decision row front.diagnostic-echo,
//! #4819).
//!
//! Everything the `almide` binary writes to its own stdout or stderr — a
//! diagnostic, a hint, an echoed source line, a test name, a JSON envelope,
//! an LSP message, a progress line — leaves through the four sink functions
//! of `src/lib.rs` (`out`, `out_no_nl`, `err`, `err_no_nl`), and each of them
//! writes [`visible`] of its text. So a hidden character a sink forgot to
//! escape is escaped there anyway: a NEW sink is escaped by construction, not
//! by remembering to call an escaper. `tests/echo_sink_test.rs` refuses a
//! print macro or a raw stdout / stderr handle anywhere else in the binary's
//! sources; the one raw channel is the byte-exact relay of what a USER
//! PROGRAM wrote (`almide run --target wasm` replaying the embedded host's
//! captured stdout), which is the program's output, not the compiler's.
//!
//! The escape set is C-418's ([`crate::err_text::ALMIDE_RT_ERRTEXT_ESCAPED`])
//! minus the three layout characters a multi-line report needs (`\t`, `\n`,
//! `\r`), written `\u{XXXX}` exactly as [`crate::err_text::escape_hidden`]
//! writes it, so a text a sink already escaped passes through unchanged.
//!
//! The one structure kept raw is a Select Graphic Rendition sequence (ECMA-48
//! `ESC [ Ps ; … m`) whose every parameter sets intensity or a colour — the
//! renderer's own colouring. Its parameters are read by their ECMA-48
//! meaning: reset (0), bold / faint (1, 2), normal intensity (22), the
//! foreground and background colours (30–37, 39, 40–47, 49, 90–97,
//! 100–107). A sequence carrying anything else — conceal (8), which hides
//! text, a cursor move, a screen erase, an OSC title — is escaped like any
//! other control.

use std::borrow::Cow;

/// Is `c` a character the sink escapes?
fn hidden(c: char) -> bool {
    !matches!(c, '\t' | '\n' | '\r') && crate::err_text::almide_rt_errtext_is_escaped(c)
}

/// Does an SGR parameter set only intensity or a colour (ECMA-48 8.3.117)?
fn colour_param(p: &str) -> bool {
    if p.is_empty() {
        // `ESC [ m` is `ESC [ 0 m`.
        return true;
    }
    let Ok(n) = p.parse::<u16>() else { return false };
    matches!(n, 0 | 1 | 2 | 22 | 30..=37 | 39 | 40..=47 | 49 | 90..=97 | 100..=107)
}

/// The length in bytes of the colour SGR sequence at the start of `s`
/// (which begins with ESC), or `None` when it is not one.
fn colour_sgr_len(s: &str) -> Option<usize> {
    let rest = s.strip_prefix("\u{1b}[")?;
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == ';'))?;
    if rest.as_bytes()[end] != b'm' {
        return None;
    }
    rest[..end].split(';').all(colour_param).then_some(2 + end + 1)
}

/// `s` as the sink writes it: every hidden character written as its escape
/// `\u{XXXX}`, but a colour SGR sequence kept (see the module header).
pub fn visible(s: &str) -> Cow<'_, str> {
    if !s.chars().any(hidden) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 16);
    let mut i = 0;
    while let Some(c) = s[i..].chars().next() {
        if c == '\u{1b}'
            && let Some(n) = colour_sgr_len(&s[i..])
        {
            out.push_str(&s[i..i + n]);
            i += n;
            continue;
        }
        if hidden(c) {
            out.push_str(&format!("\\u{{{:04X}}}", c as u32));
        } else {
            out.push(c);
        }
        i += c.len_utf8();
    }
    Cow::Owned(out)
}

/// `json`, the text of a JSON document, as the sink writes it: every hidden
/// character written as its JSON escape `\uXXXX` (a surrogate pair above the
/// BMP), which every JSON reader decodes back to the same character. A
/// hidden character can only sit inside a JSON string (outside one, JSON
/// allows only TAB, LF, CR and space), so the document means what it meant.
pub fn json_visible(json: &str) -> Cow<'_, str> {
    if !json.chars().any(hidden) {
        return Cow::Borrowed(json);
    }
    let mut out = String::with_capacity(json.len() + 16);
    for c in json.chars() {
        if hidden(c) {
            let mut units = [0u16; 2];
            for u in c.encode_utf16(&mut units).iter() {
                out.push_str(&format!("\\u{u:04x}"));
            }
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::{json_visible, visible};

    #[test]
    fn a_json_document_keeps_its_meaning() {
        assert_eq!(json_visible("{\"m\":\"a\u{202E}b\u{E0041}\"}"), "{\"m\":\"a\\u202eb\\udb40\\udc41\"}");
        assert!(matches!(json_visible("{\"m\":1}"), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn hidden_characters_are_escaped_and_layout_is_kept() {
        assert_eq!(visible("a\u{202E}b\u{200B}c\n\td"), "a\\u{202E}b\\u{200B}c\n\td");
        assert_eq!(visible("tag\u{E0041}"), "tag\\u{E0041}");
        assert!(matches!(visible("plain text"), std::borrow::Cow::Borrowed(_)));
    }

    /// The renderer's colouring passes; every other control sequence is
    /// escaped — the control that the rule is read off the parameters.
    #[test]
    fn only_a_colour_sgr_sequence_stays_raw() {
        for keep in ["\u{1b}[1;31m", "\u{1b}[0m", "\u{1b}[m", "\u{1b}[2m", "\u{1b}[34m", "\u{1b}[1;36m", "\u{1b}[97;104m"] {
            assert_eq!(visible(&format!("x{keep}y")), format!("x{keep}y"), "{keep:?}");
        }
        for (bad, shown) in [
            ("\u{1b}[8m", "\\u{001B}[8m"),
            ("\u{1b}[2J", "\\u{001B}[2J"),
            ("\u{1b}[1;8m", "\\u{001B}[1;8m"),
            ("\u{1b}]0;title\u{7}", "\\u{001B}]0;title\\u{0007}"),
            ("\u{1b}", "\\u{001B}"),
            ("\u{1b}[31", "\\u{001B}[31"),
        ] {
            assert_eq!(visible(&format!("x{bad}y")), format!("x{shown}y"), "{bad:?}");
        }
    }

    /// An already-escaped text is a fixed point.
    #[test]
    fn an_escaped_text_passes_unchanged() {
        let once = visible("a\u{202E}b").into_owned();
        assert_eq!(visible(&once), once);
    }
}
