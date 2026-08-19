// Golden battery — ONE body compiled against BOTH sides of the port:
//   incumbent: almide@a877d2138 (almide-base + src/diagnostic_render.rs)
//   greenfield: the almide-diag crate
// The parent module provides, via `use super::env::*`:
//   Diagnostic, Level, Applicability, SecondarySpan, Span,
//   levenshtein, suggest, display, to_json, display_with_source
// The committed golden (diag-golden.txt) is the INCUMBENT's byte-exact output
// of `golden_output()`; the greenfield test replays the same body and must
// match byte-for-byte. Incumbent quirks are reproduced on purpose (e.g.
// `to_json` escapes backslashes in `suggestions[].replacement` but not in
// `message`); fixing a quirk is a later, contract-visible change — never a
// silent porting edit.

fn case(out: &mut String, name: &str, d: &Diagnostic) {
    out.push_str(&format!("\n--- {} ---\n", name));
    out.push_str("[Diagnostic::display]\n");
    out.push_str(&d.display());
    out.push_str("\n[render::display]\n");
    out.push_str(&display(d));
    out.push_str("\n[render::to_json]\n");
    out.push_str(&to_json(d));
    out.push('\n');
}

fn case_with_source(out: &mut String, name: &str, d: &Diagnostic, src: &str) {
    case(out, name, d);
    out.push_str("[render::display_with_source]\n");
    out.push_str(&display_with_source(d, src));
    out.push('\n');
}

pub fn golden_output() -> String {
    let mut out = String::new();

    // ── plain shapes ────────────────────────────────────────────────────
    case(&mut out, "error-bare", &Diagnostic::error("type mismatch", "expected Int, found String", "let x = f(s)"));
    case(&mut out, "warning-bare-empty-hint-context", &Diagnostic::warning("unused variable `n`", "", ""));
    case(&mut out, "coded-file-line", &Diagnostic::error("unknown function 'prase_int'", "did you mean 'parse_int'?", "prase_int(s)").with_code("E014").at("app.almd", 3));
    case(&mut out, "file-no-line", &{
        let mut d = Diagnostic::error("module failed to load", "check the import path", "");
        d.file = Some("lib/util.almd".to_string());
        d
    });
    case(&mut out, "line-no-file", &{
        let mut d = Diagnostic::warning("shadowed binding", "rename one of them", "let x = 2");
        d.line = Some(7);
        d
    });

    // ── spans ───────────────────────────────────────────────────────────
    let src3 = "fn main() -> Int =\n  let total = cout + 1\n  total\n";
    case_with_source(&mut out, "at_span-caret-range", &Diagnostic::error("unknown variable 'cout'", "did you mean 'count'?", "let total = cout + 1").with_code("E003").at_span("app.almd", Span { line: 2, col: 15, end_col: 19 }), src3);
    case_with_source(&mut out, "at_span-degenerate-end", &Diagnostic::error("stray token", "remove it", "").at_span("app.almd", Span { line: 2, col: 15, end_col: 15 }), src3);

    // ── here / try ──────────────────────────────────────────────────────
    case(&mut out, "with_here-multiline-collapses", &Diagnostic::error("bad call", "check the arity", "f(1, 2, 3)").with_here("\n   \n  f(1, 2, 3)\n  g(4)\n"));
    case(&mut out, "with_try-display-only", &Diagnostic::error("missing import", "json is not auto-imported", "json.parse(s)").with_code("E020").with_try("// add at the top of the file\nimport json"));

    // ── fix-its: the #1312 applicability matrix ─────────────────────────
    let machine = Diagnostic::error("`!` is not a prefix operator", "use `not`", "if !user_admin then x").with_code("E007").at("app.almd", 1).with_machine_fix(1, 4, 5, "not ");
    case(&mut out, "machine-fix", &machine);
    case(&mut out, "machine-fix-deletion", &Diagnostic::error("duplicate keyword", "delete it", "let let x = 1").with_machine_fix(1, 5, 9, ""));
    case(&mut out, "suggested-fix", &Diagnostic::error("unknown name 'lenght'", "did you mean 'length'?", "lenght(xs)").with_suggested_fix(1, 1, 7, "length"));
    case(&mut out, "guessed-span-line0", &Diagnostic::error("e", "h", "c").with_machine_fix(0, 3, 5, "boom"));
    case(&mut out, "guessed-span-col0", &Diagnostic::error("e", "h", "c").with_machine_fix(2, 0, 5, "boom"));
    case(&mut out, "guessed-span-inverted", &Diagnostic::error("e", "h", "c").with_machine_fix(2, 9, 4, "boom"));

    // ── secondary spans ─────────────────────────────────────────────────
    let src_sec = "fn f(n: Int) -> Int =\n  n + 1\n\n\n\n\n\n\n\nfn g() -> Int =\n  f(\"x\")\n";
    case_with_source(&mut out, "secondary-distant-ellipsis", &Diagnostic::error("argument type mismatch", "f expects Int", "f(\"x\")").with_code("E002").at_span("app.almd", Span { line: 11, col: 3, end_col: 9 }).with_secondary(1, Some(6), "declared as Int here"), src_sec);
    case_with_source(&mut out, "secondary-no-col", &Diagnostic::error("cycle detected", "break the cycle", "").at("app.almd", 2).with_secondary(1, None, "first edge here"), "a -> b\nb -> a\n");
    case_with_source(&mut out, "secondary-empty-label-and-quote", &Diagnostic::error("shadow", "rename", "let x = 2").at_span("app.almd", Span { line: 2, col: 5, end_col: 6 }).with_secondary(1, Some(5), "first \"x\" here").with_secondary(2, Some(5), ""), "let x = 1\nlet x = 2\n");

    // ── JSON escaping quirks (reproduced verbatim) ──────────────────────
    case(&mut out, "json-escaping", &Diagnostic::error("bad \"literal\"\nsecond line", "write \\n as a two-char escape", "s = \"a\\b\"").with_code("E047").at("esc.almd", 4).with_machine_fix(4, 5, 11, "\"a\\\\b\"\nrest"));

    // ── unicode ─────────────────────────────────────────────────────────
    let src_jp = "let 挨拶 = \"こんにちは\"\nprint(挨拶s)\n";
    case_with_source(&mut out, "unicode-caret-cols", &Diagnostic::error("unknown variable '挨拶s'", "did you mean '挨拶'?", "print(挨拶s)").at_span("jp.almd", Span { line: 2, col: 7, end_col: 10 }), src_jp);

    // ── apply_try_to: the fix engine ────────────────────────────────────
    out.push_str("\n--- apply_try_to ---\n");
    let ap = |d: &Diagnostic, s: &str| format!("{:?}", d.apply_try_to(s));
    out.push_str(&format!("mid-line:      {}\n", ap(&machine, "if !user_admin then x")));
    out.push_str(&format!("whole-token:   {}\n", ap(&Diagnostic::error("e", "h", "c").with_machine_fix(1, 7, 15, "int.parse"), "let x=parseInt(s)")));
    out.push_str(&format!("second-line:   {}\n", ap(&Diagnostic::error("e", "h", "c").with_machine_fix(2, 5, 13, "int.parse"), "fn main() -> Int =\n    parseInt(s)\n")));
    out.push_str(&format!("zero-width:    {}\n", ap(&Diagnostic::error("e", "h", "c").with_machine_fix(1, 1, 1, "import json\n"), "effect fn main() = ...")));
    out.push_str(&format!("eol-insert:    {}\n", ap(&Diagnostic::error("e", "h", "c").with_machine_fix(1, 6, 6, "!"), "boom\nx = 1")));
    out.push_str(&format!("unicode-line:  {}\n", ap(&Diagnostic::error("e", "h", "c").with_machine_fix(2, 7, 10, "挨拶"), src_jp)));
    out.push_str(&format!("oob-line:      {}\n", ap(&Diagnostic::error("e", "h", "c").with_machine_fix(5, 1, 2, "x"), "only one line")));
    out.push_str(&format!("oob-col:       {}\n", ap(&Diagnostic::error("e", "h", "c").with_machine_fix(1, 100, 110, "x"), "short")));
    out.push_str(&format!("suggested-not-applied-by-machine_fix: {:?}\n", Diagnostic::error("e", "h", "c").with_suggested_fix(1, 1, 2, "y").machine_fix()));
    out.push_str(&format!("machine-read-path: {:?}\n", machine.machine_fix()));

    // ── suggest / levenshtein ───────────────────────────────────────────
    out.push_str("\n--- suggest ---\n");
    let names = ["parse_int", "parse_float", "to_string", "codepoint", "length"];
    for probe in ["prase_int", "to_code_points", "lenght", "zzzzz", "parse_int"] {
        out.push_str(&format!("suggest({:?}) = {:?}\n", probe, suggest(probe, names.iter().copied())));
    }
    for (a, b) in [("kitten", "sitting"), ("Almide", "almide"), ("", "abc"), ("同じ", "同じ")] {
        out.push_str(&format!("levenshtein({:?}, {:?}) = {}\n", a, b, levenshtein(a, b)));
    }

    // ── applicability wire spellings + labels ───────────────────────────
    out.push_str("\n--- applicability ---\n");
    for a in [Applicability::MachineApplicable, Applicability::MaybeIncorrect, Applicability::HasPlaceholders, Applicability::Unspecified] {
        out.push_str(&format!("{:?} = {} machine={}\n", a, a.as_str(), a.is_machine_applicable()));
    }
    out.push_str(&format!("label-machine:  {:?}\n", machine.try_label_suffix()));
    out.push_str(&format!("label-deletion: {:?}\n", Diagnostic::error("e", "h", "c").with_machine_fix(1, 1, 2, "").try_label_suffix()));
    out.push_str(&format!("label-none:     {:?}\n", Diagnostic::error("e", "h", "c").with_try("x").try_label_suffix()));

    out
}
