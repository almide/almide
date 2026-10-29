//! Where an operand needs parentheses to keep its grouping (#4319).
//!
//! A tool that builds an operator node from sub-trees it did not parse in
//! that position (`almide fix` turning `option.unwrap_or(x, 0 - 1)` into
//! `x ?? (0 - 1)`, or `bool.eq(a or b, c)` into `(a or b) == c`) prints the
//! tree back as text, and the formatter prints an operand as written. An
//! operand that binds looser than its slot re-parses into a different tree —
//! `x ?? 0 - 1` is `(x ?? 0) - 1` — so the tool must group it explicitly.
//!
//! The decision is read off the parser itself: the infix binding powers come
//! from [`Parser::infix_bp`], the table the Pratt loop uses, and an operator's
//! spelling is mapped to its token by the lexer. Nothing here lists
//! operators, so a new operator or a re-ranked one carries its grouping with it.

use super::Parser;
use crate::ast::{Expr, ExprKind};
use crate::lexer::{Lexer, TokenType};

/// The position an operand occupies next to an operator — which parse
/// function reads it, and so which expressions it accepts without grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// Left operand of an infix operator (a `parse_expr_bp` loop operand).
    InfixLeft(TokenType),
    /// Right operand of an infix operator (`parse_expr_bp(r_bp)`).
    InfixRight(TokenType),
    /// Left of `??`: the parser has read a postfix chain there.
    CoalesceValue,
    /// Right of `??`: the parser reads it with `parse_unary`.
    CoalesceFallback,
    /// Operand of prefix `-` / `not`: `parse_unary`.
    PrefixOperand,
    /// Object or callee of a postfix step (`.f`, `.0`, `(…)`, `[…]`, `!`, `?`, `?.`).
    PostfixBase,
}

/// How tightly an expression holds together when printed.
enum Level {
    /// Self-delimited: a literal, a name, a bracketed form, a postfix chain.
    Closed,
    /// A prefix form (`-x`, `not x`) or `a ?? b` — an operand of any infix
    /// operator, but nothing may follow it as a postfix.
    Unary,
    /// An infix node, by its operator token.
    Infix(TokenType),
    /// Anything that runs to the right with no closing token (`if`, `match`,
    /// a lambda, …) or whose shape this table does not know: always grouped.
    Open,
}

/// The token of an infix node's operator, from the lexer's own table.
pub fn infix_token(expr: &Expr) -> Option<TokenType> {
    match &expr.kind {
        ExprKind::Binary { op, .. } => Lexer::tokenize(op.as_str()).first().map(|t| t.token_type),
        ExprKind::Pipe { .. } => Some(TokenType::PipeArrow),
        ExprKind::Compose { .. } => Some(TokenType::ComposeArrow),
        ExprKind::Range { inclusive: true, .. } => Some(TokenType::DotDotDot),
        ExprKind::Range { inclusive: false, .. } => Some(TokenType::DotDotLt),
        _ => None,
    }
}

fn level(expr: &Expr) -> Level {
    match &expr.kind {
        ExprKind::Int { value, .. } if value.as_f64().is_some_and(|v| v < 0.0) => Level::Unary,
        ExprKind::Float { value, .. } if value.is_sign_negative() => Level::Unary,
        ExprKind::Int { .. }
        | ExprKind::Float { .. }
        | ExprKind::String { .. }
        | ExprKind::InterpolatedString { .. }
        | ExprKind::Bool { .. }
        | ExprKind::Ident { .. }
        | ExprKind::TypeName { .. }
        | ExprKind::List { .. }
        | ExprKind::MapLiteral { .. }
        | ExprKind::EmptyMap
        | ExprKind::Record { .. }
        | ExprKind::SpreadRecord { .. }
        | ExprKind::Call { .. }
        | ExprKind::Member { .. }
        | ExprKind::TupleIndex { .. }
        | ExprKind::IndexAccess { .. }
        | ExprKind::Block { .. }
        | ExprKind::Hole
        | ExprKind::Placeholder
        | ExprKind::Todo { .. }
        | ExprKind::Unwrap { .. }
        | ExprKind::ToOption { .. }
        | ExprKind::OptionalChain { .. }
        | ExprKind::Paren { .. }
        | ExprKind::Tuple { .. }
        | ExprKind::Unit
        | ExprKind::None
        | ExprKind::Some { .. }
        | ExprKind::Ok { .. }
        | ExprKind::Err { .. }
        // fmt always prints an ascription inside its own parentheses.
        | ExprKind::TypeAscription { .. } => Level::Closed,
        ExprKind::Unary { .. } | ExprKind::UnwrapOr { .. } => Level::Unary,
        ExprKind::Binary { .. } | ExprKind::Pipe { .. } | ExprKind::Compose { .. } | ExprKind::Range { .. } => {
            infix_token(expr).map_or(Level::Open, Level::Infix)
        }
        _ => Level::Open,
    }
}

fn is_comparison(tt: TokenType) -> bool {
    matches!(
        tt,
        TokenType::EqEq | TokenType::BangEq | TokenType::LAngle | TokenType::RAngle | TokenType::LtEq | TokenType::GtEq
    )
}

/// Whether `child`, printed as written in `slot`, would re-parse into a
/// different tree — i.e. whether it needs parentheses there.
pub fn needs_parens(child: &Expr, slot: Slot) -> bool {
    match level(child) {
        Level::Closed => false,
        Level::Unary => matches!(slot, Slot::CoalesceValue | Slot::PostfixBase),
        Level::Open => true,
        Level::Infix(inner) => {
            let Some((inner_l, inner_r)) = Parser::infix_bp(&inner) else { return true };
            match slot {
                // `(a op2 b) op c` stays grouped iff the loop that read `b`
                // stopped at `op`: `op`'s left power is below `op2`'s right.
                // Comparisons are non-associative: two in a row are refused.
                Slot::InfixLeft(op) => match Parser::infix_bp(&op) {
                    Some((l, _)) => l >= inner_r || (is_comparison(op) && is_comparison(inner)),
                    None => true,
                },
                // `a op (c op3 d)` stays grouped iff the right operand's loop,
                // run at `op`'s right power, takes `op3`.
                Slot::InfixRight(op) => match Parser::infix_bp(&op) {
                    Some((_, r)) => inner_l < r,
                    None => true,
                },
                Slot::CoalesceValue | Slot::CoalesceFallback | Slot::PrefixOperand | Slot::PostfixBase => true,
            }
        }
    }
}

/// The operand children of `expr` that sit next to an operator, with the slot
/// each occupies. Children in delimited positions (call arguments, list
/// elements, block statements) are not operands and are not listed.
pub fn operand_slots_mut(expr: &mut Expr) -> Vec<(&mut Expr, Slot)> {
    let tt = infix_token(expr);
    match &mut expr.kind {
        ExprKind::Binary { left, right, .. }
        | ExprKind::Pipe { left, right }
        | ExprKind::Compose { left, right }
        | ExprKind::Range { start: left, end: right, .. } => match tt {
            Some(tt) => vec![(&mut **left, Slot::InfixLeft(tt)), (&mut **right, Slot::InfixRight(tt))],
            None => Vec::new(),
        },
        ExprKind::UnwrapOr { expr, fallback } => {
            vec![(&mut **expr, Slot::CoalesceValue), (&mut **fallback, Slot::CoalesceFallback)]
        }
        ExprKind::Unary { operand, .. } => vec![(&mut **operand, Slot::PrefixOperand)],
        ExprKind::Member { object, .. }
        | ExprKind::TupleIndex { object, .. }
        | ExprKind::IndexAccess { object, .. }
        | ExprKind::Call { callee: object, .. }
        | ExprKind::Unwrap { expr: object }
        | ExprKind::ToOption { expr: object }
        | ExprKind::OptionalChain { expr: object, .. } => vec![(&mut **object, Slot::PostfixBase)],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Expr {
        let mut p = Parser::new(Lexer::tokenize(src));
        let e = p.parse_expr().expect("parses");
        assert!(p.errors.is_empty(), "{src}: {:?}", p.errors);
        e
    }

    /// Strip `Paren` nodes so two spellings compare by grouping alone.
    fn shape(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Paren { expr } => shape(expr),
            ExprKind::Binary { op, left, right } => format!("({op} {} {})", shape(left), shape(right)),
            ExprKind::Pipe { left, right } => format!("(|> {} {})", shape(left), shape(right)),
            ExprKind::UnwrapOr { expr, fallback } => format!("(?? {} {})", shape(expr), shape(fallback)),
            ExprKind::Unary { op, operand } => format!("({op} {})", shape(operand)),
            ExprKind::Member { object, field } => format!("{}.{field}", shape(object)),
            ExprKind::Ident { name } => name.to_string(),
            ExprKind::Int { raw, .. } => raw.clone(),
            ExprKind::Call { callee, args, .. } => {
                format!("{}({})", shape(callee), args.iter().map(shape).collect::<Vec<_>>().join(", "))
            }
            other => format!("{:?}", std::mem::discriminant(other)),
        }
    }

    /// Print a child into a slot the way `fmt` does — parenthesised only when
    /// `needs_parens` says so — and check the text re-parses into the same tree.
    fn roundtrip(parent: impl Fn(&str, &str) -> String, left: &str, right: &str, slots: (Slot, Slot)) {
        let group = |src: &str, slot: Slot| {
            if needs_parens(&parse(src), slot) { format!("({src})") } else { src.to_string() }
        };
        let printed = parent(&group(left, slots.0), &group(right, slots.1));
        let fully = parent(&format!("({left})"), &format!("({right})"));
        assert_eq!(shape(&parse(&printed)), shape(&parse(&fully)), "{printed} vs {fully}");
    }

    const OPERANDS: &[&str] = &[
        "a", "-a", "not a", "a ?? b", "a + b", "a - b", "a * b", "a ^ b", "a == b", "a < b", "a and b",
        "a or b", "a |> f", "f(a)", "a.b",
    ];

    #[test]
    fn every_operand_pair_keeps_its_grouping_under_every_operator() {
        for op in ["+", "-", "*", "/", "%", "^", "==", "!=", "<", ">=", "and", "or", "|>"] {
            let tt = Lexer::tokenize(op)[0].token_type;
            for l in OPERANDS {
                for r in OPERANDS {
                    roundtrip(|a, b| format!("{a} {op} {b}"), l, r, (Slot::InfixLeft(tt), Slot::InfixRight(tt)));
                }
            }
        }
    }

    #[test]
    fn every_operand_keeps_its_grouping_around_coalesce() {
        for l in OPERANDS {
            for r in OPERANDS {
                roundtrip(|a, b| format!("{a} ?? {b}"), l, r, (Slot::CoalesceValue, Slot::CoalesceFallback));
            }
        }
    }

    /// #4319: the fallback `0 - 1` binds looser than `??`.
    #[test]
    fn a_compound_fallback_is_grouped() {
        assert!(needs_parens(&parse("0 - 1"), Slot::CoalesceFallback));
        assert!(needs_parens(&parse("a or b"), Slot::CoalesceValue));
        assert!(!needs_parens(&parse("-1"), Slot::CoalesceFallback));
        assert!(!needs_parens(&parse("f(x)"), Slot::CoalesceValue));
        assert!(needs_parens(&parse("-x"), Slot::CoalesceValue));
    }
}
