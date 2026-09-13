//! E062 — a shebang that runs on macOS and dies on Linux (#2159).
//!
//! `#!/usr/bin/env almide run` works on Darwin, whose kernel splits the
//! shebang line on whitespace before handing it to `env`. Linux's
//! `binfmt_script` does not split: `env` receives the rest of the line as ONE
//! argument and looks for a program literally named `almide run`. `env -S`
//! (coreutils 8.30, 2018) exists for exactly this split. The failure never
//! reproduces where the script was written, and the wrong spelling is the one
//! a writer reaches for — every shebang they have seen is one word.
//!
//! The condition is decided by the text alone: no PATH lookup, no host probe.

use almide_base::Diagnostic;

/// The warning for a shebang that calls `env` with more than one word after
/// it and no `-S` / `--split-string`. `None` when the file has no shebang,
/// does not go through `env`, already splits, or names a single program.
pub fn split_string_warning(file: &str, source: &str) -> Option<Diagnostic> {
    let first = source.strip_prefix('\u{feff}').unwrap_or(source).lines().next()?;
    let rest = first.strip_prefix("#!")?;
    let words: Vec<&str> = rest.split_whitespace().collect();
    let (interp, args) = words.split_first()?;
    let is_env = interp.rsplit('/').next() == Some("env");
    if !is_env {
        return None;
    }
    // `-S`, `--split-string`, or a flag cluster carrying `S` (`-iS`) all split.
    let splits = args.iter().any(|a| {
        *a == "--split-string" || a.starts_with("--split-string=")
            || (a.starts_with('-') && !a.starts_with("--") && a.contains('S'))
    });
    if splits {
        return None;
    }
    // What `env` would have to find: everything that is not a flag or an
    // `NAME=value` assignment. One word is fine on both kernels.
    let program_words = args.iter().filter(|a| !a.starts_with('-') && !a.contains('=')).count();
    if program_words < 2 {
        return None;
    }
    let fixed = fixed_line(interp, args);
    Some(
        Diagnostic::warning(
            "this shebang does not run on Linux",
            "Linux passes the rest of the line to `env` as ONE argument, so it looks for a \
             program literally named `".to_string()
                + &args.iter().filter(|a| !a.starts_with('-') && !a.contains('=')).cloned().collect::<Vec<_>>().join(" ")
                + "`. macOS splits it, which is why this works here. Use `-S` to split portably.",
            "",
        )
        .with_code("E062")
        .with_here(first)
        // Inserting `-S` is a pure re-spelling of the same command line —
        // `almide fix` may apply it unattended. Columns are 1-indexed
        // characters, end exclusive: the whole of line 1.
        .with_machine_fix(1, 1, first.chars().count() + 1, fixed)
        .at(file, 1),
    )
}

/// The same line with `-S` inserted right after `env`.
fn fixed_line(interp: &str, args: &[&str]) -> String {
    let mut out = format!("#!{interp} -S");
    for a in args {
        out.push(' ');
        out.push_str(a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::split_string_warning as warn;

    fn src(line: &str) -> String {
        format!("{line}\nfn main() -> Unit = println(\"x\")\n")
    }

    #[test]
    fn env_with_two_words_and_no_split_warns() {
        let d = warn("s.almd", &src("#!/usr/bin/env almide run")).expect("warns");
        assert_eq!(d.code, Some("E062"));
        assert_eq!(d.line, Some(1));
        assert_eq!(d.try_snippet.as_deref(), Some("#!/usr/bin/env -S almide run"));
        assert_eq!(d.try_replace_span, Some((1, 1, 26)), "the fix replaces the whole of line 1");
        assert!(d.try_applicability.is_machine_applicable(), "inserting -S is a pure re-spelling");
        assert!(d.hint.contains("`almide run`"), "hint names what Linux looks for: {}", d.hint);
    }

    #[test]
    fn split_forms_are_quiet() {
        for line in [
            "#!/usr/bin/env -S almide run",
            "#!/usr/bin/env --split-string almide run",
            "#!/usr/bin/env -iS almide run",
            "#!/usr/bin/env -S almide run --target wasm",
        ] {
            assert!(warn("s.almd", &src(line)).is_none(), "{line} must not warn");
        }
    }

    #[test]
    fn single_program_and_non_env_are_quiet() {
        for line in [
            "#!/usr/bin/env almide",
            "#!/usr/bin/env FOO=1 almide",
            "#!/usr/local/bin/almide run",
            "#!/bin/sh",
        ] {
            assert!(warn("s.almd", &src(line)).is_none(), "{line} must not warn");
        }
    }

    #[test]
    fn no_shebang_or_shebang_not_on_line_one_is_quiet() {
        assert!(warn("s.almd", "fn main() -> Unit = ()\n").is_none());
        assert!(warn("s.almd", "\n#!/usr/bin/env almide run\n").is_none());
    }

    #[test]
    fn a_bom_before_the_shebang_still_counts() {
        assert!(warn("s.almd", &format!("\u{feff}{}", src("#!/usr/bin/env almide run"))).is_some());
    }
}
