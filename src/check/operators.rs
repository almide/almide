use crate::types::Ty;
use super::{Checker, err, err_s};

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
                    self.diagnostics.push(err_s(
                        format!("operator '{}' requires numeric types but got {} and {}", op, left.display(), right.display()),
                        "Use Int or Float values with arithmetic operators".into(),
                        format!("operator '{}'", op),
                    ));
                    Ty::Unknown
                }
            }
            "^" => {
                if left.compatible(&Ty::Int) && right.compatible(&Ty::Int) { Ty::Int }
                else {
                    self.diagnostics.push(err(
                        format!("'^' (XOR) requires Int but got {} and {}", left.display(), right.display()),
                        "XOR only works on Int values", "operator '^'",
                    ));
                    Ty::Unknown
                }
            }
            "++" => {
                if left.compatible(&Ty::String) && right.compatible(&Ty::String) { Ty::String }
                else if matches!(left, Ty::List(_)) && left.compatible(right) { left.clone() }
                else {
                    self.diagnostics.push(err(
                        format!("'++' requires String or List but got {} and {}", left.display(), right.display()),
                        "Use '++' for String or List concatenation", "operator '++'",
                    ));
                    Ty::Unknown
                }
            }
            "==" | "!=" | "<" | ">" | "<=" | ">=" => Ty::Bool,
            "and" | "or" => {
                if !left.compatible(&Ty::Bool) {
                    self.diagnostics.push(err_s(
                        format!("'{}' requires Bool but left side is {}", op, left.display()),
                        "Use Bool values with logical operators".into(),
                        format!("operator '{}'", op),
                    ));
                }
                if !right.compatible(&Ty::Bool) {
                    self.diagnostics.push(err_s(
                        format!("'{}' requires Bool but right side is {}", op, right.display()),
                        "Use Bool values with logical operators".into(),
                        format!("operator '{}'", op),
                    ));
                }
                Ty::Bool
            }
            _ => Ty::Unknown,
        }
    }
}
