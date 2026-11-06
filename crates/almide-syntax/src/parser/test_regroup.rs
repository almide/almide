//! #4531: which `??` sites the parser names as regrouped at dialect 16, and
//! the migration's paren insertion over a pre-16 tree.

#[cfg(test)]
mod tests {
    use crate::lexer::Lexer;
    use crate::parser::regroup::{grouping_value, parenthesize_for_current, parse_legacy};
    use crate::parser::Parser;

    fn regrouped(src: &str) -> Vec<(usize, usize)> {
        let mut p = Parser::new(Lexer::tokenize(src));
        let program = p.parse().expect("parses");
        assert!(p.errors.is_empty(), "{src}: {:?}", p.errors);
        program.regrouped.iter().map(|s| (s.line, s.col)).collect()
    }

    #[test]
    fn a_moved_coalesce_is_named_at_its_operator() {
        assert_eq!(regrouped("fn f(x: Int?) -> Int = x ?? 0 - 1\n"), vec![(1, 26)]);
        assert_eq!(regrouped("fn f(s: Int, b: Int?) -> Int = s + b ?? 0\n"), vec![(1, 38)]);
        assert_eq!(regrouped("fn f(x: Int?) -> Int = -x ?? 0\n"), vec![(1, 27)]);
        assert_eq!(regrouped("fn f(x: Int?) -> String = \"${x ?? 0 + 1}\"\n").len(), 1);
        assert_eq!(
            regrouped("fn f(xs: List[Int]) -> Int = xs |> list.first ?? 0 |> (n) => n + 1\n").len(),
            1
        );
    }

    #[test]
    fn an_unmoved_coalesce_is_not_named() {
        for src in [
            "fn f(x: Int?) -> Int = x ?? 0\n",
            "fn f(x: Int?) -> Int = (x ?? 0) - 1\n",
            "fn f(x: Int?) -> Int = x ?? (0 - 1)\n",
            "fn f(x: Int?, y: Int) -> Bool = x ?? 0 == y\n",
            "fn f(x: Int?, y: Int?) -> Int = x ?? y ?? 0\n",
            // ADR-0005 D6: the pre-16 checker re-associated this to the pipe's fallback
            "fn f(xs: List[Int]) -> Int = xs |> list.first ?? 0\n",
            "fn f(xs: List[Int]) -> Int = xs |> list.filter((x) => x > 1) |> list.first ?? -4\n",
            "fn f(xs: List[Int]) -> Int = xs |> list.first ?? {\n  -8\n}\n",
        ] {
            assert!(regrouped(src).is_empty(), "{src}");
        }
    }

    /// Only the innermost moved `??` of a nest is named.
    #[test]
    fn a_nest_names_its_innermost_site() {
        assert_eq!(regrouped("fn f(a: Int?, s: Int, b: Int?) -> Int = a ?? (s + b ?? 0)\n").len(), 1);
    }

    #[test]
    fn a_stamp_at_sixteen_takes_the_new_reading() {
        assert!(regrouped("@dialect(16)\n\nfn f(x: Int?) -> Int = x ?? 0 - 1\n").is_empty());
        assert_eq!(regrouped("@dialect(15)\n\nfn f(x: Int?) -> Int = x ?? 0 - 1\n").len(), 1);
    }

    /// The migration's core adds parentheses to every moved site and nothing
    /// else: the structure under them is the pre-16 tree. That the CURRENT
    /// grammar reads the printed result back as that tree is asserted where
    /// the printer lives (almide-tools `regroup`, and the corpus test).
    #[test]
    fn parenthesizing_a_legacy_tree_keeps_its_grouping() {
        for body in [
            "x ?? 0 - 1",
            "s + x ?? 0",
            "-x ?? 0",
            "not p ?? false",
            "x ?? 2 ^ s",
            "s * x ?? 1 + s",
            "a ?? s + x ?? 0",
            "xs |> list.first ?? 0 |> g",
            "xs |> f >> g ?? d",
            "f(x ?? 0 - 1, s - x ?? 2)",
            "x ?? 0 ..< s",
        ] {
            let src = format!("fn f() -> Int = {body}\n");
            let (mut legacy, fresh) = parse_legacy(&src).expect("legacy parses");
            let reference = grouping_value(&legacy);
            assert!(parenthesize_for_current(&mut legacy, fresh) > 0, "{body}: nothing added");
            assert_eq!(grouping_value(&legacy), reference, "{body}: parens changed the structure");
            assert!(!regrouped(&src).is_empty(), "{body}: not named as regrouped");
        }
    }
}
