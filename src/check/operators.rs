use crate::types::Ty;
use super::{Checker, err};

impl Checker {
    pub(crate) fn check_binary_op(&mut self, op: &str, left: &Ty, right: &Ty) -> Ty {
        if matches!(left, Ty::Unknown) || matches!(right, Ty::Unknown) {
            return match op {
                "==" | "!=" | "<" | ">" | "<=" | ">=" | "and" | "or" => Ty::Bool,
                "++" => left.clone(),
                _ => Ty::Unknown,
            };
        }
        match op {
            "+" | "-" | "*" | "/" | "%" => {
                if left.compatible(&Ty::Int) && right.compatible(&Ty::Int) { Ty::Int }
                else if (left.compatible(&Ty::Float) || left.compatible(&Ty::Int))
                    && (right.compatible(&Ty::Float) || right.compatible(&Ty::Int)) { Ty::Float }
                else {
                    self.push_diagnostic(err(
                        format!("operator '{}' requires numeric types but got {} and {}", op, left.display(), right.display()),
                        "Use Int or Float values with arithmetic operators",
                        format!("operator '{}'", op),
                    ));
                    Ty::Unknown
                }
            }
            "^" => {
                // Int ^ Int = XOR, Float ^ Float / Int ^ Float / Float ^ Int = pow
                if left.compatible(&Ty::Int) && right.compatible(&Ty::Int) { Ty::Int }
                else if (left.compatible(&Ty::Float) || left.compatible(&Ty::Int))
                    && (right.compatible(&Ty::Float) || right.compatible(&Ty::Int)) { Ty::Float }
                else {
                    self.push_diagnostic(err(
                        format!("'^' requires numeric types but got {} and {}", left.display(), right.display()),
                        "Use Int values for XOR or Float values for exponentiation",
                        "operator '^'",
                    ));
                    Ty::Unknown
                }
            }
            "++" => {
                if left.compatible(&Ty::String) && right.compatible(&Ty::String) { Ty::String }
                else if matches!(left, Ty::List(_)) && left.compatible(right) { left.clone() }
                else {
                    self.push_diagnostic(err(
                        format!("'++' requires String or List but got {} and {}", left.display(), right.display()),
                        "Use '++' for String or List concatenation", "operator '++'",
                    ));
                    Ty::Unknown
                }
            }
            "==" | "!=" => {
                if !self.env.is_eq(left) {
                    self.push_diagnostic(err(
                        format!("'{}' cannot compare type {} — function types are not comparable", op, left.display()),
                        "Only value types (Int, String, List, records, variants, ...) support equality",
                        format!("operator '{}'", op),
                    ));
                }
                if !self.env.is_eq(right) {
                    self.push_diagnostic(err(
                        format!("'{}' cannot compare type {} — function types are not comparable", op, right.display()),
                        "Only value types (Int, String, List, records, variants, ...) support equality",
                        format!("operator '{}'", op),
                    ));
                }
                Ty::Bool
            }
            "<" | ">" | "<=" | ">=" => Ty::Bool,
            "and" | "or" => {
                if !left.compatible(&Ty::Bool) {
                    self.push_diagnostic(err(
                        format!("'{}' requires Bool but left side is {}", op, left.display()),
                        "Use Bool values with logical operators",
                        format!("operator '{}'", op),
                    ));
                }
                if !right.compatible(&Ty::Bool) {
                    self.push_diagnostic(err(
                        format!("'{}' requires Bool but right side is {}", op, right.display()),
                        "Use Bool values with logical operators",
                        format!("operator '{}'", op),
                    ));
                }
                Ty::Bool
            }
            _ => Ty::Unknown,
        }
    }
}
