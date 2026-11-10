//! The `mut` parameters of a linked callback (#4640).
//!
//! The standard codegen lends a `mut` parameter for writing (`xs: &mut
//! Vec<i64>`), and the native code reads what the callee wrote through that
//! borrow after the call. This render has no such borrow: a `mut` parameter
//! is passed by value and the function RETURNS what it wrote (the C-132
//! move-mode rewrite, `almide_ir::mut_param`), beside its own result and, for
//! a function that can fail, on the error arm as well.
//!
//! So the adapter copies the lent value in, calls, and stores each returned
//! value back through the borrow it was lent — once per parameter, on
//! whichever arm the call took, after the call returned. The copy in reads
//! the place through a shared reborrow and the value stored is a fresh one
//! built from the call's result: the rendered function never sees the host's
//! place, and nothing it owns is reachable from that place afterwards.
//!
//! The returned shape is not assumed. It is matched against the forms the
//! rewrite produces for exactly these parameters, and the host return type is
//! matched against the result that form leaves; anything else is refused by
//! name. One form is refused on purpose: a function that can fail without
//! carrying its parameters on the error arm, where a write made before the
//! error would be lost.
use almide_ir::IrFunction;
use almide_lang::types::Ty;
use crate::lower::LowerError;
use crate::native_callback_host::{canon, is_carrier};
use crate::native_foreign::{convert_value, host_type};
use crate::render_native::{wall, NativeSigKind};
use crate::render_native_closure::owned_nty;

/// What the rendered function returns beside the written parameters: its own
/// value, if it has one, and whether the error arm carries them too.
struct Carrier {
    value: Option<Ty>,
    on_err: bool,
}

/// `ty` as the written parameters `bufs`, alone or after the function's value.
fn paired(ty: &Ty, bufs: &[Ty]) -> Option<Option<Ty>> {
    if matches!(bufs, [one] if one == ty) { return Some(None); }
    let Ty::Tuple(items) = ty else { return None };
    if bufs.len() > 1 && items[..] == *bufs { return Some(None); }
    (items.len() == bufs.len() + 1 && items[1..] == *bufs).then(|| Some(items[0].clone()))
}

fn carrier(ret: &Ty, bufs: &[Ty]) -> Option<Carrier> {
    if let Some(value) = paired(ret, bufs) { return Some(Carrier { value, on_err: false }); }
    let (ok, err) = (ret.result_ok_ty()?, ret.result_err_ty()?);
    let Ty::Tuple(items) = &err else { return None };
    if items.first() != Some(&Ty::String) || items[1..] != *bufs { return None; }
    paired(&ok, bufs).map(|value| Carrier { value, on_err: true })
}

/// The adapter's answer on the arm that did not fail, as the host return
/// type `theirs` spells it.
fn answer(shape: &Carrier, theirs: &str) -> Option<&'static str> {
    let value = shape.value.clone().unwrap_or(Ty::Unit);
    let spelled = |ty: &Ty| host_type(ty).is_ok_and(|host| canon(&host) == canon(theirs));
    let (plain, wrapped) = if shape.value.is_some() { ("__almide_out", "Ok(__almide_out)") } else { ("()", "Ok(())") };
    if spelled(&Ty::result(value.clone(), Ty::String)) { return Some(wrapped); }
    (!shape.on_err && (spelled(&value) || (shape.value.is_none() && theirs.is_empty()))).then_some(plain)
}

/// The adapter's tail for a callback with `mut` parameters `places` (name and
/// declared type, in parameter order): the call, each write-back, the answer.
pub(crate) fn returned(
    f: &IrFunction, result: Option<NativeSigKind>, theirs: &str, call: &str, places: &[(&str, &Ty)],
) -> Result<String, LowerError> {
    let refuse = |what: &str| wall(format!("native: callback `{}` has `mut` parameters, and {what}", f.name));
    let bufs: Vec<Ty> = places.iter().map(|(_, ty)| (*ty).clone()).collect();
    let shape = carrier(&f.ret_ty, &bufs).ok_or_else(|| refuse("its rendered return is not its value beside each of them"))?;
    let kind = owned_nty(result.ok_or_else(|| refuse("this render returns no value for them"))?);
    if !shape.on_err && f.ret_ty.result_ok_ty().is_none() && is_carrier(kind) {
        return Err(refuse("it can fail without carrying them: a write made before the error would be lost"));
    }
    let answer = answer(&shape, theirs).ok_or_else(|| refuse(&format!("`{theirs}` is not a host spelling of its return")))?;
    let value = convert_value("&__almide_ret", &f.ret_ty, kind, true, 0, true)?;
    let stores: String = places.iter().enumerate().map(|(j, (name, _))| format!("*{name} = __almide_mut{j}; ")).collect();
    let muts = (0..places.len()).map(|j| format!("__almide_mut{j}")).collect::<Vec<_>>().join(", ");
    let ok = match (&shape.value, places.len()) {
        (None, 1) => muts.clone(),
        (None, _) => format!("({muts},)"),
        (Some(_), _) => format!("(__almide_out, {muts},)"),
    };
    Ok(if shape.on_err {
        format!("let __almide_ret = {call};\n    match {value} {{\n        Ok({ok}) => {{ {stores}{answer} }}\n        Err((__almide_err, {muts},)) => {{ {stores}Err(__almide_err) }}\n    }}")
    } else {
        format!("let __almide_ret = {call};\n    let {ok} = {value};\n    {}\n    {answer}", stores.trim_end())
    })
}

#[cfg(test)]
mod tests {
    use super::{answer, carrier, paired};
    use almide_lang::types::Ty;

    fn tuple(items: &[Ty]) -> Ty { Ty::Tuple(items.to_vec().into()) }

    #[test]
    fn the_returned_shape_is_one_of_the_rewrites_forms_for_these_parameters() {
        let (list, text) = (Ty::list(Ty::Int), Ty::String);
        assert_eq!(paired(&list, std::slice::from_ref(&list)), Some(None));
        assert_eq!(paired(&tuple(&[Ty::Int, list.clone()]), std::slice::from_ref(&list)), Some(Some(Ty::Int)));
        let two = [list.clone(), text.clone()];
        assert_eq!(paired(&tuple(&two), &two), Some(None));
        assert_eq!(paired(&tuple(&[list.clone(), list.clone(), text.clone()]), &two), Some(Some(list.clone())));
        assert_eq!(paired(&tuple(&[Ty::Int, text.clone()]), std::slice::from_ref(&list)), None, "another parameter's type");
        assert_eq!(paired(&Ty::Int, std::slice::from_ref(&list)), None, "the unrewritten return");
        let carried = Ty::result(tuple(&[Ty::Int, list.clone()]), tuple(&[text.clone(), list.clone()]));
        let shape = carrier(&carried, std::slice::from_ref(&list)).expect("the carried form");
        assert!(shape.on_err && shape.value == Some(Ty::Int));
        assert!(carrier(&Ty::result(tuple(&[Ty::Int, list.clone()]), text), &[list]).is_none(), "the error arm carries nothing");
    }

    #[test]
    fn the_answer_is_the_host_return_types_spelling_of_what_is_left() {
        let list = Ty::list(Ty::Int);
        let value = carrier(&tuple(&[Ty::Int, list.clone()]), std::slice::from_ref(&list)).expect("the value form");
        assert_eq!(answer(&value, "i64"), Some("__almide_out"));
        assert_eq!(answer(&value, "Result<i64, AlmideString>"), Some("Ok(__almide_out)"));
        assert_eq!(answer(&value, "AlmideString"), None);
        let unit = carrier(&list, std::slice::from_ref(&list)).expect("the unit form");
        assert_eq!(answer(&unit, "()"), Some("()"));
        assert_eq!(answer(&unit, ""), Some("()"));
        assert_eq!(answer(&unit, "Result<(), AlmideString>"), Some("Ok(())"));
        let carried = Ty::result(list.clone(), tuple(&[Ty::String, list.clone()]));
        let failing = carrier(&carried, &[list]).expect("the carried unit form");
        assert_eq!(answer(&failing, "Result<(), AlmideString>"), Some("Ok(())"));
        assert_eq!(answer(&failing, "()"), None, "a function that can fail answers a Result");
    }
}
