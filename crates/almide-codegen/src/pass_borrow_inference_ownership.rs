// Included by pass_borrow_inference.rs: the per-function ownership decision,
// stated as a policy over the shared use-kind analysis (`use_kind.rs`,
// #2186), and the signature oracle that tells the walk what each call slot
// does with its argument.

/// What one fixed-point round reads — frozen at the round's start, so every
/// function in the round sees the same signatures.
pub(crate) struct Round<'a> {
    /// The signatures known when the round began. A callee's slot mode is
    /// read from here; the round's own results land in the live table the
    /// driver owns.
    pub snapshot: &'a HashMap<String, Vec<ParamBorrow>>,
    /// Every fn this pass WILL analyse (bare name for program fns,
    /// `mod::name` for module fns). A call to one of these whose signature
    /// is not in the snapshot yet is a forward or MUTUALLY RECURSIVE
    /// reference inside the same round — treated optimistically like a
    /// self-call (#2040): the first round seeds it as borrowed, and a callee
    /// that turns out to consume the slot promotes the caller to Own in the
    /// next round. Seeding it Own on the first miss locked every
    /// `parse_rule ↔ parse_seq ↔ …` group to by-value + clone per call.
    pub pending: &'a HashSet<String>,
    /// Names of user-declared RECORD types (`type Tok = { … }`). A param of
    /// such a type is `Ty::Named("Tok")` (not a structural `Ty::Record`), so
    /// without this set `is_borrow_eligible` / `intrinsic_borrow_mode` treat
    /// it as Own and every reader deep-clones the whole record. Records get
    /// borrow inference like structural records. #647
    pub records: &'a HashSet<String>,
    /// Names of user-declared VARIANT types. A variant param is
    /// borrow-eligible too; its `match` subject position reads it by
    /// reference when every binder the arms introduce is itself only read
    /// ([`scrutinee_binders_borrow_only`]) — the match then lowers as the
    /// borrowed-subject match the clone pass already emits for a live
    /// subject, and every caller keeps its value without a clone.
    pub variants: &'a HashSet<String>,
}

/// One function's view of a [`Round`]: the module it lives in (its callees
/// resolve `mod::name` before the bare name) and its own name (a
/// self-recursive call is treated optimistically — see [`Scope::call_slot`]).
pub(crate) struct Scope<'a> {
    pub round: &'a Round<'a>,
    pub module: Option<&'a str>,
    pub current_fn: &'a str,
}

/// What the snapshot says about a callee.
enum Callee<'a> {
    /// Its signature is published.
    Known(&'a [ParamBorrow]),
    /// A fn this pass analyses whose signature the snapshot does not hold
    /// YET: a forward reference or a mutual-recursion partner in the current
    /// round.
    Pending,
    /// Nothing will ever be published under that name.
    Unknown,
}

impl Scope<'_> {
    /// Resolve `callee`: the module-scoped key first, then the bare one — and
    /// a key that is PENDING stops the search where a published one would
    /// have, so a round never reads the bare key while the scoped one is
    /// still to come (that switch between rounds is a descent the monotone
    /// ascent must never take).
    fn resolve(&self, callee: &str) -> Callee<'_> {
        let scoped = self.module.map(|m| format!("{}::{}", m, callee));
        for key in scoped.iter().map(String::as_str).chain(std::iter::once(callee)) {
            if let Some(b) = self.round.snapshot.get(key) {
                return Callee::Known(b);
            }
            if self.round.pending.contains(key) {
                return Callee::Pending;
            }
        }
        Callee::Unknown
    }

    fn is_borrow_eligible(&self, ty: &Ty) -> bool {
        is_borrow_eligible(ty, self.round.records) || is_named_in(ty, self.round.variants)
    }
}

/// The slot mode a signature entry spells: a missing slot is `absent`.
fn slot_of(entry: Option<&ParamBorrow>, absent: SlotMode) -> SlotMode {
    match entry {
        None => absent,
        Some(ParamBorrow::Own) => SlotMode::Consume,
        Some(ParamBorrow::RefMut) => SlotMode::Mut,
        Some(ParamBorrow::Ref | ParamBorrow::RefSlice | ParamBorrow::RefStr) => SlotMode::Borrow,
    }
}

impl SlotOracle for Scope<'_> {
    /// A user callee's slot: consult the fixed-point snapshot so a caller can
    /// transitively keep `data` borrowed when the callee also borrows it.
    /// Generalized from `Ty::Bytes` to every borrow-eligible type (records,
    /// lists, strings) so the natural `vocab_id(t, ..)` / `merge_rank(t, ..)`
    /// factoring no longer clones the whole record per call (#647).
    ///
    /// A call into a sibling user module (`other.get_far(ts, i)`) is a user
    /// callee too — its signature sits under `module::func` (#2164). A
    /// bundled stdlib module's fns arrive the same way: their borrow modes
    /// were seeded from the PARSED declaration (`@intrinsic` param types and
    /// the `@consume` / `@borrow_ref` / `mut` attributes), never from a
    /// template's text.
    ///
    /// A method receiver's args and a computed callee's args are consumed:
    /// nothing names their signature here.
    fn call_slot(&self, target: &CallTarget, index: usize, arg: &IrExpr) -> SlotMode {
        let name = match target {
            // Self-recursive: optimistic. For tail-recursive parsers passing
            // the same `data` through, the first-pass pessimism must not lock
            // the param to Own and prevent the fixed point from promoting it.
            CallTarget::Named { name } if name.as_str() == self.current_fn => return SlotMode::Borrow,
            CallTarget::Named { name } => name.to_string(),
            CallTarget::Module { module, func, .. } => format!("{}::{}", module, func),
            CallTarget::Method { .. } | CallTarget::Computed { .. } => return SlotMode::Consume,
        };
        match self.resolve(&name) {
            Callee::Known(borrows) => {
                let mode = slot_of(borrows.get(index), SlotMode::Consume);
                // A fn-typed argument rides a callee's NON-ESCAPING slot as a
                // borrow too (#2288): the callee only calls it.
                if mode != SlotMode::Consume && (self.is_borrow_eligible(&arg.ty) || matches!(arg.ty, Ty::Fn { .. })) { mode } else { SlotMode::Consume }
            }
            Callee::Pending => SlotMode::Borrow,
            Callee::Unknown => SlotMode::Consume,
        }
    }

    /// The lowered form of an `@intrinsic` / bundled call: its signature sits
    /// under the mangled symbol. A slot past the declared params borrows (a
    /// default-filled tail); an unknown symbol consumes everything.
    fn runtime_slot(&self, symbol: Sym, index: usize, _arg: &IrExpr) -> SlotMode {
        match self.round.snapshot.get(symbol.as_str()) {
            Some(borrows) => slot_of(borrows.get(index), SlotMode::Borrow),
            None => SlotMode::Consume,
        }
    }
}

/// The positions that need the value OWNED: the borrow policy over one
/// occurrence. Every constructor operand, concat operand, match subject,
/// fold seed, method receiver, consuming call slot and result position moves
/// the value; so does any occurrence inside a closure (the `move` capture
/// takes it), and an iteration — a `for` head or a fused chain's source —
/// whose body needs the ELEMENTS owned (`element_reads_only`: the walk
/// decides `Iterable::consumed` from what the body does with the element,
/// never from the combinator's slot). A heap-typed field read straight off the param
/// into a record literal counts too: `CloneInsertion` moves such fields out
/// of an owned final-use record instead of cloning them
/// (`pass_clone_record_fields`), and that rewrite exists for record literals
/// only — a list element or an interpolation part clones the field either way.
fn consumes(u: &Use) -> bool {
    if u.depth > 0 {
        return true;
    }
    match u.site {
        Site::Result | Site::Scrutinee | Site::Concat | Site::Construct(_) | Site::Receiver
        | Site::Callback | Site::FoldInit | Site::Arg(SlotMode::Consume)
        | Site::Iterable { consumed: true } => true,
        Site::Member => matches!(
            u.chain,
            Some(Chain { top: Site::Construct(Ctor::Record), len: 1, heap: true })
        ),
        Site::Arg(SlotMode::Borrow | SlotMode::Mut) | Site::Callee
        | Site::Iterable { consumed: false } | Site::Borrow { .. } | Site::Clone
        | Site::TupleIndex | Site::Index | Site::MapKeyed | Site::Deref | Site::Operand | Site::Compare
        | Site::Assigned | Site::Reassign | Site::InPlace => false,
    }
}

/// Borrow modes for one function's params.
///
/// `@inline_rust` / `@wasm_intrinsic` fns are dispatch-only declarations with
/// a Hole body whose template is authoritative for borrow semantics (it spells
/// `&*{s}` / `&{m}` / `{n}` explicitly), so every param is `Own` and the
/// template controls the arg decoration verbatim. `@intrinsic` has no
/// template: the mode is derived mechanically from each param's Almide type so
/// `BorrowInsertion` (not the walker) decorates args at the call site.
fn infer_function_borrows(func: &IrFunction, scope: &Scope) -> Vec<ParamBorrow> {
    let has_inline_template = func.attrs.iter().any(|a|
        matches!(a.name.as_str(), "inline_rust" | "wasm_intrinsic"));
    if has_inline_template {
        return func.params.iter().map(|_| ParamBorrow::Own).collect();
    }
    let has_intrinsic = func.attrs.iter().any(|a| a.name.as_str() == "intrinsic");
    if has_intrinsic {
        return func.params.iter().map(|p| intrinsic_borrow_mode(&p.ty, scope.round.records)).collect();
    }
    let uses = UseSites::of_fn(func, scope);
    func.params.iter().enumerate().map(|(slot, param)| param_borrow(slot, param, &uses, scope, &func.body)).collect()
}

/// Does every `match` in `body` whose subject is the bare variable `var`
/// bind nothing — literal, wildcard and nullary patterns only? Such a match
/// on a `String` renders as `match &*s { "lit" => .. }` (`MatchSubject`),
/// which borrows: the subject position does not consume the param (#2231 —
/// `fn string_match(s: String) -> Int = match s { "alpha" => 1, .. }` owned
/// `s` for three literal comparisons). A pattern that binds may move a
/// payload out, so any binding keeps the conservative verdict.
fn scrutinee_only_compares(body: &IrExpr, var: VarId) -> bool {
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
    struct Scan { var: VarId, ok: bool }
    fn binds_nothing(p: &IrPattern) -> bool {
        match p {
            IrPattern::Wildcard | IrPattern::Literal { .. } | IrPattern::None => true,
            IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => binds_nothing(inner),
            IrPattern::Constructor { args, .. } => args.iter().all(binds_nothing),
            IrPattern::Tuple { elements } => elements.iter().all(binds_nothing),
            IrPattern::List { elements, rest } => elements.iter().all(binds_nothing) && rest.as_deref().map_or(true, binds_nothing),
            IrPattern::Bind { .. } | IrPattern::RecordPattern { .. } | IrPattern::As { .. } => false,
        }
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Match { subject, arms } = &e.kind
                && matches!(subject.kind, IrExprKind::Var { id } if id == self.var)
                && !arms.iter().all(|a| binds_nothing(&a.pattern))
            {
                self.ok = false;
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) { walk_stmt(self, s); }
    }
    let mut scan = Scan { var, ok: true };
    scan.visit_expr(body);
    scan.ok
}

/// Is `ty` a `Ty::Named` whose name is in `names`?
pub(crate) fn is_named_in(ty: &Ty, names: &HashSet<String>) -> bool {
    matches!(ty, Ty::Named(n, _) if names.contains(n.as_str()))
}

/// Does every `match` in `body` whose subject is the bare variable `var`
/// bind only payloads the arms READ — a `Copy` scalar, or a heap value whose
/// every occurrence is a non-consuming position (a borrowed argument, a
/// member read, a comparison, an interpolation part)? Then the subject can
/// be matched by reference: the arms bind `&T` payloads and nothing needs
/// the value moved out. One binder returned, concatenated, built into a
/// record / list, captured or handed to an owned slot keeps the subject
/// owned, where matching by value moves the payload out for free. Shared
/// by the borrow verdict and the ownership certifier's C4, so the two read
/// one rule.
pub(crate) fn scrutinee_binders_borrow_only(body: &IrExpr, var: VarId, uses: &UseSites) -> bool {
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
    use std::collections::HashMap;
    /// Every match in the body whose subject is a variable (bare, or under
    /// the clone / borrow / box-deref a pass wrapped it in), by that variable:
    /// the binders its arms introduce.
    struct Scan { matches: HashMap<VarId, Vec<(VarId, Ty)>> }
    fn binders(p: &IrPattern, out: &mut Vec<(VarId, Ty)>) {
        match p {
            IrPattern::Wildcard | IrPattern::Literal { .. } | IrPattern::None => {}
            IrPattern::Bind { var, ty } => out.push((*var, ty.clone())),
            IrPattern::As { var, ty, inner } => { out.push((*var, ty.clone())); binders(inner, out); }
            IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => binders(inner, out),
            IrPattern::Constructor { args, .. } => args.iter().for_each(|a| binders(a, out)),
            IrPattern::Tuple { elements } => elements.iter().for_each(|e| binders(e, out)),
            IrPattern::List { elements, rest } => {
                elements.iter().for_each(|e| binders(e, out));
                if let Some(r) = rest { binders(r, out); }
            }
            IrPattern::RecordPattern { fields, .. } => {
                for f in fields { if let Some(p) = &f.pattern { binders(p, out); } }
            }
        }
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            // The subject is the variable itself, or the clone / borrow /
            // box-deref of it a pass wrapped it in: the certifier reads the
            // FINAL IR, where the clone pass has already spelled a held
            // subject as `xs.clone()` and BoxDeref a boxed payload as `*t`,
            // and must see the same matches the verdict saw.
            if let IrExprKind::Match { subject, arms } = &e.kind
                && let Some(id) = subject_root(subject)
            {
                let entry = self.matches.entry(id).or_default();
                for arm in arms { binders(&arm.pattern, entry); }
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) { walk_stmt(self, s); }
    }
    let mut scan = Scan { matches: HashMap::new() };
    scan.visit_expr(body);
    if !scan.matches.contains_key(&var) {
        return false;
    }
    // A binder is READ when none of its occurrences consumes it — directly,
    // or through a projection chain (`*t` of a boxed payload into a
    // constructor, `kids` iterated by value: the use-kind walk records those
    // as a `Deref` / `Member` root with the consuming site on top). A binder
    // that is only the subject of a further match (a nested `match *t`)
    // reads through that match, so its binders join the check.
    let consumed = |u: &Use| consumes(u)
        || matches!(u.chain, Some(c) if c.heap && c.top != Site::Scrutinee && consumes(&Use { site: c.top, chain: None, ..*u }));
    let nested_subject = |u: &Use| u.site == Site::Scrutinee
        || matches!((u.site, u.chain), (Site::Deref, Some(c)) if c.top == Site::Scrutinee && c.len == 1);
    let mut todo = vec![var];
    let mut seen: std::collections::HashSet<VarId> = std::collections::HashSet::new();
    while let Some(root) = todo.pop() {
        if !seen.insert(root) { continue; }
        let Some(bound) = scan.matches.get(&root) else { continue };
        for (b, ty) in bound {
            if almide_ir::top_let_storage::clone_free(ty) { continue; }
            for u in uses.of(*b) {
                if nested_subject(u) {
                    if !scan.matches.contains_key(b) { return false; }
                    todo.push(*b);
                } else if consumed(u) && !u.in_guard {
                    // A guard's consuming read clones (#2605), from a `&T`
                    // binder as well as from an owned one.
                    return false;
                }
            }
        }
    }
    true
}

/// The variable a match subject reads, through the wrappers passes add:
/// `Clone` (the clone pass, a held subject), `Borrow` (the clone pass, a
/// live subject) and `Deref` (BoxDeref, a boxed payload binder).
pub(crate) fn subject_root(s: &IrExpr) -> Option<VarId> {
    match &s.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Clone { expr } | IrExprKind::Borrow { expr, .. } | IrExprKind::Deref { expr } => subject_root(expr),
        _ => None,
    }
}

/// Is `later` after `earlier` in evaluation order? Occurrences are recorded
/// in that order, so pointer position in the table decides.
fn after(earlier: &Use, later: &Use) -> bool {
    (later as *const Use) > (earlier as *const Use)
}

/// One param's mode from the body's occurrences of it.
fn param_borrow(slot: usize, param: &IrParam, uses: &UseSites, scope: &Scope, body: &IrExpr) -> ParamBorrow {
    // An explicit `mut` param is passed by mutable reference, and the keyword
    // is authoritative: the checker (`validate_mut_args`) guarantees the
    // caller hands over a `var` binding, so the param IS a `&mut T` by
    // construction regardless of how the body uses it — it may mutate a
    // *field* (`list.push(b.xs, v)` on `mut b`, #703), forward it to another
    // `mut` callee, or reassign it. Honored before the body policy AND before
    // the heap guard (#2243): a `mut d: Int` used to fall through the guard
    // as an owned `i64`, so `d = d + 1` was rustc E0384 on the one native path
    // that does not inline the callee (the test leg) while the wasm leg and
    // the inlined binary mutated the caller's `d` as the language means.
    if param.is_mut {
        return ParamBorrow::RefMut;
    }
    if matches!(param.ty, Ty::Fn { .. }) {
        return fn_param_borrow(slot, param, uses, scope, body);
    }
    if !scope.is_borrow_eligible(&param.ty) || almide_base::env::flag("ALMIDE_BORROW_OWN_ALL") {
        return ParamBorrow::Own;
    }
    // A `String` param a `match` only compares, or an interpolation only
    // formats (`format_args!` borrows its parts), is read by reference at
    // those positions (#2231).
    let is_string = matches!(param.ty, Ty::String);
    let literal_subject = is_string && scrutinee_only_compares(body, param.var);
    // A variant param whose every `match` only READS what it binds — scalar
    // payloads, heap payloads at borrowed positions — is matched by
    // reference: the arms bind `&T` payloads (Rust's default binding modes),
    // the shape the clone pass already emits for a live subject, and no
    // caller clones the value to pass it. A payload that is returned,
    // built into a value or handed to an owned slot keeps the owned
    // verdict: matching by value moves it out for free where a borrowed
    // match would clone it.
    let variant_subject = is_named_in(&param.ty, scope.round.variants)
        && scrutinee_binders_borrow_only(body, param.var, uses);
    let read_by_ref = |u: &Use| ((literal_subject || variant_subject) && u.site == Site::Scrutinee)
        || (is_string && u.site == Site::Construct(Ctor::Interp) && u.depth == 0);
    // A consuming use that a LATER statement follows with another use of
    // the param can never move it — `CloneInsertion` clones it because the
    // var stays live — so it earns the param nothing by being owned; only a
    // consuming use with no later-statement use can be the move ownership
    // exists for (#2231: `show_list(xs)` consumed `xs` as a chain source and
    // read `list.len(xs)` on the next line — owned, and cloned anyway).
    // Refined further: a consuming use is CLONED ANYWAY — and so earns
    // nothing by ownership — when a later occurrence can still run after it
    // (same arm, an enclosing one, or a nested one — anything but a sibling
    // branch: the var stays live, `CloneInsertion` clones; a guarded list
    // pattern's fall-through re-reads the subject inside the arm), or when a
    // direct `&v` argument of the same call keeps the var borrowed through
    // it (`map.fold(&base, base.clone(), λ)`: the E0505 guard clones).
    // A consuming use inside a loop body is cloned on every iteration (a
    // param is never one of the loop's own fresh binders). `UseSites::keeps_live`,
    // `Use::guard_forced` and `Use::in_loop` are the same facts the clone
    // pass acts on.
    let all: Vec<&Use> = uses.of(param.var).collect();
    let cloned_anyway = |u: &Use| u.guard_forced || u.in_loop
        || all.iter().any(|w| !std::ptr::eq(*w, u) && after(u, w) && uses.keeps_live(u, w));
    if all.iter().any(|u| consumes(u) && !read_by_ref(u) && !cloned_anyway(u)) {
        return ParamBorrow::Own;
    }
    // Implicit mut for bundled bodies: when the body forwards this param into
    // a callee slot that expects `RefMut` (`bytes.set_u16_le` et al), the
    // caller's own param must also be `RefMut` — else the generated code
    // writes `&mut b` against a `b: &Vec<u8>` sig, which fails to borrow-check.
    if uses.of(param.var).any(|u| u.site == Site::Arg(SlotMode::Mut)) {
        return ParamBorrow::RefMut;
    }
    match &param.ty {
        Ty::String => ParamBorrow::RefStr,
        Ty::Applied(TypeConstructorId::List, _) => ParamBorrow::RefSlice,
        _ => ParamBorrow::Ref,
    }
}

/// A fn-typed param (#2288): `&dyn Fn(A) -> B` unless an occurrence lets the
/// callable ESCAPE the call — then the `Rc<dyn Fn>` handle it always was.
/// Calling it, handing it to another fn's non-escaping slot (the fixed
/// point's `Borrow` mode, optimistic for a pending or self-recursive callee)
/// and borrowing it do not escape. The ablation `ALMIDE_FN_ESCAPE_OFF=1`
/// borrows every fn param regardless — the certifier's C5 negative control.
fn fn_param_borrow(slot: usize, param: &IrParam, uses: &UseSites, scope: &Scope, body: &IrExpr) -> ParamBorrow {
    if almide_base::env::flag("ALMIDE_FN_ESCAPE_OFF") {
        return ParamBorrow::Ref;
    }
    if uses.of(param.var).any(fn_param_escapes) || self_call_rebinds(body, scope.current_fn, slot, param.var) {
        ParamBorrow::Own
    } else {
        ParamBorrow::Ref
    }
}

/// Does a self-recursive call hand this param's slot anything but the param
/// itself? A CPS accumulator (`ascending(b, (ys) => acc(ICons(a, ys)), rest)`)
/// does: the new callable captures the old one. Plain recursion could borrow
/// it — each frame's closure lives in that frame — but the tail-call loop
/// rewrite reassigns the slot (`acc = &closure`) across iterations, and a
/// closure built inside one iteration does not outlive it. The slot stays the
/// `Rc<dyn Fn>` handle; passing the param through unchanged (`walk(f, n - 1)`)
/// stays borrowed.
fn self_call_rebinds(body: &IrExpr, fn_name: &str, slot: usize, var: VarId) -> bool {
    use almide_ir::visit::{IrVisitor, walk_expr, walk_stmt};
    struct Scan<'a> { fn_name: &'a str, var: VarId, slot: usize, hit: bool }
    impl IrVisitor for Scan<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &e.kind
                && name.as_str() == self.fn_name
                && let Some(arg) = args.get(self.slot)
                && !matches!(arg.kind, IrExprKind::Var { id } if id == self.var)
            {
                self.hit = true;
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) { walk_stmt(self, s); }
    }
    let mut scan = Scan { fn_name, var, slot, hit: false };
    scan.visit_expr(body);
    scan.hit
}

/// Does this occurrence let a fn-typed param's callable outlive the call?
/// Returned, stored (bound, built into a value, assigned), handed to an owned
/// slot or as a stored chain callback, captured by a closure, or anything
/// this file cannot name: escapes. Called, borrowed, cloned, handed to a
/// borrowed slot: does not. Shared with the ownership certifier's C5, so the
/// verdict and its check read one rule.
pub(crate) fn fn_param_escapes(u: &Use) -> bool {
    if u.depth > 0 {
        return true;
    }
    !matches!(u.site, Site::Callee | Site::Arg(SlotMode::Borrow) | Site::Borrow { mutable: false } | Site::Clone)
}

/// Borrow mode derived from an `@intrinsic` fn's Almide param type.
/// Used to populate the signature table so `BorrowInsertion` can
/// decorate call-site args uniformly without walker-side heuristics.
fn intrinsic_borrow_mode(ty: &Ty, records: &HashSet<String>) -> ParamBorrow {
    match ty {
        // Owned scalars — pass by value.
        Ty::Int | Ty::Int8 | Ty::Int16 | Ty::Int32
        | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
        | Ty::Float | Ty::Float32 | Ty::Bool | Ty::Unit
            => ParamBorrow::Own,

        // String → &str.
        Ty::String => ParamBorrow::RefStr,

        // List → &Vec / &[T].
        Ty::Applied(TypeConstructorId::List, _) => ParamBorrow::RefSlice,

        // Bytes / Record / Variant / Map / Set → & reference.
        Ty::Bytes
        | Ty::Record { .. } | Ty::Variant { .. }
        | Ty::Applied(TypeConstructorId::Map, _)
        | Ty::Applied(TypeConstructorId::Set, _)
            => ParamBorrow::Ref,

        // A user-declared RECORD type (`t: Tok` → `Ty::Named("Tok")`) borrows like
        // a structural record (#647). Non-record Named types fall through to Own.
        Ty::Named(n, _) if records.contains(n.as_str()) => ParamBorrow::Ref,

        // Option / Result → Own. `.unwrap_or` / `.map` consume the
        // container, and the walker renders `.is_some()` /
        // `.is_none()` via `Fn(Option<T>) -> bool` signatures that
        // accept the value by move and borrow internally. Passing a
        // `&Option<T>` would break the runtime-fn ergonomics for no
        // Almide-level gain.
        Ty::Applied(TypeConstructorId::Option, _)
        | Ty::Applied(TypeConstructorId::Result, _)
            => ParamBorrow::Own,

        // Generic TypeVar / user types / Fn / Tuple / etc. — pass owned.
        // The caller knows the concrete type; if it resolves to a borrow
        // type downstream, Clone/Borrow annotations travel through the
        // call unchanged.
        _ => ParamBorrow::Own,
    }
}

/// Eligible types for borrow inference — a REFINEMENT over the heap
/// classification, not another definition of it (#926): every type admitted
/// here is heap, but not every heap type is admitted. The narrowing is the
/// point and each exclusion is a reasoned one — `Fn` values ride the closure
/// ABI (their ownership story is the env block's, not a `&`/`&mut` param),
/// `Unknown` cannot be borrowed against a type the checker never resolved, and
/// `Option`/`Result` params pass through the Own path their unwrap machinery
/// expects. It was NAMED `is_heap_type`, which is how an audit read it as a
/// sixth divergent copy of the classification; the name now says which
/// question it answers.
///
/// The Record case is the key
/// addition — without it, a `GGUFFile`-style record carried through a
/// layer loop gets `.clone()` inserted on every iteration (observed on
/// bonsai-almide at 72% inclusive time, cf.
/// memory/feedback_almide_bytes_clone.md).
pub(crate) fn is_borrow_eligible(ty: &Ty, records: &HashSet<String>) -> bool {
    matches!(ty,
        Ty::String
        | Ty::Bytes
        | Ty::Applied(TypeConstructorId::List, _)
        // Map/Set are heap collections too — without them a `mut Map`/`mut Set`
        // parameter is forced to `Own` here (never reaching the borrow analysis),
        // so an in-place `map.insert(m, …)` emits `&mut m` against a non-`mut`
        // owned binding and fails to borrow-check (#436, E0596). With them the
        // param is inferred Ref/RefMut/Own like a List.
        | Ty::Applied(TypeConstructorId::Map, _)
        | Ty::Applied(TypeConstructorId::Set, _)
        | Ty::Record { .. }
        | Ty::OpenRecord { .. }
    ) || matches!(ty, Ty::Named(n, _) if records.contains(n.as_str()))
    // `Value`, the codec universal model, reads like a record: every
    // `value.*` intrinsic already takes `&Value` (`intrinsic_borrow_mode`),
    // so a user or derived fn that only feeds its `Value` param to those
    // never needs to own it (#1679 — decode was cloning an 8-field object
    // per call to read it once).
    || is_value_ty(ty)
}

fn is_value_ty(ty: &Ty) -> bool {
    matches!(ty, Ty::Named(n, _) if n.as_str() == "Value")
}

/// Borrow modes for a `@derived` convention fn (and the codec workers the
/// derive emits). Derives are a generated API surface whose call sites pass
/// owned values and cannot always see an inferred signature (cross-module
/// bare keys, #1549), so only their `Value` params are inferred — a derived
/// `decode` reads its input through `value.*` intrinsics and never needs to
/// own it (#1679). Every other param keeps `Own`, exactly as before.
fn derived_value_borrows(func: &IrFunction, scope: &Scope) -> Vec<ParamBorrow> {
    let inferred = infer_function_borrows(func, scope);
    func.params.iter().zip(inferred)
        .map(|(p, b)| if is_value_ty(&p.ty) { b } else { ParamBorrow::Own })
        .collect()
}
