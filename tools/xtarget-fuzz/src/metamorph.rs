//! Metamorphic binding-shape variants (#515, completeness §3).
//!
//! THE language rule under test: `let x = e` is accepted ⟺ `var x = e` is
//! accepted ⟺ `var x = d; x = e` is accepted (mod mutability), and the
//! effect-Result unwrap/coercion applied to `e` is IDENTICAL in every
//! binding position. The checker and the lowering each implement this rule
//! once; fixture C-064 pins it pointwise. This module pins it
//! METAMORPHICALLY: every clean synthesized program is replayed with its
//! binding shapes rewritten, and any acceptance or behavior delta is a
//! finding.
//!
//! Scope guard: only generator-SHAPED lines are rewritten (`let rN: T =
//! e;` single-line, bracket/quote balanced), and the rung only runs on
//! `Origin::Synthesis` programs — corpus mutations can contain arbitrary
//! text the line heuristic must not touch.

/// True iff the line is a complete one-line `let` statement we can rewrite:
/// starts with `let `, ends with `;`, and all brackets/quotes are balanced.
fn is_rewritable_let(trimmed: &str) -> bool {
    if !trimmed.starts_with("let ") || !trimmed.ends_with(';') {
        return false;
    }
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for c in trimmed.chars() {
        if in_str {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    depth == 0 && !in_str
}

/// Variant A: every rewritable `let` becomes `var` — mutability of the
/// binding must not change what the binding position accepts or computes.
fn let_to_var(source: &str) -> Option<String> {
    let mut changed = false;
    let out: Vec<String> = source
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if is_rewritable_let(trimmed) {
                changed = true;
                let indent = &line[..line.len() - trimmed.len()];
                format!("{indent}var {}", &trimmed[4..])
            } else {
                line.to_string()
            }
        })
        .collect();
    changed.then(|| out.join("\n"))
}

/// Variant B: every rewritable `let x: T = e;` becomes
/// `var x: T = e;\n x = e;` — the ASSIGN position must accept exactly what
/// the bind position accepted, with identical unwrap/coercion. Generated
/// synthesis expressions are pure (the denylist excludes nondeterminism),
/// so the re-evaluation is observably equivalent.
fn bind_then_assign(source: &str) -> Option<String> {
    let mut changed = false;
    let out: Vec<String> = source
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if is_rewritable_let(trimmed) {
                if let Some(eq) = trimmed.find(" = ") {
                    let header = &trimmed[4..eq]; // `name: Ty`
                    let name = header.split(':').next().unwrap_or(header).trim();
                    let expr = &trimmed[eq + 3..trimmed.len() - 1]; // sans `;`
                    changed = true;
                    let indent = &line[..line.len() - trimmed.len()];
                    return format!(
                        "{indent}var {};\n{indent}{name} = {expr};",
                        &trimmed[4..trimmed.len() - 1]
                    );
                }
            }
            line.to_string()
        })
        .collect();
    changed.then(|| out.join("\n"))
}

/// All binding-shape variants of a synthesized program, labeled for the
/// finding summary.
pub fn binding_variants(source: &str) -> Vec<(&'static str, String)> {
    let mut v = Vec::new();
    if let Some(s) = let_to_var(source) {
        v.push(("let→var", s));
    }
    if let Some(s) = bind_then_assign(source) {
        v.push(("bind→assign", s));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROG: &str = "fn main() -> Unit = {\n  let x5: Int = int.abs(-3);\n  let y6: String = \"a;b\" + \"c\";\n  println(\"${x5}${y6}\")\n}";

    #[test]
    fn variants_generated() {
        let vs = binding_variants(PROG);
        assert_eq!(vs.len(), 2, "both variants must fire on bind-bearing programs");
        let (label_a, a) = &vs[0];
        assert_eq!(*label_a, "let→var");
        assert!(a.contains("var x5: Int = int.abs(-3);"));
        assert!(a.contains("var y6: String = \"a;b\" + \"c\";"));
        let (label_b, b) = &vs[1];
        assert_eq!(*label_b, "bind→assign");
        assert!(b.contains("var x5: Int = int.abs(-3);\n  x5 = int.abs(-3);"));
    }

    #[test]
    fn multiline_and_strings_untouched() {
        // unbalanced (multi-line) lets and let-text inside strings are skipped
        let src = "  let a: Int = f(\n    1);\n  println(\"let r1: Int = 2;\")";
        assert!(binding_variants(src).is_empty());
    }
}
