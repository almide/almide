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
    /// borrow inference like structural records; user VARIANTs stay Own
    /// (conservative — variant borrowing is not generalized here). #647
    pub records: &'a HashSet<String>,
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
        is_borrow_eligible(ty, self.round.records)
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
                if mode != SlotMode::Consume && self.is_borrow_eligible(&arg.ty) { mode } else { SlotMode::Consume }
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
/// loop iterable, fold seed, method receiver, consuming call slot and result
/// position moves the value; so does any occurrence inside a closure (the
/// `move` capture takes it). A heap-typed field read straight off the param
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
        | Site::TupleIndex | Site::Index | Site::MapKeyed | Site::Deref | Site::Operand
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
    func.params.iter().map(|param| param_borrow(param, &uses, scope)).collect()
}

/// One param's mode from the body's occurrences of it.
fn param_borrow(param: &IrParam, uses: &UseSites, scope: &Scope) -> ParamBorrow {
    if !scope.is_borrow_eligible(&param.ty) {
        return ParamBorrow::Own;
    }
    // Explicit `mut` heap param → passed by mutable reference, and it is
    // authoritative: the checker (`validate_mut_args`) guarantees the caller
    // hands over a `var` binding, so the param IS a `&mut T` by construction
    // regardless of how the body uses it — it may mutate a *field* of it
    // (`list.push(b.xs, v)` on `mut b`, #703) or forward it to another `mut`
    // callee. Honor the keyword before the body policy (mirrors the
    // @intrinsic mut path; a primitive `mut x: Int` is filtered by the heap
    // guard above).
    if param.is_mut {
        return ParamBorrow::RefMut;
    }
    if uses.of(param.var).any(consumes) {
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
fn is_borrow_eligible(ty: &Ty, records: &HashSet<String>) -> bool {
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
