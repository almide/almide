
/// One variant type's VALUE-MODEL layout. A v1 variant value is a record-like heap block
/// in the SAME uniform-i64-slot model records use (NOT v0's byte-packed layout — only the
/// OBSERVABLE output must match v0, never the internal bytes): `slot 0` holds the tag and
/// `slots 1..` hold the ACTIVE constructor's fields. `slot_count` is `1 + max arity over
/// all cases`, so EVERY constructor of the type occupies an identically sized block — a
/// uniform alloc and a sound `==` over the whole block, the v1 analogue of v0's
/// max-payload padding (`variant_alloc_size`).
#[derive(Clone, Debug)]
pub struct VariantLayout {
    pub generics: Vec<almide_lang::intern::Sym>,
    /// Indexed by tag (`cases[t].tag == t`).
    pub cases: Vec<VariantCaseLayout>,
    pub slot_count: usize,
}

impl VariantLayout {
    /// The case whose constructor is `ctor`, if any.
    pub fn case_by_ctor(&self, ctor: &str) -> Option<&VariantCaseLayout> {
        self.cases.iter().find(|c| c.ctor.as_str() == ctor)
    }
}

/// Recursively replace a bare generic-parameter reference (`Ty::Named(p, [])` where `p` is a
/// key of `subst`) with its concrete binding — the DECLARATION-time field type of a generic
/// variant (`type Either[L,R] = Left(L) | Right(R)`) stores `L`/`R` verbatim as `Named("L",[])`/
/// `Named("R",[])` (confirmed via debug trace, NOT `Ty::TypeVar`), so heap/flat classification
/// over the RAW registry entry is blind to any concrete instantiation. Recurses into `Named`'s
/// own args and `Applied`'s args (a generic parameter could itself appear nested, e.g.
/// `List[L]`) so a partially-generic composite field also resolves correctly.
fn substitute_generic_ty(ty: &Ty, subst: &HashMap<almide_lang::intern::Sym, Ty>) -> Ty {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        Ty::Named(n, args) if args.is_empty() => {
            subst.get(n).cloned().unwrap_or_else(|| ty.clone())
        }
        Ty::Named(n, args) => {
            Ty::Named(*n, args.iter().map(|a| substitute_generic_ty(a, subst)).collect())
        }
        Ty::Applied(TypeConstructorId::UserDefined(n), args) => Ty::Applied(
            TypeConstructorId::UserDefined(n.clone()),
            args.iter().map(|a| substitute_generic_ty(a, subst)).collect(),
        ),
        Ty::Applied(c, args) => {
            Ty::Applied(c.clone(), args.iter().map(|a| substitute_generic_ty(a, subst)).collect())
        }
        _ => ty.clone(),
    }
}

/// A WASM-identifier-safe, unique suffix for a generic variant instantiation (`Either` +
/// `[Int, String]` → `"Either_Int_String"`) — the name of the PER-INSTANTIATION drop function
/// (`$__drop_<this>`/`$__drop_list_<this>`) generated for it, distinct from the bare generic
/// name so two different instantiations of the same type (were the corpus ever to use both)
/// never collide on one ambiguous function. `None` for an arg shape not confidently nameable
/// here (a nested generic instantiation, a tuple, …) — the caller declines (stays walled)
/// rather than guess a name that could collide or misrender.
/// The bare Almide SOURCE spelling of a scalar `Ty` — the set both the instantiation-name
/// mangler and the shadow-type-declaration renderer (`generate_generic_variant_instantiation_
/// sources`, drop_sources.rs) treat as safely nameable/renderable. Kept as ONE shared list so
/// the two never drift apart (a type nameable-but-not-renderable, or vice versa, would break the
/// admission⟹generation invariant `is_rich_variant_ty` depends on).
pub fn generic_variant_instantiation_scalar_name(ty: &Ty) -> Option<&'static str> {
    match ty {
        Ty::Int => Some("Int"),
        Ty::Float => Some("Float"),
        Ty::Bool => Some("Bool"),
        Ty::String => Some("String"),
        Ty::Int8 => Some("Int8"),
        Ty::Int16 => Some("Int16"),
        Ty::Int32 => Some("Int32"),
        Ty::Int64 => Some("Int64"),
        Ty::UInt8 => Some("UInt8"),
        Ty::UInt16 => Some("UInt16"),
        Ty::UInt32 => Some("UInt32"),
        Ty::UInt64 => Some("UInt64"),
        Ty::Float32 => Some("Float32"),
        Ty::Float64 => Some("Float64"),
        _ => None,
    }
}

pub fn generic_variant_instantiation_name(base: &str, args: &[Ty]) -> Option<String> {
    let mut out = base.to_string();
    for a in args {
        let piece = generic_variant_instantiation_scalar_name(a)?;
        out.push('_');
        out.push_str(piece);
    }
    Some(out)
}

/// The variant-type sibling of [`RecordLayouts`]: type NAME → its [`VariantLayout`], plus a
/// constructor-name → owning-type reverse index (a `Lit(7)` constructor expression carries
/// its ctor name; this resolves the variant type the way v0's `find_variant_tag_by_ctor`
/// fallback does). Threaded into lowering alongside `record_layouts` so a variant
/// construct / `match` can find its tag + field layout. Empty when lowering without a type
/// registry — a variant value then stays walled (the pre-ADT-brick status quo).
#[derive(Clone, Debug, Default)]
pub struct VariantLayouts {
    pub by_type: HashMap<String, VariantLayout>,
    pub ctor_to_type: HashMap<String, String>,
    /// Record-variant field DEFAULT exprs (`Rect { color: String = "" }`), keyed
    /// `ctor → field → expr` — consulted by the ctor builder when a literal OMITS a
    /// defaulted field (v0 fills the default at construction; leaving the slot would be
    /// garbage, and declining walled the whole default_fields family).
    pub ctor_field_defaults: HashMap<String, HashMap<String, almide_ir::IrExpr>>,
}

impl VariantLayouts {
    /// Resolve a constructor name to its owning type's name + layout + the specific case.
    pub fn lookup_ctor(&self, ctor: &str) -> Option<(&str, &VariantLayout, &VariantCaseLayout)> {
        let ty = self.ctor_to_type.get(ctor)?;
        let layout = self.by_type.get(ty)?;
        let case = layout.case_by_ctor(ctor)?;
        Some((ty.as_str(), layout, case))
    }

    /// The CORE of [`Self::needs_recursive_drop`], factored out so an INSTANTIATED (generic-
    /// substituted) case list can share the exact same classification as the raw registry entry
    /// — the two must never disagree (a false "doesn't need recursion" verdict on a heap field
    /// is a silent leak, not a wall).
    fn cases_need_recursive_drop(
        &self,
        cases: &[VariantCaseLayout],
        is_record: &dyn Fn(&str) -> bool,
    ) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        // Mirrors the generator's `variant_needs_recursive_drop`: a nested-variant field (the
        // original rule) OR heap fields the generated drop can ALL free (String / List[scalar] /
        // List[variant] / List[String] (per-element via `__drop_list_str`) / a RECORD — via
        // `$__drop_<R>` or a scalar-only record's flat rc_dec). The `is_record` predicate is
        // supplied by the caller (LowerCtx checks its record registry).
        let supported_heap = |t: &Ty| -> bool {
            self.field_is_variant(t)
                || matches!(t, Ty::Named(n, _) if is_record(n.as_str()))
                || matches!(t, Ty::String)
                // A CLOSURE field — freed via `__drop_closure` (mirrors the generator).
                || matches!(t, Ty::Fn { .. })
                || matches!(t, Ty::Applied(TypeConstructorId::List, a)
                    if a.len() == 1
                        && (!is_heap_ty(&a[0])
                            || matches!(a[0], Ty::String)
                            || self.field_is_variant(&a[0])))
                || matches!(t, Ty::Applied(TypeConstructorId::Option, a)
                    if a.len() == 1 && !is_heap_ty(&a[0]))
        };
        let mut any_heap = false;
        let mut all_supported = true;
        let mut has_variant_field = false;
        for c in cases {
            for (_, ty) in &c.fields {
                if self.field_is_variant(ty) {
                    has_variant_field = true;
                }
                if is_heap_ty(ty) {
                    any_heap = true;
                    if !supported_heap(ty) {
                        all_supported = false;
                    }
                }
            }
        }
        has_variant_field || (any_heap && all_supported)
    }

    /// Does the variant type `type_name` need the RECURSIVE [`Op::DropVariant`] (the generated
    /// `$__drop_<ty>`) — i.e. does some ctor field hold another user variant whose flat free would
    /// leak its children? A String-only-field variant uses the masked `DropListStr` instead (ADT
    /// brick 5a/5c). This is the lowering-side mirror of
    /// [`crate::lower::variant_needs_recursive_drop`], computed from the registry's field Tys.
    /// UNSUBSTITUTED: for a GENERIC variant this reads the raw declaration (type-parameter
    /// placeholders, never a concrete instantiation) — see [`Self::instantiated_needs_recursive_
    /// drop`] for the instantiation-aware sibling a `List[<generic variant>]` element check needs.
    pub fn needs_recursive_drop(&self, type_name: &str, is_record: &dyn Fn(&str) -> bool) -> bool {
        let Some(layout) = self.by_type.get(type_name) else { return false };
        self.cases_need_recursive_drop(&layout.cases, is_record)
    }

    /// Substitute a generic variant's DECLARED field types (`Left(L) | Right(R)` → `L`/`R` as
    /// bare `Ty::Named(sym,[])` placeholders, confirmed via debug trace — never `Ty::TypeVar`)
    /// with the CONCRETE type args at one instantiation site (`Either[Int,String]`'s `[Int,
    /// String]`, zipped positionally against `layout.generics`). A NON-generic variant (`layout.
    /// generics.is_empty()`) returns its cases UNCHANGED (zero-cost passthrough, no behavior
    /// change for the entire existing non-generic corpus). `None` on an arity mismatch (the
    /// checker guarantees this never happens for a well-typed program, but a mismatched
    /// registry/call-site pairing declines rather than substituting garbage).
    fn instantiated_cases(&self, type_name: &str, args: &[Ty]) -> Option<Vec<VariantCaseLayout>> {
        let layout = self.by_type.get(type_name)?;
        if layout.generics.is_empty() {
            return Some(layout.cases.clone());
        }
        if layout.generics.len() != args.len() {
            return None;
        }
        let subst: HashMap<almide_lang::intern::Sym, Ty> =
            layout.generics.iter().copied().zip(args.iter().cloned()).collect();
        Some(
            layout
                .cases
                .iter()
                .map(|c| VariantCaseLayout {
                    ctor: c.ctor,
                    tag: c.tag,
                    fields: c
                        .fields
                        .iter()
                        .map(|(n, t)| (*n, substitute_generic_ty(t, &subst)))
                        .collect(),
                })
                .collect(),
        )
    }

    /// The instantiation-aware sibling of [`Self::needs_recursive_drop`] — substitutes generic
    /// field types with `args` BEFORE classifying, so `Either[Int,String]`'s `Right(String)` case
    /// is correctly seen as heap (unlike the raw registry's unresolved `Right(R)`). Identical to
    /// `needs_recursive_drop` for a non-generic type (args ignored via `instantiated_cases`'s
    /// passthrough).
    pub fn instantiated_needs_recursive_drop(
        &self,
        type_name: &str,
        args: &[Ty],
        is_record: &dyn Fn(&str) -> bool,
    ) -> bool {
        match self.instantiated_cases(type_name, args) {
            Some(cases) => self.cases_need_recursive_drop(&cases, is_record),
            None => false,
        }
    }

    /// Extract `(bare type name, concrete type args)` from a variant-type reference — the
    /// SHARED match arms `is_flat_variant_ty`/`is_rich_variant_ty`/`field_variant_name` each
    /// duplicated (discarding the args); factored out so the instantiation-aware paths can see
    /// both halves. `Ty::Named(n, args)` carries a GENERIC variant's concrete instantiation args
    /// at a USE site (`Either[Int,String]` → `Named("Either", [Int, String])`, confirmed via
    /// debug trace) — `args` is empty for a non-generic reference or an unresolved bare mention.
    fn variant_name_and_args(ty: &Ty) -> Option<(&str, &[Ty])> {
        use almide_lang::types::constructor::TypeConstructorId;
        match ty {
            Ty::Named(n, args) => Some((n.as_str(), args.as_slice())),
            Ty::Variant { name, .. } => Some((name.as_str(), &[])),
            Ty::Applied(TypeConstructorId::UserDefined(n), args) => Some((n.as_str(), args.as_slice())),
            _ => None,
        }
    }

    /// Is `ty` a registry variant ALL of whose constructors have ONLY scalar fields — i.e. a FLAT
    /// tag-block with NO heap slot (a nullary enum like `Capability`, or a scalar-payload variant)?
    /// Such a block is a single allocation freed by one `prim.rc_dec`, so a `List[flat-variant]`
    /// drops correctly via the per-element-`rc_dec` `__drop_list_str` (each element + the list block),
    /// the SAME flat shape as a `List[String]`. A variant carrying a `String`/nested/`List` field is
    /// NOT flat (its block owns an inner handle a flat `rc_dec` would leak) → `false` (stays walled).
    /// Substitutes generic field types against `ty`'s own instantiation args first (a no-op for a
    /// non-generic variant), so a generic instantiated with an all-scalar arg set (`Pair[Int,Int]`)
    /// is correctly flat while one with a heap arg (`Either[Int,String]`) correctly is not.
    pub fn is_flat_variant_ty(&self, ty: &Ty) -> bool {
        let Some((n, args)) = Self::variant_name_and_args(ty) else { return false };
        match self.instantiated_cases(n, args) {
            Some(cases) => cases.iter().all(|c| c.fields.iter().all(|(_, fty)| !is_heap_ty(fty))),
            None => false,
        }
    }

    /// Is `ty` a RICH (recursive-drop) registry variant — a user variant for which `$__drop_<V>` and
    /// `$__drop_list_<V>` are generated (some ctor holds a nested user variant whose flat free would
    /// leak its children)? This is the lowering-side gate for admitting a `List[<rich variant>]`
    /// element (the wasm `Instr` accumulator) — its drop routes to `$__drop_list_<V>` via
    /// `variant_drop_handles`. Mirrors [`crate::lower::variant_needs_recursive_drop`] (the generator's
    /// gate) so the two never disagree: a variant admitted here ALWAYS has a generated `$__drop_list_<V>`.
    ///
    /// For a GENERIC variant instantiated with concrete args (`Either[Int,String]`), the returned
    /// name is the INSTANTIATION-SPECIFIC one (`generic_variant_instantiation_name`, e.g.
    /// `"Either_Int_String"`) rather than the bare generic name — a distinct `$__drop_list_<this>`
    /// is generated per instantiation actually used (see `discover_generic_variant_list_
    /// instantiations` in drop_sources.rs), since a single shared function could not correctly
    /// serve two instantiations with DIFFERENT per-slot heap-ness. `None` if the args aren't a
    /// confidently nameable shape (declines / stays walled rather than risk a colliding name) or
    /// this specific instantiation doesn't actually need recursive drop.
    pub fn is_rich_variant_ty(&self, ty: &Ty, is_record: &dyn Fn(&str) -> bool) -> Option<String> {
        let (n, args) = Self::variant_name_and_args(ty)?;
        if !self.by_type.contains_key(n) {
            return None;
        }
        // A NON-generic variant element admits the SAME record-field widening the drop
        // GENERATOR uses (`variant_needs_recursive_drop` counts record fields via
        // `all_record_names`) — admission ⊆ generation stays intact because both sides now
        // ask the same question: `$__drop_<V>`/`$__drop_list_<V>` free a record field via
        // `$__drop_<R>` / a scalar-only record's flat rc_dec (the drop generator's field
        // loop), so a record-field variant list (`List[Shape]`, `Label { at: Point }`) is
        // freed exactly. The GENERIC arm below keeps the `|_| false` narrowing (shadow-type
        // generation covers no record fields).
        if args.is_empty() {
            return self
                .needs_recursive_drop(n, is_record)
                .then(|| n.to_string());
        }
        let inst_name = generic_variant_instantiation_name(n, args)?;
        // ADMISSION must never outrun GENERATION: the shadow `type <inst_name> = ...` +
        // `$__drop_<inst_name>` source text (`generate_generic_variant_instantiation_sources`,
        // drop_sources.rs) can only render a field whose SUBSTITUTED type is one of the scalars
        // `generic_variant_instantiation_name` itself already supports, or another ALREADY-
        // DECLARED (non-generic) user variant referenced by its real bare name. A field type
        // outside that set (e.g. a generic field like `Left(List[L])` — Either's OWN fields
        // happen to be bare type params, so this never fires for it, but a future generic
        // variant might declare a composite field) declines the WHOLE instantiation here, so a
        // "yes" from this method is ALWAYS backed by real generated source — never a dangling
        // `$__drop_list_<inst_name>` call (the exact class of bug this campaign nearly shipped
        // once already, this session, on a different wall).
        let cases = self.instantiated_cases(n, args)?;
        if !cases.iter().all(|c| {
            c.fields.iter().all(|(_, fty)| {
                generic_variant_instantiation_scalar_name(fty).is_some() || self.field_is_variant(fty)
            })
        }) {
            return None;
        }
        self.instantiated_needs_recursive_drop(n, args, &|_| false)
            .then_some(inst_name)
    }

    /// Is `ty` one of the variant types in this registry (a nested-variant ctor field)?
    pub fn field_is_variant(&self, ty: &Ty) -> bool {
        use almide_lang::types::constructor::TypeConstructorId;
        let n = match ty {
            Ty::Named(n, _) => n.as_str(),
            Ty::Variant { name, .. } => name.as_str(),
            Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.as_str(),
            _ => return false,
        };
        self.by_type.contains_key(n)
    }

    /// The variant type NAME of `ty` if it is a registry variant (the recursion / construct target).
    pub fn field_variant_name(&self, ty: &Ty) -> Option<String> {
        use almide_lang::types::constructor::TypeConstructorId;
        let n = match ty {
            Ty::Named(n, _) => n.as_str().to_string(),
            Ty::Variant { name, .. } => name.as_str().to_string(),
            Ty::Applied(TypeConstructorId::UserDefined(n), _) => n.clone(),
            _ => return None,
        };
        self.by_type.contains_key(&n).then_some(n)
    }
}

/// Is `ty` the dynamic `Value` type (the Codec data model)? Its scope-end drop is the
/// runtime-tag-dispatched [`Op::DropValue`], since a heap-payload Value (Str/Array/Object) owns a
/// handle the flat `Drop` would leak.
pub fn is_value_ty(ty: &Ty) -> bool {
    match ty {
        Ty::Named(name, _) => name.as_str() == "Value",
        Ty::Variant { name, .. } => name.as_str() == "Value",
        _ => false,
    }
}

/// Does `ty` CONTAIN a function type anywhere (a `Ty::Fn`, or a List/Option/etc. OF functions —
/// `List[(Int) -> Int]`)? A self-host list combinator over such an argument (`list.map(fns, …)`
/// where `fns: List[(Int)->Int]`) cannot faithfully fill its result (the v1 model has no
/// representation for a list of closures), so the result is empty/garbage and must NOT be treated
/// as a real `materialized_lists` block (a direct `xs[i]` over it would trap on cap 0).
pub(crate) fn ty_contains_fn(ty: &Ty) -> bool {
    match ty {
        Ty::Fn { .. } => true,
        Ty::Applied(_, args) => args.iter().any(ty_contains_fn),
        Ty::Tuple(tys) => tys.iter().any(ty_contains_fn),
        _ => false,
    }
}

/// Is `ty` a `List[T]` whose element `T` is a SCALAR (non-heap) type (`List[Int/Float/Bool]`)?
/// Such a list's slots are plain i64 values — a direct `xs[i]` reads one with `Load { width: 8 }`,
/// and `__list_concat` byte-copies them with no ownership. The complement of `is_heap_elem_list_ty`
/// for the List constructor.
pub(crate) fn is_scalar_elem_list_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && !is_heap_ty(&a[0]))
}

/// Is `ty` a `List[T]` / `Option[T]` whose element `T` is itself a HEAP type (e.g. `List[String]`,
/// `Option[String]`)? Such a container OWNS its element(s) — it needs the recursive
/// [`Op::DropListStr`], not a flat drop. An `Option[String]` is physically a 0-or-1-element
/// `List[String]` (Machinery 2), so the SAME recursive free applies (len 0 frees nothing, len 1
/// frees the one element + the block).
/// A `List[List[String]]` — its element slots hold owned `List[String]` blocks (the csv `rows`
/// shape). Its scope-end drop must be [`Op::DropListListStr`] (the nested cell + row free); a flat
/// `DropListStr` (what `is_heap_elem_list_ty` would route it to, since List[List[String]] is also a
/// `List[heap]`) would only `rc_dec` each row HANDLE, leaking the cell Strings. So EVERY tracking
/// site checks this FIRST.
pub(crate) fn is_list_list_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    // A `List[Matrix]` (matrix.split_cols_even's result) is the SAME two-level shape: a
    // v1 Matrix IS a List[List[Float]] block whose slots hold owned row handles, so
    // `DropListListStr`'s per-element inner sweep (rc_dec each row + the matrix block,
    // then the outer block) is its exact recursive free — each row is a FLAT f64 block,
    // like a String. The flat `DropListStr` would leak every row.
    if matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1
            && matches!(&a[0], Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _)))
    {
        return true;
    }
    matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
            Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && matches!(b[0], Ty::String)))
}

/// An `Option[List[String]]` — the heap-accumulator fold's value (is_balanced's paren
/// stack). PHYSICALLY a 0/1-element `List[List[String]]`, so `DropListListStr`'s nested
/// sweep (per outer slot: rc_dec each inner cell String + the inner block, then the outer
/// block) is its exact recursive free — the flat `DropListStr` (`heap_elem_lists`) would
/// rc_dec only the inner-list HANDLE, leaking every stack String (a fold loop OOMs).
pub(crate) fn is_opt_list_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::Option, a) if a.len() == 1 && matches!(&a[0],
            Ty::Applied(TypeConstructorId::List, b) if b.len() == 1 && matches!(b[0], Ty::String)))
}

/// A `List[(String, String)]` — the `map.entries` / render_attrs shape. Each element is an owned
/// (String, String) TUPLE; its scope-end drop must be [`Op::DropListStrStr`] (per tuple: rc_dec BOTH
/// String slots, then the tuple, then the list). The flat `DropListStr` (`heap_elem_lists`) would
/// rc_dec only the tuple HANDLE — freeing the tuple block but LEAKING its two Strings (a render loop
/// OOMs). Checked BEFORE `is_heap_elem_list_ty` (which also matches this List type).
pub(crate) fn is_list_str_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    // BOTH pair sides must be single FLAT blocks — a String, a List[scalar] row
    // (list.zip_rc over matrix rows), or an all-scalar TUPLE (map.entries over the
    // hval-tuple flavor, C-039) — so DropListStrStr's two per-slot rc_decs are each
    // a FULL free. A rich payload (List[heap], record, Value) stays out (would leak).
    let flat = |t: &Ty| {
        matches!(t, Ty::String)
            || matches!(t, Ty::Applied(TypeConstructorId::List, b)
                if b.len() == 1 && !is_heap_ty(&b[0]))
            || matches!(t, Ty::Tuple(ts) if !ts.is_empty() && ts.iter().all(|c| !is_heap_ty(c)))
    };
    matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
            Ty::Tuple(tys) if tys.len() == 2 && flat(&tys[0]) && flat(&tys[1])))
}

/// A `List[(Int, String)]` — the `list.enumerate` result. Each element is an (Int @12 scalar, String
/// @20 heap) tuple; its scope-end drop must be the recursive `$__drop_list_int_str` (rc_dec each
/// tuple's String + block), routed via `variant_drop_handles="list_int_str"`. A flat `DropListStr`
/// would leak each tuple's String (a 10⁴ loop OOMs).
/// `Map[Int, String]` — the scalar-key / owned-heap-value map (self-host map_ivh).
pub fn is_map_ivh_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::Map, a)
            if a.len() == 2 && matches!(a[0], Ty::Int) && matches!(a[1], Ty::String))
}

/// `Map[String, List[scalar]]` — the String-key / FLAT-heap-value map (self-host
/// map_hval; a flat value block's rc_dec is its full free).
/// `Map[String, <Fn>]` — the mclo (closure-valued map) family: String keys +
/// closure-block values. CONSTRUCTION rides the handle-level `_hval` twins
/// (set/get/get_or store & share plain value handles — type-agnostic physics);
/// only the DROP differs: `$__drop_map_mclo` frees each value via
/// `__drop_closure` (the hval flat per-slot `rc_dec` would leak every captured
/// env slot — the `__drop_list_closure` leak class).
pub fn is_map_fn_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::Map, a)
            if a.len() == 2 && matches!(a[0], Ty::String) && matches!(a[1], Ty::Fn { .. }))
}

pub fn is_map_hval_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    // A FLAT heap value: a List[scalar] row OR an all-scalar tuple (`map.map(mi,
    // (v) => (v, v*v))` — C-039). Either way `$__drop_map_hval`'s rc_dec of all
    // 2n slots is the exact free (both value classes are single flat blocks).
    matches!(ty,
        Ty::Applied(TypeConstructorId::Map, a)
            if a.len() == 2 && matches!(a[0], Ty::String)
                && (matches!(&a[1],
                        Ty::Applied(TypeConstructorId::List, e) if e.len() == 1 && !is_heap_ty(&e[0]))
                    || matches!(&a[1],
                        Ty::Tuple(ts) if !ts.is_empty() && ts.iter().all(|c| !is_heap_ty(c)))))
}

/// `Map[String, Map[String, String]]` — the msv family (String keys, MAP values;
/// stdlib/map_msv.almd). Its drop must sweep each last-ref inner map's String slots
/// (`$__drop_map_msv`), so binds route it by type exactly like `is_map_hval_ty`.
pub fn is_map_msv_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2
        && matches!(a[0], Ty::String)
        && matches!(&a[1], Ty::Applied(TypeConstructorId::Map, b)
            if b.len() == 2 && matches!(b[0], Ty::String) && matches!(b[1], Ty::String)))
}

/// `Map[String, List[Option[Int]]]` — the mlo family (String keys, LIST-OF-OPTIONS values;
/// stdlib/map_mlo.almd — compound_repr_interp's `deep` literal). Its drop must sweep each
/// last-ref value list's Option-block slots (`$__drop_map_mlo`), exactly the msv discipline
/// with list slots instead of inner-map string slots.
pub fn is_map_mlo_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Map, a) if a.len() == 2
        && matches!(a[0], Ty::String)
        && matches!(&a[1], Ty::Applied(TypeConstructorId::List, b)
            if b.len() == 1
                && matches!(&b[0], Ty::Applied(TypeConstructorId::Option, o)
                    if o.len() == 1 && matches!(o[0], Ty::Int))))
}

pub(crate) fn is_list_int_str_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty,
        Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(&a[0],
            Ty::Tuple(tys) if tys.len() == 2 && matches!(tys[0], Ty::Int) && matches!(tys[1], Ty::String)))
}

/// `Result[Unit, _]` — the static type of an `effect fn … -> Unit` CALL (the auto-`?`
/// effect Result carrying no value). The v1 MIR pipeline lowers such an effect fn to a
/// VOID wasm function (no `func.ret`), so a call to it is an EFFECT statement, never a
/// scalar/heap value. Used to route a `Result[Unit, _]`-typed tail/value call to the
/// effect-call path instead of the scalar-call path (which would expect an i32 result
/// the void callee never produces — an invalid-wasm type mismatch).
pub(crate) fn is_unit_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2 && matches!(a[0], Ty::Unit))
}

/// A `Result[Value, String]` — the `ok(value.array(...))` shape. Its Ok payload is a dynamic Value
/// (freed RECURSIVELY via `$__drop_value`), its Err a String. Its scope-end drop must be
/// [`Op::DropResultValue`] (the tag-dispatched recursive free); a flat `DropListStr` would leak the
/// Ok Value's nested payload.
pub fn is_value_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2 && is_value_ty(&a[0]) && matches!(a[1], Ty::String))
}

/// `Result[(String, Int), String]` — the toml `parse_key_part` `ok((slice, pos))` shape. Its Ok
/// payload is a `(String, Int)` tuple (a heap String slot + a scalar Int slot), so both the producer
/// (`try_lower_result_str_int_ctor`) and the match-subject drop route it to `str_int_result_results`
/// (the recursive `Op::DropResultStrInt`), NOT the flat `heap_elem_lists`/`DropListStr` which would
/// leak the tuple's String.
pub fn is_str_int_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2
            && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                && matches!(ts[0], Ty::String) && matches!(ts[1], Ty::Int))
            && matches!(a[1], Ty::String))
}

/// `Result[(Value, Int), String]` — the toml `parse_val` `ok((value.…, pos))` shape. The Ok payload
/// is a `(Value, Int)` tuple (a dynamic-Value heap slot + a scalar Int); routed to
/// `value_int_result_results` (recursive `Op::DropResultValueInt` via `$__drop_value_tuple`).
pub fn is_value_int_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2
            && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                && is_value_ty(&ts[0]) && matches!(ts[1], Ty::Int))
            && matches!(a[1], Ty::String))
}

/// `Result[(List[String], Int), String]` — the toml `parse_key` / `parse_table_key` `ok((keys, pos))`
/// shape. The Ok-tuple's slot0 is a `List[String]`; routed to `list_str_int_result_results` (recursive
/// `Op::DropResultListStrInt`, which frees the inner List's element Strings).
pub fn is_list_str_int_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2
            && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                && matches!(&ts[0], Ty::Applied(TypeConstructorId::List, le)
                    if le.len() == 1 && matches!(le[0], Ty::String))
                && matches!(ts[1], Ty::Int))
            && matches!(a[1], Ty::String))
}

/// `Result[List[String], String]` — the `fs.list_dir` `ok([name,…])` shape (NO tuple, the DIRECT
/// list). The Ok payload @12 is a `List[String]`; routed to `list_str_result_results` (recursive
/// `Op::DropResultListStr`, which frees the inner List's element Strings + block). Distinct from
/// `is_list_str_int_result_ty` (that one's Ok is a `(List[String], Int)` tuple).
pub fn is_list_str_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2
            && matches!(&a[0], Ty::Applied(TypeConstructorId::List, le)
                if le.len() == 1 && matches!(le[0], Ty::String))
            && matches!(a[1], Ty::String))
}

/// `Result[(List[Value], Int), String]` — toml `collect_array_items`. The Ok-tuple's slot0 is a
/// `List[Value]`; routed to `list_value_int_result_results` (recursive `Op::DropResultListValueInt`,
/// freeing each element Value via `$__drop_list_value`).
pub fn is_list_value_int_result_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    matches!(ty, Ty::Applied(TypeConstructorId::Result, a)
        if a.len() == 2
            && matches!(&a[0], Ty::Tuple(ts) if ts.len() == 2
                && matches!(&ts[0], Ty::Applied(TypeConstructorId::List, le)
                    if le.len() == 1 && is_value_ty(&le[0]))
                && matches!(ts[1], Ty::Int))
            && matches!(a[1], Ty::String))
}

pub(crate) fn is_heap_elem_list_ty(ty: &Ty) -> bool {
    use almide_lang::types::constructor::TypeConstructorId;
    match ty {
        // A `Matrix` VALUE (the v1 value model): a List[List[Float]] block whose slots
        // hold owned row handles — each row a FLAT f64 block, so the per-slot-rc_dec
        // `DropListStr` is its exact recursive free (a Matrix drops like a List[String]).
        Ty::Matrix | Ty::Applied(TypeConstructorId::Matrix, _) => true,
        // `List[heap]` / `Option[heap]` / `Set[heap]` — heap element slots (DynListStr nested
        // ownership). A `Set[heap]` is physically a `List[heap]` of unique elements, so the SAME
        // recursive free applies (each owned element + the block).
        Ty::Applied(TypeConstructorId::List | TypeConstructorId::Option | TypeConstructorId::Set, args)
            if args.len() == 1 && is_heap_ty(&args[0]) =>
        {
            true
        }
        // `Result[_, heap-Err]` is physically the SAME DynListStr (the Ok/Err materialization reuses
        // it): `Err` owns the heap Err payload in slot 0 (len 1 → DropListStr frees it), `Ok` is
        // len 0 (frees nothing). So a Result value is dropped recursively, exactly like Option[heap].
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 && is_heap_ty(&args[1]) => {
            true
        }
        // `Map[heap, heap]` (e.g. `Map[String, String]`) — a DynListStr of INTERLEAVED key+value
        // String handles [k0,v0,k1,v1,...]; EVERY slot is a heap handle, so the uniform recursive
        // DropListStr frees all keys and values. (`len` = the slot count; map.len reads len/2.)
        Ty::Applied(TypeConstructorId::Map, args)
            if args.len() == 2 && is_heap_ty(&args[0]) && is_heap_ty(&args[1]) =>
        {
            true
        }
        _ => false,
    }
}

