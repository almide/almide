//! Which stdlib functions reach a host op of the primitive floor (#4827).
//!
//! The floor declares its sandbox exits: a `prim` fn marked `@prim_op(host)`
//! in stdlib/prim.almd is a host op. A stdlib function reaches the host when
//! a body that serves it calls one, directly or through a function it names:
//! its declaration in the module's source, or a self-host registry body that
//! serves its call name. The answer is read from those declarations and
//! bodies, never from a module or function name. A method an `effect fn`
//! dispatches on a parameter whose type is a protocol-bounded generic runs
//! the conformer the caller passes, which no body here shows: that dispatch
//! reaches the host. A plain `fn` cannot call an effect requirement (`Read`'s
//! read), so its dispatches stay judged by what it names.

use almide_lang::ast::{Decl, Expr, ExprKind, Program, TypeExpr};
use std::collections::HashSet;
#[cfg(test)]
use std::collections::BTreeSet;
#[cfg(test)]
use std::sync::OnceLock;

/// The floor's host ops: every `prim` fn declared `@prim_op(host)`.
#[cfg(test)]
pub(crate) fn prim_host_ops() -> &'static BTreeSet<String> {
    static OPS: OnceLock<BTreeSet<String>> = OnceLock::new();
    OPS.get_or_init(|| {
        almide_lang::stdlib_info::bundled_source("prim")
            .and_then(almide_lang::parse_cached)
            .map(host_ops_declared_in)
            .unwrap_or_default()
    })
}

/// The fns of `prog` whose `@prim_op` names `host` as its kind.
#[cfg(test)]
fn host_ops_declared_in(prog: &Program) -> BTreeSet<String> {
    prog.decls
        .iter()
        .filter_map(|d| match d {
            Decl::Fn { name, .. } if is_host_op(d) => Some(name.to_string()),
            _ => None,
        })
        .collect()
}

/// The bodies that serve the stdlib call `module.func`: its declaration in
/// the module's own source, and every self-host registry body registered
/// under that call name. Empty when nothing serves it.
pub fn serving_bodies(module: &str, func: &str) -> Vec<(&'static Program, String)> {
    let mut out = Vec::new();
    if let Some(prog) = almide_lang::stdlib_info::bundled_source(module).and_then(almide_lang::parse_cached) {
        if find_fn(prog, func).is_some() {
            out.push((prog, func.to_string()));
        }
    }
    let call = format!("{module}.{func}");
    for (src, rows) in almide_lang::self_host_registry::self_host_runtime() {
        for (imp, name) in rows.iter() {
            if *name == call {
                if let Some(prog) = almide_lang::parse_cached(src) {
                    out.push((prog, imp.to_string()));
                }
            }
        }
    }
    out
}

/// The self-host registry bodies served under the bare call name `call` (a
/// compiler-derived helper the IR names without a module: the derived
/// Codec's `__err_at`, `__decode_option_string`, …).
pub(crate) fn registry_bodies(call: &str) -> Vec<(&'static Program, String)> {
    let mut out = Vec::new();
    for (src, rows) in almide_lang::self_host_registry::self_host_runtime() {
        for (imp, name) in rows.iter() {
            if *name == call {
                if let Some(prog) = almide_lang::parse_cached(src) {
                    out.push((prog, imp.to_string()));
                }
            }
        }
    }
    out
}

/// Does fn `name` of the stdlib source `prog` reach a floor host op or an
/// output builtin that does not abort, directly or through what it calls?
/// A bodyless declaration it reaches is judged by its own `@category`.
pub(crate) fn body_reaches_category(prog: &Program, name: &str) -> bool {
    let mut seen = HashSet::new();
    reaches_in(prog, "", name, &mut seen, Mode::CATEGORY)
}

/// Does a body serving the stdlib call `module.func` reach a floor host op?
pub fn stdlib_fn_reaches_host(module: &str, func: &str) -> bool {
    let mut seen = HashSet::new();
    reaches_stdlib(module, func, &mut seen, Mode::HOST)
}

/// Does a body serving the stdlib call `module.func` reach a floor host op,
/// or call an output builtin that does not abort (`println`, `eprintln`:
/// [`crate::effect::builtin_writes_without_aborting`])? A builtin is named by
/// a bare call that no fn of the source declares.
pub fn stdlib_fn_reaches_category(module: &str, func: &str) -> bool {
    let mut seen = HashSet::new();
    reaches_stdlib(module, func, &mut seen, Mode::CATEGORY)
}

/// Does fn `name` of `prog`, a source of stdlib `module`, reach a floor host
/// op, directly or through a fn of the same source or a stdlib call it
/// names? A name it spells that resolves to nothing contributes nothing.
#[cfg(test)]
pub(crate) fn fn_reaches_host(prog: &Program, module: &str, name: &str) -> bool {
    let mut seen = HashSet::new();
    reaches_in(prog, module, name, &mut seen, Mode::HOST)
}

/// [`stdlib_fn_reaches_category`] for fn `name` of `prog`, a source of
/// stdlib `module` that is not (yet) bundled — the gate's control.
pub(crate) fn fn_reaches_category_in(prog: &Program, module: &str, name: &str) -> bool {
    let mut seen = HashSet::new();
    reaches_in(prog, module, name, &mut seen, Mode::CATEGORY)
}

type Seen = HashSet<(usize, String)>;

/// What a reach counts besides a floor host op.
#[derive(Clone, Copy)]
pub(crate) struct Mode {
    /// An output builtin that does not abort (`println`, `eprintln`).
    builtins: bool,
    /// A method an `effect fn` dispatches on a protocol-bounded param.
    dispatch: bool,
}

impl Mode {
    /// The host reach (#4827): host ops and bounded dispatch.
    const HOST: Mode = Mode { builtins: false, dispatch: true };
    /// Anything that carries a category: host ops, output builtins, bounded
    /// dispatch.
    const CATEGORY: Mode = Mode { builtins: true, dispatch: true };
    /// [`Mode::CATEGORY`] less the bounded dispatch, which the effect flow
    /// charges at the conformer the instantiation picks.
    const CATEGORY_BUT_DISPATCH: Mode = Mode { builtins: true, dispatch: false };
}

/// [`stdlib_fn_reaches_category`] less a bounded dispatch: does `module.func`
/// reach a category through anything but the conformer it dispatches to?
pub(crate) fn stdlib_fn_reaches_category_but_dispatch(module: &str, func: &str) -> bool {
    let mut seen = HashSet::new();
    reaches_stdlib(module, func, &mut seen, Mode::CATEGORY_BUT_DISPATCH)
}

/// A bodyless declaration that a self-host body also serves is that body's
/// signature: the body is what is followed. Only a call served by nothing
/// but a bodyless declaration is judged by that declaration.
fn reaches_stdlib(module: &str, func: &str, seen: &mut Seen, mode: Mode) -> bool {
    let all = serving_bodies(module, func);
    let bodied: Vec<_> = all.iter().filter(|(p, n)| find_fn(p, n).is_some_and(has_body)).cloned().collect();
    let judged = if bodied.is_empty() { all } else { bodied };
    judged.into_iter().any(|(prog, name)| reaches_in(prog, module, &name, seen, mode))
}

fn has_body(decl: &Decl) -> bool {
    matches!(decl, Decl::Fn { body: Some(b), .. } if !matches!(b.kind, ExprKind::Hole))
}

/// The reach of one declaration. A host op of the floor reaches the host by
/// its own declaration. A bodyless declaration (`= _`, served by the runtime)
/// shows no body to follow, so it is judged by what it declares: a non-empty
/// `@category` reaches the host, an empty one does not, and with neither it
/// reaches the host unless its module is classified `Pure`. An `effect fn`
/// body that dispatches a method on a protocol-bounded generic parameter
/// reaches it too ([`dispatches_on_a_bounded_param`]).
fn reaches_in(prog: &Program, module: &str, name: &str, seen: &mut Seen, mode: Mode) -> bool {
    if !seen.insert((prog as *const Program as usize, name.to_string())) {
        return false;
    }
    let Some(decl) = find_fn(prog, name) else { return false };
    if is_host_op(decl) {
        return true;
    }
    if let Decl::Fn { attrs, .. } = decl {
        if !has_body(decl) {
            return match attrs.iter().find(|a| a.name.as_str() == "category") {
                Some(a) => !a.args.is_empty(),
                None => crate::effect::module_effects(module) != Some(crate::effect::ModuleEffects::Pure),
            };
        }
    }
    if mode.dispatch && dispatches_on_a_bounded_param(decl) {
        return true;
    }
    named(decl).into_iter().any(|r| match r {
        Named::Qualified(m, f) => reaches_stdlib(&m, &f, seen, mode),
        // A bare name of a bodyless declaration that a self-host body serves
        // is followed into that body, as a qualified call is.
        Named::Bare(f) => match find_fn(prog, &f) {
            Some(d) if !has_body(d) && !serving_bodies(module, &f).is_empty() => reaches_stdlib(module, &f, seen, mode),
            Some(_) => reaches_in(prog, module, &f, seen, mode),
            // No fn of the source: a builtin. An output builtin (`println`,
            // `eprintln`) writes a stream; an abort builtin (`panic`,
            // `assert*`) is ADR-0022's halt, the class of a division-by-zero
            // trap, which a stdlib body may end on without a category — a fn
            // whose PURPOSE is the abort declares it (#4951).
            None => mode.builtins && crate::effect::builtin_writes_without_aborting(&f),
        },
    })
}

/// Does `decl`, an `effect fn`, call a method on a parameter typed by one of
/// its own generics that carries a protocol bound (`[H: Read](h: H)` calling
/// `h.__self_read(..)`)? The method run is the conformer's that the caller
/// supplies; the declaration shows none, so its reach is unknown and taken as
/// the host's. Only an `effect fn` can call a requirement that acts on the
/// host; a plain `fn`'s dispatch (a `Body`'s bytes) cannot. Read from the
/// declaration's shape, never from a protocol or method name.
fn dispatches_on_a_bounded_param(decl: &Decl) -> bool {
    let Decl::Fn { effect: Some(true), generics, params, .. } = decl else { return false };
    let bounded: HashSet<&str> = generics
        .iter()
        .flatten()
        .filter(|g| g.bounds.as_ref().is_some_and(|b| !b.is_empty()))
        .map(|g| g.name.as_str())
        .collect();
    let receivers: HashSet<String> = params
        .iter()
        .filter(|p| matches!(&p.ty, TypeExpr::Simple { name } if bounded.contains(name.as_str())))
        .map(|p| p.name.to_string())
        .collect();
    !receivers.is_empty()
        && named(decl).into_iter().any(|r| matches!(r, Named::Qualified(m, _) if receivers.contains(&m)))
}

/// Is `decl` a host op of the floor (`@prim_op(host)`)?
pub(crate) fn is_host_op(decl: &Decl) -> bool {
    let Decl::Fn { attrs, .. } = decl else { return false };
    attrs.iter().any(|a| {
        a.name.as_str() == "prim_op"
            && a.args.first().is_some_and(|x| {
                x.name.is_none() && matches!(&x.value, almide_lang::ast::AttrValue::Ident { name } if name.as_str() == "host")
            })
    })
}

fn find_fn<'p>(prog: &'p Program, name: &str) -> Option<&'p Decl> {
    prog.decls.iter().find(|d| matches!(d, Decl::Fn { name: n, .. } if n.as_str() == name))
}

enum Named {
    Bare(String),
    Qualified(String, String),
}

/// Every name a declaration's body spells as `f` or `m.f` — a call, a pipe
/// target, a fn passed as a value. Over-approximate on purpose: a local that
/// shares a fn's name only widens the reach.
///
/// The qualifier of `m.f` is not itself a name the body uses as a fn: a
/// local spelled like a fn of the source (`let float = …` beside
/// `random.float`) read as `float.__from_int(x)` names `float.__from_int`,
/// not `random.float`.
fn named(decl: &Decl) -> Vec<Named> {
    let mut d = decl.clone();
    let mut out = Vec::new();
    let mut qualifiers: Vec<*const Expr> = Vec::new();
    almide_lang::ast::visit_decl_exprs_mut(&mut d, &mut |e: &mut Expr| match &e.kind {
        ExprKind::Ident { name } => {
            if !qualifiers.contains(&(&*e as *const Expr)) {
                out.push(Named::Bare(name.to_string()));
            }
        }
        ExprKind::Member { object, field } => {
            if let ExprKind::Ident { name } = &object.kind {
                qualifiers.push(&**object as *const Expr);
                out.push(Named::Qualified(name.to_string(), field.to_string()));
            }
        }
        _ => {}
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leak_parse(src: String) -> &'static Program {
        almide_lang::parse_cached(Box::leak(src.into_boxed_str())).expect("parses")
    }

    #[test]
    fn the_floor_declares_its_host_ops() {
        let ops = prim_host_ops();
        for op in ["fd_write", "random_get", "write_text_file", "clock_time_get", "env_get"] {
            assert!(ops.contains(op), "`prim.{op}` is declared @prim_op(host)");
        }
        for op in ["load8", "store32", "band", "alloc_str", "rc_inc"] {
            assert!(!ops.contains(op), "`prim.{op}` is not a host op");
        }
    }

    /// The reach is followed through a same-source helper and refuses
    /// nothing a body does not spell (the control).
    #[test]
    fn a_body_reaches_a_host_op_through_a_helper() {
        let prog = leak_parse(
            "fn helper(p: String) -> Int = prim.write_text_file(p, p)\n\
             fn sneaky(p: String) -> Int = helper(p)\n\
             fn plain(p: String) -> Int = prim.load8(0)\n"
                .to_string(),
        );
        assert!(fn_reaches_host(prog, "string", "sneaky"));
        assert!(!fn_reaches_host(prog, "string", "plain"));
    }

    /// A host op reaches the host by its own declaration; another floor op
    /// does not.
    #[test]
    fn a_host_op_reaches_the_host_by_its_declaration() {
        assert!(stdlib_fn_reaches_host("prim", "fd_write"));
        assert!(stdlib_fn_reaches_host("prim", "write_text_file"));
        assert!(!stdlib_fn_reaches_host("prim", "load8"));
    }

    /// A fn receiving a handle through a protocol bound and reading it reaches
    /// the host, whatever it declares; the control is the same body with
    /// `@category()` over a concrete receiver, which reaches nothing.
    #[test]
    fn a_read_dispatched_on_a_bounded_handle_reaches_the_host() {
        let prog = leak_parse(
            "@category\neffect fn take[H: Read](h: H, n: Int) -> Result[Bytes, String] = h.__self_read(n)\n\
             @category\neffect fn take_plain(h: Stdin, n: Int) -> Result[Bytes, String] = h.__self_read(n)\n\
             effect fn via_helper[H: Read](h: H) -> Result[Bytes, String] = take(h, 4)\n"
                .to_string(),
        );
        assert!(fn_reaches_host(prog, "io", "take"));
        assert!(fn_reaches_host(prog, "io", "via_helper"), "reached through a same-source helper");
        assert!(!fn_reaches_host(prog, "io", "take_plain"));
    }

    /// The rule is the declaration's shape, not the `Read` protocol: an
    /// `effect fn` generic over a user protocol dispatching its method
    /// reaches the host. The controls do not: the same call over a concrete
    /// type, a plain `fn` dispatching on the same bound (it cannot call an
    /// effect requirement), and a bounded `effect fn` that only passes its
    /// param on as data.
    #[test]
    fn a_method_on_any_protocol_bounded_param_reaches_the_host() {
        let prog = leak_parse(
            "protocol Sink {\n  effect fn put(self, n: Int) -> Unit\n  fn size(self) -> Int\n}\n\
             type Box: Sink = { n: Int }\n\
             effect fn Box.put(self, n: Int) -> Unit = ()\n\
             fn Box.size(self) -> Int = self.n\n\
             effect fn fill[S: Sink](s: S) -> Unit = s.put(1)\n\
             effect fn fill_box(s: Box) -> Unit = s.put(1)\n\
             fn measure[S: Sink](s: S) -> Int = s.size()\n\
             effect fn keep[S: Sink](s: S) -> List[S] = [s]\n"
                .to_string(),
        );
        assert!(fn_reaches_host(prog, "geom", "fill"));
        assert!(!fn_reaches_host(prog, "geom", "fill_box"));
        assert!(!fn_reaches_host(prog, "geom", "measure"));
        assert!(!fn_reaches_host(prog, "geom", "keep"));
    }

    /// The served bodies: a registry body is followed under its call name.
    #[test]
    fn a_registry_body_is_followed_under_its_call_name() {
        assert!(stdlib_fn_reaches_host("datetime", "now"), "clock_now reads prim.clock_time_get");
        assert!(stdlib_fn_reaches_host("random", "int"));
        assert!(!stdlib_fn_reaches_host("datetime", "add_days"));
        assert!(!stdlib_fn_reaches_host("string", "trim"));
    }
}
