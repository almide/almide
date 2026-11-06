//! A closure argument the linked body's declaration does not spell (#4588).
//!
//! A linked self-hosted body is written for the wasm leg's representation,
//! where every heap value is one pointer wide: `option_map` declares
//! `(Int?, (Int) -> Int)` and the wasm leg runs it for an Option of any
//! payload. The native render dispatches a closure call through the table
//! the call's DECLARED kinds pick (`render_native_closure`), so a lambda of
//! another signature has no arm there; with no table of that arity at all,
//! the render named a dispatcher it never emitted. A call whose other
//! arguments the body accepts (a `none` rides the scalar Option carrier
//! whatever its payload type) can still pass such a closure, so each closure
//! argument's declared signature is checked against the param the callee
//! declares: a disagreement refuses the call like any argument the body does
//! not declare (the runtime body its declaration names serves it, or the
//! program walls).

use crate::lower::LowerError;
use crate::{CallArg, MirFunction, ValueId};
use almide_lang::types::Ty;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

/// A function type's params and result.
type FnSig = (Vec<Ty>, Ty);

#[derive(Default)]
struct Decls {
    /// Each declared function's function-typed params, by position.
    fn_params: BTreeMap<String, BTreeMap<usize, FnSig>>,
    /// The linked bodies: the callees the check applies to.
    linked: BTreeSet<String>,
}

thread_local! {
    static DECLS: RefCell<Decls> = RefCell::new(Decls::default());
}

/// Record the declarations of the program about to render, and which of its
/// functions are linked bodies.
pub(crate) fn set<'a>(decls: impl Iterator<Item = &'a almide_ir::IrFunction>, linked: BTreeSet<String>) {
    let mut fn_params: BTreeMap<String, BTreeMap<usize, FnSig>> = BTreeMap::new();
    for f in decls {
        let by_pos: BTreeMap<usize, FnSig> = f
            .params
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match &p.ty {
                Ty::Fn { params, ret, .. } => Some((i, (params.clone(), (**ret).clone()))),
                _ => None,
            })
            .collect();
        if !by_pos.is_empty() {
            fn_params.entry(f.name.as_str().to_string()).or_insert(by_pos);
        }
    }
    DECLS.with(|d| *d.borrow_mut() = Decls { fn_params, linked });
}

/// The call `name(args)` from the function being rendered: an error when a
/// closure argument's declared signature disagrees with the param `name`
/// declares for it.
pub(crate) fn check(name: &str, args: &[CallArg], user_fns: &BTreeMap<&str, &MirFunction>) -> Result<(), LowerError> {
    DECLS.with(|d| {
        let d = d.borrow();
        let Some(params) = d.fn_params.get(name).filter(|_| d.linked.contains(name)) else { return Ok(()) };
        for (i, want) in params {
            let Some(CallArg::Handle(v)) = args.get(*i) else { continue };
            let Some(got) = closure_sig(&d, *v, user_fns) else { continue };
            if !sig_agrees(want, &got) {
                eprintln!("PROBE4588 refuse {name} in {:?}: want {want:?} got {got:?}", crate::native_hof::current_fn());
                return Err(crate::render_native::wall(format!(
                    "native: the closure passed to `{name}` is not of the signature its param {i} declares (#4588)"
                )));
            }
        }
        Ok(())
    })
}

/// The declared signature of the closure `v`: the lambda its block was built
/// from, or the caller's own function-typed param it is.
fn closure_sig(d: &Decls, v: ValueId, user_fns: &BTreeMap<&str, &MirFunction>) -> Option<FnSig> {
    if let Some(lambda) = crate::native_hof::lambda_of(v) {
        return crate::lambda_sigs::get(&lambda);
    }
    let caller = crate::native_hof::current_fn()?;
    let declared = d.fn_params.get(&caller)?;
    let pos = user_fns.get(caller.as_str())?.params.iter().position(|p| p.value == v)?;
    declared.get(&pos).cloned()
}

fn sig_agrees((wp, wr): &FnSig, (gp, gr): &FnSig) -> bool {
    wp.len() == gp.len() && wp.iter().zip(gp).all(|(w, g)| ty_agrees(w, g)) && ty_agrees(wr, gr)
}

/// Two types the dispatch treats alike: equal, both on the scalar lane, or
/// one not fully known here (the check then has nothing to say).
fn ty_agrees(a: &Ty, b: &Ty) -> bool {
    let lane = crate::render_native_rec::is_lane_scalar;
    let open = |t: &Ty| t.contains_typevar() || t.contains_unknown();
    a == b || (lane(a) && lane(b)) || open(a) || open(b)
}
