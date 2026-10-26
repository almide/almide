//! The http server's stderr quoting (`http.serve: request <id>: handler error:
//! <err quoted>`, C-409 / C-434) escapes exactly C-418's set (#4126): every
//! character `almide_rt_errtext_is_escaped` names reads `\u{hex}`, `"` and `\`
//! read `\"` and `\\`, and every other character is copied. The set is read
//! from the escaper's own predicate, so the two cannot drift; before, the
//! quoting used `char::is_control` and let the bidi controls through raw.
use almide_rt_core::http_server_core::{almide_rt_errtext_is_escaped, http_server_quote};

#[test]
fn the_server_quote_escapes_exactly_c418s_set() {
    for cp in 0u32..=0x2fff {
        let Some(c) = char::from_u32(cp) else { continue };
        let got = http_server_quote(&c.to_string());
        let want = match c {
            '"' => "\"\\\"\"".to_string(),
            '\\' => "\"\\\\\"".to_string(),
            c if almide_rt_errtext_is_escaped(c) => format!("\"\\u{{{cp:x}}}\""),
            c => format!("\"{c}\""),
        };
        assert_eq!(got, want, "U+{cp:04X}");
    }
}
