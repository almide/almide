//! Expression precedence tests — scenario matrix from pipe-operator-precedence.md
//!
//! Tests the parse tree shape (not evaluation). Each test parses a source fragment
//! and checks the resulting ExprKind nesting using a compact string representation.

#[cfg(test)]
mod tests {
    use crate::lexer::Lexer;
    use crate::ast::ExprKind;
    use super::super::Parser;

    /// Parse a single expression and return its ExprKind tree as a compact string.
    /// Format: `(op left right)` for binary, `(|> left right)` for pipe, etc.
    fn parse_shape(src: &str) -> String {
        let tokens = Lexer::tokenize(src);
        let mut parser = Parser::new(tokens);
        match parser.parse_expr() {
            Ok(expr) => shape(&expr.kind),
            Err(e) => format!("ERR: {}", e),
        }
    }

    fn shape(kind: &ExprKind) -> String {
        match kind {
            ExprKind::Binary { op, left, right } => {
                format!("({} {} {})", op, shape(&left.kind), shape(&right.kind))
            }
            ExprKind::Pipe { left, right } => {
                format!("(|> {} {})", shape(&left.kind), shape(&right.kind))
            }
            ExprKind::Compose { left, right } => {
                format!("(>> {} {})", shape(&left.kind), shape(&right.kind))
            }
            ExprKind::Range { start, end, inclusive } => {
                let op = if *inclusive { "..=" } else { ".." };
                format!("({} {} {})", op, shape(&start.kind), shape(&end.kind))
            }
            ExprKind::Unary { op, operand } => {
                format!("(unary:{} {})", op, shape(&operand.kind))
            }
            ExprKind::Call { callee, args, .. } => {
                let callee_s = shape(&callee.kind);
                if args.is_empty() {
                    format!("{}()", callee_s)
                } else {
                    let args_s: Vec<String> = args.iter().map(|a| shape(&a.kind)).collect();
                    format!("{}({})", callee_s, args_s.join(", "))
                }
            }
            ExprKind::Member { object, field } => {
                format!("{}.{}", shape(&object.kind), field)
            }
            ExprKind::Ident { name } => name.to_string(),
            ExprKind::Int { raw, .. } => raw.clone(),
            ExprKind::String { value } => format!("\"{}\"", value),
            ExprKind::Bool { value } => value.to_string(),
            ExprKind::Lambda { params, body } => {
                let ps: Vec<String> = params.iter().map(|p| p.name.to_string()).collect();
                format!("(\\({}) {})", ps.join(", "), shape(&body.kind))
            }
            ExprKind::Match { subject, arms } => {
                format!("(match {} [{} arms])", shape(&subject.kind), arms.len())
            }
            ExprKind::Unwrap { expr } => format!("(! {})", shape(&expr.kind)),
            ExprKind::Try { expr } => format!("(? {})", shape(&expr.kind)),
            ExprKind::UnwrapOr { expr, fallback } => {
                format!("(?? {} {})", shape(&expr.kind), shape(&fallback.kind))
            }
            ExprKind::Paren { expr } => shape(&expr.kind),
            ExprKind::List { elements } => {
                let es: Vec<String> = elements.iter().map(|e| shape(&e.kind)).collect();
                format!("[{}]", es.join(", "))
            }
            _ => format!("{:?}", std::mem::discriminant(kind)),
        }
    }

    // ═══════════════════════════════════════════════════════
    //  A: Pipe × Arithmetic
    // ═══════════════════════════════════════════════════════

    #[test]
    fn a1_basic_pipe() {
        assert_eq!(parse_shape("xs |> list.map(f)"), "(|> xs list.map(f))");
    }

    #[test]
    fn a2_pipe_then_add() {
        // THE BLOCKER: pipe result + ys
        assert_eq!(parse_shape("xs |> list.map(f) + ys"), "(+ (|> xs list.map(f)) ys)");
    }

    #[test]
    fn a3_add_then_pipe() {
        assert_eq!(parse_shape("a + b |> f"), "(|> (+ a b) f)");
    }

    #[test]
    fn a4_pipe_then_add_chain() {
        assert_eq!(parse_shape("xs |> f + a + b"), "(+ (+ (|> xs f) a) b)");
    }

    #[test]
    fn a5_add_chain_then_pipe() {
        assert_eq!(parse_shape("a + b + c |> f"), "(|> (+ (+ a b) c) f)");
    }

    #[test]
    fn a6_pipe_then_sub() {
        assert_eq!(parse_shape("xs |> f - 1"), "(- (|> xs f) 1)");
    }

    #[test]
    fn a7_pipe_then_mul() {
        assert_eq!(parse_shape("xs |> list.len * 2"), "(* (|> xs list.len) 2)");
    }

    #[test]
    fn a8_pipe_then_mul_add() {
        assert_eq!(
            parse_shape("xs |> list.len * 2 + 1"),
            "(+ (* (|> xs list.len) 2) 1)"
        );
    }

    // ═══════════════════════════════════════════════════════
    //  R: Pipe × Range
    // ═══════════════════════════════════════════════════════

    #[test]
    fn r1_range_then_pipe() {
        assert_eq!(
            parse_shape("0..n |> list.map(f)"),
            "(|> (.. 0 n) list.map(f))"
        );
    }

    #[test]
    fn r2_range_add_then_pipe() {
        assert_eq!(
            parse_shape("0..n+1 |> list.map(f)"),
            "(|> (.. 0 (+ n 1)) list.map(f))"
        );
    }

    #[test]
    fn r3_range_with_add() {
        assert_eq!(parse_shape("0..n + 1"), "(.. 0 (+ n 1))");
    }

    #[test]
    fn r4_range_both_add() {
        assert_eq!(parse_shape("a+1..b+2"), "(.. (+ a 1) (+ b 2))");
    }

    #[test]
    fn r5_range_pipe_add() {
        assert_eq!(
            parse_shape("0..n |> list.map(f) + ys"),
            "(+ (|> (.. 0 n) list.map(f)) ys)"
        );
    }

    #[test]
    fn r6_range_pipe_chain() {
        assert_eq!(
            parse_shape("0..10 |> list.filter(p) |> list.map(f)"),
            "(|> (|> (.. 0 10) list.filter(p)) list.map(f))"
        );
    }

    // ═══════════════════════════════════════════════════════
    //  C: Pipe Chains
    // ═══════════════════════════════════════════════════════

    #[test]
    fn c1_pipe_chain_2() {
        assert_eq!(parse_shape("xs |> f |> g"), "(|> (|> xs f) g)");
    }

    #[test]
    fn c2_pipe_chain_3() {
        assert_eq!(parse_shape("xs |> f |> g |> h"), "(|> (|> (|> xs f) g) h)");
    }

    #[test]
    fn c3_pipe_chain_then_add() {
        assert_eq!(parse_shape("xs |> f |> g + ys"), "(+ (|> (|> xs f) g) ys)");
    }

    #[test]
    fn c4_pipe_add_pipe() {
        assert_eq!(
            parse_shape("xs |> f + ys |> g"),
            "(|> (+ (|> xs f) ys) g)"
        );
    }

    // ═══════════════════════════════════════════════════════
    //  L: Pipe × Comparison / Logical
    // ═══════════════════════════════════════════════════════

    #[test]
    fn l1_pipe_then_compare() {
        assert_eq!(parse_shape("xs |> list.len > 5"), "(> (|> xs list.len) 5)");
    }

    #[test]
    fn l2_pipe_both_sides_compare() {
        assert_eq!(
            parse_shape("xs |> list.len == ys |> list.len"),
            "(== (|> xs list.len) (|> ys list.len))"
        );
    }

    #[test]
    fn l3_pipe_and() {
        assert_eq!(
            parse_shape("xs |> list.any(p) and ys |> list.all(q)"),
            "(and (|> xs list.any(p)) (|> ys list.all(q)))"
        );
    }

    #[test]
    fn l4_pipe_compare_and() {
        assert_eq!(
            parse_shape("xs |> list.len > 0 and ys |> list.len > 0"),
            "(and (> (|> xs list.len) 0) (> (|> ys list.len) 0))"
        );
    }

    #[test]
    fn l5_pipe_add_compare() {
        assert_eq!(
            parse_shape("xs |> list.len + 1 > 5"),
            "(> (+ (|> xs list.len) 1) 5)"
        );
    }

    // ═══════════════════════════════════════════════════════
    //  M: Pipe × Match
    // ═══════════════════════════════════════════════════════

    #[test]
    fn m1_pipe_match() {
        assert_eq!(
            parse_shape("x |> match { 1 => true, _ => false }"),
            "(match x [2 arms])"
        );
    }

    #[test]
    fn m2_pipe_chain_match() {
        assert_eq!(
            parse_shape("xs |> list.first() |> match { 1 => true, _ => false }"),
            "(match (|> xs list.first()) [2 arms])"
        );
    }

    // ═══════════════════════════════════════════════════════
    //  F: Compose >>
    // ═══════════════════════════════════════════════════════

    #[test]
    fn f1_compose_basic() {
        assert_eq!(parse_shape("f >> g"), "(>> f g)");
    }

    #[test]
    fn f2_compose_chain() {
        assert_eq!(parse_shape("f >> g >> h"), "(>> (>> f g) h)");
    }

    #[test]
    fn f3_pipe_into_compose() {
        assert_eq!(parse_shape("xs |> f >> g"), "(|> xs (>> f g))");
    }

    #[test]
    fn f4_compose_then_add() {
        assert_eq!(parse_shape("f >> g + h"), "(+ (>> f g) h)");
    }

    // ═══════════════════════════════════════════════════════
    //  B: Real-world patterns (almide-bindgen)
    // ═══════════════════════════════════════════════════════

    #[test]
    fn b1_flat_map_plus_map() {
        assert_eq!(
            parse_shape("types |> list.flat_map(f) + (fns |> list.map(g))"),
            "(+ (|> types list.flat_map(f)) (|> fns list.map(g)))"
        );
    }

    #[test]
    fn b2_add_paren_pipe() {
        // Parens already make this unambiguous — must not break
        assert_eq!(
            parse_shape("header + (xs |> list.join(sep))"),
            "(+ header (|> xs list.join(sep)))"
        );
    }

    #[test]
    fn b6_pipe_chain_join() {
        assert_eq!(
            parse_shape("xs |> list.map(f) |> list.join(sep)"),
            "(|> (|> xs list.map(f)) list.join(sep))"
        );
    }

    // ═══════════════════════════════════════════════════════
    //  U: Unary / Postfix × Pipe
    // ═══════════════════════════════════════════════════════

    #[test]
    fn u4_unary_minus_pipe() {
        assert_eq!(parse_shape("-xs |> f"), "(|> (unary:- xs) f)");
    }

    // ═══════════════════════════════════════════════════════
    //  E: Edge cases
    // ═══════════════════════════════════════════════════════

    #[test]
    fn e1_lambda_body_add() {
        // + inside lambda body is NOT affected by pipe
        assert_eq!(
            parse_shape("xs |> list.map((x) => x + 1)"),
            "(|> xs list.map((\\(x) (+ x 1))))"
        );
    }

    #[test]
    fn e5_plusplus_after_pipe() {
        assert_eq!(parse_shape("xs |> list.map(f) ++ ys"), "(++ (|> xs list.map(f)) ys)");
    }

    #[test]
    fn e6_power_then_pipe() {
        assert_eq!(parse_shape("a ^ 2 |> f"), "(|> (^ a 2) f)");
    }

    // ═══════════════════════════════════════════════════════
    //  K: Compatibility — must match current behavior
    // ═══════════════════════════════════════════════════════

    #[test]
    fn k_basic_arithmetic() {
        assert_eq!(parse_shape("a + b * c"), "(+ a (* b c))");
        assert_eq!(parse_shape("a * b + c"), "(+ (* a b) c)");
        assert_eq!(parse_shape("a + b + c"), "(+ (+ a b) c)");
    }

    #[test]
    fn k_comparison() {
        assert_eq!(parse_shape("a + 1 > b - 2"), "(> (+ a 1) (- b 2))");
    }

    #[test]
    fn k_logical() {
        assert_eq!(parse_shape("a > 0 and b > 0"), "(and (> a 0) (> b 0))");
        assert_eq!(
            parse_shape("a > 0 and b > 0 or c > 0"),
            "(or (and (> a 0) (> b 0)) (> c 0))"
        );
    }

    #[test]
    fn k_range_basic() {
        assert_eq!(parse_shape("0..10"), "(.. 0 10)");
        assert_eq!(parse_shape("0..=10"), "(..= 0 10)");
    }

    #[test]
    fn k_power_right_assoc() {
        assert_eq!(parse_shape("2 ^ 3 ^ 2"), "(^ 2 (^ 3 2))");
    }

    #[test]
    fn k_unary_basic() {
        assert_eq!(parse_shape("-a + b"), "(+ (unary:- a) b)");
        assert_eq!(parse_shape("not a and b"), "(and (unary:not a) b)");
    }
}
