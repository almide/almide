//! The compiler primitive `fan.map`'s checked callback/result contract.
//! Shared by frontend constraints and native runtime ABI validation.
use super::Ty;

pub fn map_callback(input: Ty, output: Ty) -> Ty {
    Ty::Fn { is_effect: true, params: vec![input], ret: Box::new(Ty::result(output, Ty::String)) }
}

pub fn map_result(output: Ty) -> Ty {
    Ty::result(Ty::list(output), Ty::String)
}
