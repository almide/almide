//! C-418 (#4126, #4265): the escaper every leg's error text runs. The escape
//! set is one table, `ALMIDE_RT_ERRTEXT_ESCAPED`: C0, DEL, C1, General
//! Category Cf / Zl / Zp of Unicode 16.0, and the Tags block. Every other
//! character, quote and backslash included, is kept.
//!
//! Two legs cannot call the Rust escaper and carry the ranges themselves: the
//! stock artifact's fs service (`crates/almide-wasi/src/fs_service.wat`,
//! `$esc_cp`) and the self-hosted `string.__err_text`
//! (`stdlib/string_err_text.almd`, `__et_escaped`). This test reads both
//! copies back out of their source and fails on any range that differs from
//! the table, so the set has one declaration. The wasm emitter and the p3
//! shim read the table itself at emit time.
use almide_base::err_text::{
    almide_rt_errtext_escape as esc, almide_rt_errtext_is_escaped, almide_rt_errtext_quote,
    ALMIDE_RT_ERRTEXT_ESCAPED,
};
use std::path::PathBuf;

fn repo(rel: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

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

/// #4265's probe: all seven reached an fs error text raw on both legs.
#[test]
fn line_separators_invisible_characters_and_tags_are_escaped() {
    assert_eq!(
        esc("a\u{2028}b\u{2029}c\u{200b}d\u{2060}e\u{feff}f\u{e0041}\u{e0042}g"),
        "a\\u{2028}b\\u{2029}c\\u{200b}d\\u{2060}e\\u{feff}f\\u{e0041}\\u{e0042}g"
    );
    assert_eq!(esc("\u{200c}\u{200d}\u{2064}\u{ad}\u{180e}"), "\\u{200c}\\u{200d}\\u{2064}\\u{ad}\\u{180e}");
    assert_eq!(esc("\u{e0000}\u{e0001}\u{e007f}"), "\\u{e0000}\\u{e0001}\\u{e007f}");
}

#[test]
fn keeps_every_other_character() {
    let plain = "C:\\dir\\file \"q\".txt \u{a0}\u{200a}\u{2065}\u{2070}\u{301}\u{e0080}日本 🎉";
    assert_eq!(esc(plain), plain);
}

#[test]
fn the_table_is_sorted_disjoint_and_is_the_predicate() {
    for w in ALMIDE_RT_ERRTEXT_ESCAPED.windows(2) {
        assert!(w[0].0 <= w[0].1 && w[0].1 + 1 < w[1].0, "rows out of order, overlapping or adjacent: {w:?}");
    }
    let listed = |c: u32| ALMIDE_RT_ERRTEXT_ESCAPED.iter().any(|&(lo, hi)| lo <= c && c <= hi);
    for c in (0..=0x10ffffu32).filter_map(char::from_u32) {
        assert_eq!(almide_rt_errtext_is_escaped(c), listed(c as u32), "U+{:04X}", c as u32);
        if listed(c as u32) && !matches!(c, '\n' | '\r' | '\t' | '\0') {
            assert_eq!(esc(&c.to_string()), format!("\\u{{{:x}}}", c as u32));
        }
    }
}

/// `(lo, hi)` pairs from the WAT's `$esc_cp`: each line
/// `(i32.sub (local.get $cp) (i32.const LO)) (i32.const HI - LO)`.
fn wat_ranges(src: &str) -> Vec<(u32, u32)> {
    let start = src.find("(func $esc_cp").expect("fs_service.wat has no $esc_cp");
    let end = start + src[start..].find("(i32.const 1))").expect("$esc_cp has no end");
    let hex = |t: &str| u32::from_str_radix(t.trim_start_matches("0x"), 16).expect("hex");
    src[start..end]
        .lines()
        .filter_map(|l| {
            let a = l.split("(i32.sub (local.get $cp) (i32.const ").nth(1)?;
            let (lo, rest) = a.split_once(')')?;
            let span = rest.split("(i32.const ").nth(1)?.split(')').next()?;
            Some((hex(lo), hex(lo) + hex(span)))
        })
        .collect()
}

/// `(lo, hi)` pairs from the self-hosted `__et_escaped`: each
/// `__et_in(n, LO, HI)`, however the formatter wraps it.
fn almd_ranges(src: &str) -> Vec<(u32, u32)> {
    let start = src.find("fn __et_escaped").expect("string_err_text.almd has no __et_escaped");
    let end = start + src[start..].find("\n\n").expect("__et_escaped has no end");
    src[start..end]
        .split("__et_in(")
        .skip(1)
        .map(|t| {
            // `n, LO, HI` — on one line or, as `almide fmt` wraps a long
            // chain, one argument per line with a trailing comma.
            let args: Vec<&str> = t.split(')').next().unwrap().split(',').map(str::trim).filter(|x| !x.is_empty()).collect();
            assert_eq!(args.first(), Some(&"n"), "__et_in's first argument is the codepoint: {args:?}");
            (args[1].parse().expect("decimal"), args[2].parse().expect("decimal"))
        })
        .collect()
}

fn same_as_table(leg: &str, got: &[(u32, u32)]) -> Result<(), String> {
    if got == ALMIDE_RT_ERRTEXT_ESCAPED {
        Ok(())
    } else {
        Err(format!("{leg}: its ranges differ from ALMIDE_RT_ERRTEXT_ESCAPED\n  leg:   {got:x?}\n  table: {ALMIDE_RT_ERRTEXT_ESCAPED:x?}"))
    }
}

#[test]
fn the_legs_that_carry_the_set_carry_exactly_the_table() {
    same_as_table("fs_service.wat $esc_cp", &wat_ranges(&repo("crates/almide-wasi/src/fs_service.wat"))).unwrap();
    same_as_table("string_err_text.almd __et_escaped", &almd_ranges(&repo("stdlib/string_err_text.almd"))).unwrap();
}

/// The control: a copy missing a row, or with one row's end moved, is caught;
/// the readers parse what they are given, not a constant.
#[test]
fn control_a_copy_that_drifts_is_caught() {
    let wat = repo("crates/almide-wasi/src/fs_service.wat");
    let dropped = wat.replacen("      (br_if $yes (i32.le_u (i32.sub (local.get $cp) (i32.const 0x2028)) (i32.const 0x6)))\n", "", 1);
    assert_ne!(dropped, wat, "the control's edit did not apply");
    assert!(same_as_table("wat", &wat_ranges(&dropped)).is_err());
    let almd = repo("stdlib/string_err_text.almd");
    let moved = almd.replacen("__et_in(n, 917504, 917631)", "__et_in(n, 917504, 917630)", 1);
    assert_ne!(moved, almd, "the control's edit did not apply");
    assert!(same_as_table("almd", &almd_ranges(&moved)).is_err());
}
