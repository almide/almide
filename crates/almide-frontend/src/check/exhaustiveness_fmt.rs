// Witness formatting for the exhaustiveness checker: the missing-pattern
// text and the arm templates a diagnostic suggests. `include!`d by
// exhaustiveness.rs (the 800-line file budget); it shares that module's scope.

// ────────────────────────────────────────────────
//  Formatting
// ────────────────────────────────────────────────

/// Compact witness text at a position of type `ty`. A record-payload variant is
/// written with its field names — the fields the witness pins, then `..` for
/// the rest (`Circle { r: 0, .. }`, or `Circle { .. }` when nothing is pinned)
/// — so the text is itself a pattern the user can write.
fn fmt_pat(pat: &Pat, ty: &Ty, env: &TypeEnv) -> String {
    match pat {
        Pat::Wild => "_".into(),
        Pat::Ctor(ctor, args) => {
            let name = ctor_name(ctor, ty);
            let sub_types = field_types(ctor, ty, env);
            let sub = |i: usize, a: &Pat| fmt_pat(a, sub_types.get(i).unwrap_or(&Ty::Unknown), env);
            if let Some(fields) = record_payload_fields(ctor, ty, env) {
                let mut parts: Vec<String> = args
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| !matches!(a, Pat::Wild))
                    .map(|(i, a)| {
                        let fname = fields.get(i).map_or_else(|| format!("_{i}"), |(n, _)| n.to_string());
                        format!("{fname}: {}", sub(i, a))
                    })
                    .collect();
                if parts.len() < fields.len() {
                    parts.push("..".into());
                }
                return format!("{} {{ {} }}", name, parts.join(", "));
            }
            if args.is_empty() {
                name
            } else {
                let inner: Vec<_> = args.iter().enumerate().map(|(i, a)| sub(i, a)).collect();
                if let CtorId::List { open, .. } = ctor {
                    return list_text(inner, *open);
                }
                if matches!(ctor, CtorId::Tuple) {
                    format!("({})", inner.join(", "))
                } else {
                    format!("{}({})", name, inner.join(", "))
                }
            }
        }
    }
}

/// A list pattern's text: `[a, b]`, or `[a, b, ..]` for the open (rest) form.
fn list_text(mut parts: Vec<String>, open: bool) -> String {
    if open {
        parts.push("..".into());
    }
    format!("[{}]", parts.join(", "))
}

/// Paste-ready arm template for a witness pattern. Unlike `fmt_pat`
/// (which emits `Node(_, _)` with wildcards), this produces
/// `Node(arg1, arg2) => _` with positional binding placeholders so the
/// LLM can copy the arm directly into the source. Field names are
/// reconstructed from the variant's `VariantPayload::Record` when
/// available, otherwise positional `argN`.
///
/// Nested witnesses (e.g. `Node(Node(_, _), Leaf)` on a recursive
/// `Tree = Leaf | Node(Tree, Tree)`) preserve their full structure —
/// the template recurses through inner constructors so the user sees
/// `Node(Node(arg1, arg2), Leaf) => _` rather than the flat
/// `Node(arg1, arg2) => _`. `argN` is a file-scope counter to keep
/// bindings unique across the nesting.
fn fmt_arm_template(pat: &Pat, subject_ty: &Ty, env: &TypeEnv) -> String {
    let mut counter = 1usize;
    let head = fmt_arm_head(pat, subject_ty, env, &mut counter);
    format!("{} => _", head)
}

fn fmt_arm_head(pat: &Pat, ty: &Ty, env: &TypeEnv, counter: &mut usize) -> String {
    match pat {
        Pat::Wild => next_arg_name(counter),
        Pat::Ctor(ctor, args) => fmt_ctor_arm_head(ctor, args, ty, env, counter),
    }
}

/// `argN`, bumping the shared counter.
///
/// Names only have to be unique within one emitted arm — a wildcard binding
/// cannot shadow anything at lint time, and the arm is paste fodder — so the
/// counter is not reset between rows.
fn next_arg_name(counter: &mut usize) -> String {
    let n = *counter;
    *counter += 1;
    format!("arg{n}")
}

/// How a constructor is written in an arm head: its name, and whether it takes
/// tuple or prefix-call syntax.
struct CtorSyntax {
    name: String,
    is_tuple: bool,
    is_prefix_call: bool,
}

/// The name a constructor is written with at a position of type `ty`: a plain
/// record's shape is written with the record type's own name.
fn ctor_name(ctor: &CtorId, ty: &Ty) -> String {
    match (ctor, ty) {
        (CtorId::Record, Ty::Named(n, _)) => n.to_string(),
        _ => ctor_syntax(ctor).name,
    }
}

fn ctor_syntax(ctor: &CtorId) -> CtorSyntax {
    let (name, is_tuple, is_prefix_call) = match ctor {
        CtorId::Variant(s) => (s.to_string(), false, true),
        CtorId::Some => ("some".into(), false, true),
        CtorId::None => ("none".into(), false, false),
        CtorId::Ok => ("ok".into(), false, true),
        CtorId::Err => ("err".into(), false, true),
        CtorId::True => ("true".into(), false, false),
        CtorId::False => ("false".into(), false, false),
        CtorId::Tuple => (String::new(), true, false),
        // The written name is the record TYPE's; callers that print a record
        // shape substitute it (`record_type_name`).
        CtorId::Record => (String::new(), false, true),
        CtorId::Lit(v) => (v.clone(), false, false),
        // Written by `list_text`; the name is the empty list's.
        CtorId::List { open, .. } => (list_text(vec![], *open), false, false),
    };
    CtorSyntax { name, is_tuple, is_prefix_call }
}

/// The field names of a record-payload variant case, in declaration order.
///
/// `None` for anything else — a tuple payload, a unit case, or a constructor
/// that is not a variant at all — because those bind positionally and have no
/// names to offer.
fn record_payload_field_names(ctor: &CtorId, ty: &Ty, env: &TypeEnv) -> Option<Vec<String>> {
    record_payload_fields(ctor, ty, env).map(|fs| fs.iter().map(|(n, _)| n.to_string()).collect())
}

/// The binding name for one wildcard sub-pattern of a constructor.
///
/// Single-field `Option`/`Result` payloads get the conventional `x` / `e` so the
/// paste-ready arm reads like the idiom (`some(x)`, `err(e)`) rather than a
/// generic `arg1`. A record payload uses the field's own name. Everything else
/// falls back to the counter.
fn wildcard_binding_name(
    ctor: &CtorId,
    arity: usize,
    index: usize,
    record_fields: &Option<Vec<String>>,
    counter: &mut usize,
) -> String {
    if arity == 1 {
        if matches!(ctor, CtorId::Some | CtorId::Ok) {
            return "x".to_string();
        }
        if matches!(ctor, CtorId::Err) {
            return "e".to_string();
        }
    }
    if let Some(fname) = record_fields.as_ref().and_then(|fs| fs.get(index)) {
        return fname.clone();
    }
    next_arg_name(counter)
}

fn fmt_ctor_arm_head(
    ctor: &CtorId,
    args: &[Pat],
    ty: &Ty,
    env: &TypeEnv,
    counter: &mut usize,
) -> String {
    let CtorSyntax { is_tuple, is_prefix_call, .. } = ctor_syntax(ctor);
    let name = ctor_name(ctor, ty);
    if args.is_empty() {
        return name;
    }
    let sub_types = field_types(ctor, ty, env);
    let record_fields = record_payload_field_names(ctor, ty, env);
    let parts: Vec<String> = args.iter().enumerate().map(|(i, arg)| {
        match arg {
            Pat::Wild => wildcard_binding_name(ctor, args.len(), i, &record_fields, counter),
            Pat::Ctor(_, _) => {
                let sub_ty = sub_types.get(i).cloned().unwrap_or(Ty::Unknown);
                let head = fmt_arm_head(arg, &sub_ty, env, counter);
                // A record payload binds by name: a pinned field is
                // `field: pattern`, never a bare positional pattern.
                match record_fields.as_ref().and_then(|fs| fs.get(i)) {
                    Some(fname) => format!("{fname}: {head}"),
                    Option::None => head,
                }
            }
        }
    }).collect();
    if let CtorId::List { open, .. } = ctor {
        return list_text(parts, *open);
    }
    if is_tuple {
        return format!("({})", parts.join(", "));
    }
    if !is_prefix_call {
        return name;
    }
    if record_fields.is_some() {
        return format!("{} {{ {} }}", name, parts.join(", "));
    }
    format!("{}({})", name, parts.join(", "))
}

/// Whether every row's head constructor belongs to the subject type's
/// constructor space. A foreign head (`ok(x)` over a record, `some(x)` over a
/// variant) is a pattern/type mismatch the checker has already reported;
/// coverage over it is meaningless, and reporting a missing arm on top of the
/// mismatch is a cascade that points at the wrong fix.
fn heads_fit(matrix: &[Vec<Pat>], ty: &Ty, env: &TypeEnv) -> bool {
    let set = ctor_set(ty, env);
    matrix.iter().all(|row| match row.first() {
        Some(Pat::Ctor(c, _)) => match &set {
            CtorSet::Finite(all) => all.contains(c),
            CtorSet::Single(one) => c == one,
            CtorSet::Infinite => matches!(c, CtorId::Lit(_)),
            CtorSet::List => matches!(c, CtorId::List { .. }),
            CtorSet::Opaque => true,
        },
        _ => true,
    })
}
