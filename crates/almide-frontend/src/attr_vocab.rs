//! E053: an attribute the compiler does not read.
//!
//! Attributes used to be silently dropped. `@deprecatd(since = 3, ...)` — one
//! transposed letter — parsed, checked, compiled, and told nobody, while the
//! author believed callers were being warned. For a language whose writers are
//! largely models, a construct that accepts anything and reports nothing is
//! the worst possible surface: there is no signal to learn from.
//!
//! This is a WARNING, not an error, on purpose. Rejecting outright would break
//! every file carrying an attribute from some older tool, and the language now
//! has the machinery to do that properly instead: promote it to an error at a
//! future dialect epoch, with `proofs/dialect-epochs.toml` recording the break
//! and E052-style migration telling each caller what to write.

use almide_base::diagnostic::Diagnostic;
use almide_lang::ast;

/// Every attribute name the compiler reads somewhere. Adding an attribute
/// means adding it here — the list is what makes "unknown" mean anything.
///
/// Ordered by area so a reader can tell what each group is for; the lookup
/// is a linear scan over a handful of entries and never shows up in a
/// profile.
pub const KNOWN_ATTRS: &[&str] = &[
    // Foreign bodies and symbols
    "extern",
    "export",
    "intrinsic",
    "wasm_intrinsic",
    "inline_rust",
    // The bounded profile (ALS §B, E070–E078)
    "bounded",
    // Ownership / calling convention
    "mutating",
    "consume",
    "borrow_ref",
    // May take over a dying source's storage (crates/almide-types: `reuse`)
    "reuse",
    // A self-hosted body names the stdlib fn it implements (crates/almide-types: `implements`)
    "implements",
    // A parameter that runs concurrently (ADR-0020 §3.1, E008)
    "concurrent",
    // A stdlib fn whose effect on process-local state a sibling fan arm could
    // read (ADR-0024 D3): a fan reaching one runs its arms sequentially
    "ordered",
    // The empty effect set, checked (E092, #3250)
    "pure",
    // A stdlib fn's effect categories when they differ from its module's
    // default (ADR-0026 D2 leaves, #4155; crates/almide-ir `effect`)
    "category",
    // Outside the memory-safety guarantee (#4008): `--profile critical`
    // refuses a call (E100); crates/almide-types `memory_unsafe_stdlib_fns`
    "memory_unsafe",
    // A foreign Region's length and release function on its `@extern`
    // declaration (ADR-0031 D2, D6): check/extern_abi_check.rs validates it,
    // the native @extern(c) glue builds the Region from it
    "region",
    // Signals or probes a pid (#4125): `[permissions] proc` bounds it as the
    // `kill` command; crates/almide-types `signals_process_stdlib_fns`
    "signals_process",
    // Decodes untrusted input into its `Decode`-bounded type parameters
    // (row 22, C-436): each must bind an input type — a `: Decode` record
    // that is not also `Encode` — or the call is E110 (crates/almide-frontend
    // check/input_sink.rs)
    "input_sink",
    // Scheduling and placement
    "schedule",
    "location",
    // Compiler-internal marks
    "derived",
    // The derived `T.decode`'s key order (#1679) — read by the native
    // DecodeSlotHintPass; stamped next to `derived` by the codec derive.
    "codec_slots",
    "main",
    // The stability surface
    "deprecated",
    "dialect",
    // Fusion-rule declarations. Read by TWO build-time generators sharing
    // buildscript/fusion_parse.rs::extract_rewrites — forward,
    // crates/almide-egg-lab/build.rs emits egg::rewrite! rules; reverse,
    // crates/almide-codegen/build.rs emits the committed desugar fallback
    // (generated/matrix_desugar_gen.rs). Both are text scanners over
    // stdlib/matrix.almd, not AST-attribute consumers, which is why a
    // `attr == "rewrite"` grep over crates/ finds nothing (almide#1450).
    "rewrite",
];

/// Attributes the grammar accepts and NO code reads.
///
/// Kept out of `KNOWN_ATTRS` so the distinction stays visible, and kept out
/// of the warning so the existing corpus is not noisy — but written down,
/// because "accepted and inert" is exactly the state this module exists to
/// make impossible to hold accidentally.
///
/// Currently empty. `@rewrite` sat here from this check's first corpus run
/// until almide#1450 corrected the record: the attribute has been read since
/// 6d2bee82f retired `MatrixFusionPass` for the egg saturation driver, and
/// 0fbfb6c29 added the reverse desugar generator — both at build time, so no
/// runtime code ever matches on the name.
pub const DECLARED_BUT_UNREAD: &[&str] = &[];

/// Closest known attribute by edit distance, for the did-you-mean. `None`
/// when nothing is close enough to be worth suggesting — a wrong suggestion
/// is worse than none, because it reads as authoritative.
fn nearest(name: &str) -> Option<&'static str> {
    let mut best: Option<(usize, &'static str)> = None;
    for cand in KNOWN_ATTRS {
        let d = edit_distance(name, cand);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, cand));
        }
    }
    // A third of the name may differ; beyond that the "suggestion" is noise.
    best.filter(|(d, _)| *d * 3 <= name.len().max(3)).map(|(_, c)| c)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let sub = prev[j - 1] + usize::from(a[i - 1] != b[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

pub fn check_attrs(attrs: &[ast::Attribute], diagnostics: &mut Vec<Diagnostic>) {
    for attr in attrs {
        let name = attr.name.as_str();
        if KNOWN_ATTRS.contains(&name) || DECLARED_BUT_UNREAD.contains(&name) {
            continue;
        }
        let hint = match nearest(name) {
            Some(c) => format!(
                "Did you mean `@{c}`? An attribute the compiler does not read is dropped, \
                 so nothing it was meant to do happens."
            ),
            None => format!(
                "Almide reads these attributes: {}. An attribute outside that list is dropped, \
                 so nothing it was meant to do happens.",
                KNOWN_ATTRS.join(", ")
            ),
        };
        let mut diag =
            Diagnostic::warning(format!("unknown attribute `@{name}`"), hint, format!("@{name}"))
                .with_code("E053");
        if let Some(s) = &attr.span {
            diag.line = Some(s.line);
            diag.col = Some(s.col);
        }
        diagnostics.push(diag);
    }
}

// ── Record-field attributes (#4108) ───────────────────────────────
//
// A field attribute is a separate vocabulary from a fn attribute: the two
// positions mean different things, and `@skip` on a fn would be as wrong as
// `@pure` on a field. An unknown FIELD attribute is an ERROR, not a warning
// (dialect epoch 15): in this position a typo means "the author believes a
// field is protected and it is not" — `@serde(skip) is_admin: Bool` used to
// compile, and the decoder still set `is_admin` from the request body.

/// What the Codec derive does with a field: read it from input (`decode`),
/// write it to output (`encode`). The ONE declaration both derive halves and
/// the family gate read (`tests::the_codec_skip_family_is_complete`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecFieldUse {
    pub decoded: bool,
    pub encoded: bool,
}

/// The Codec field-skip family {decode, encode} × {kept, skipped}: one row per
/// attribute. A field with none of these is decoded and encoded. The
/// intentional omission is the output-only cell (`@skip_encode`): hiding a
/// field from output is the `Secret[A]` type or a separate output type.
pub const CODEC_FIELD_SKIPS: &[(&str, CodecFieldUse)] = &[
    ("skip", CodecFieldUse { decoded: false, encoded: false }),
    ("skip_decode", CodecFieldUse { decoded: false, encoded: true }),
];

/// Every attribute name the compiler reads on a record field.
pub const KNOWN_FIELD_ATTRS: &[&str] = &[
    // WGSL stage I/O (crates/almide-codegen emit_wgsl)
    "builtin",
    "location",
    // The Codec field-skip family (CODEC_FIELD_SKIPS)
    "skip",
    "skip_decode",
];

/// The derive's view of one field. `ALMIDE_CONTROL_IGNORE_SKIP_DECODE=1` is
/// the negative control of the #4108 matrix gate: it makes the derive read a
/// `@skip_decode` field from input again, which must flip the fixture's
/// `admin=false` cell to `admin=true` and fail it. Never set it otherwise.
pub fn codec_field_use(attrs: &[ast::Attribute]) -> CodecFieldUse {
    let ignore_skip_decode = almide_base::env::flag("ALMIDE_CONTROL_IGNORE_SKIP_DECODE");
    let mut out = CodecFieldUse { decoded: true, encoded: true };
    for a in attrs {
        if let Some((name, u)) = CODEC_FIELD_SKIPS.iter().find(|(n, _)| *n == a.name.as_str()) {
            if ignore_skip_decode && *name == "skip_decode" { continue; }
            out.decoded &= u.decoded;
            out.encoded &= u.encoded;
        }
    }
    out
}

/// The spellings other ecosystems use for a field rule, mapped to what Almide
/// writes instead. Matched on the attribute name, and for `@serde(...)` /
/// `@json(...)` on the argument text too.
fn field_attr_hint(attr: &ast::Attribute) -> Option<&'static str> {
    let arg_text: Vec<String> = attr.args.iter().map(|a| {
        let v = match &a.value {
            ast::AttrValue::String { value } => value.clone(),
            ast::AttrValue::Ident { name } => name.to_string(),
            _ => String::new(),
        };
        format!("{}{}", a.name.map(|n| n.to_string()).unwrap_or_default(), v)
    }).collect();
    let mentions = |s: &str| arg_text.iter().any(|t| t.contains(s));
    const USE_SKIP: &str = "Did you mean `@skip`? It keeps the field out of both `T.decode` and `T.encode`; the field needs a default (`= value`).";
    const USE_SKIP_DECODE: &str = "Did you mean `@skip_decode`? It keeps the field out of `T.decode` (input can never set it) and still writes it in `T.encode`; the field needs a default (`= value`).";
    const USE_SECRET: &str = "Almide hides a value with its TYPE, not a field attribute: write the field as `Secret[String]` (`import secret`); it prints as `Secret(***)` everywhere and `secret.reveal(x)` reads it.";
    match attr.name.as_str() {
        "serde" | "json" | "codec" if mentions("skip_deserializing") || mentions("skip_decode") => Some(USE_SKIP_DECODE),
        "serde" | "json" | "codec" if mentions("skip") || mentions("ignore") => Some(USE_SKIP),
        "json_ignore" | "jsonignore" | "transient" | "ignore" | "skip_serializing" | "exclude" | "omit" => Some(USE_SKIP),
        "skip_deserializing" | "readonly" | "read_only" | "skip_deserialize" | "no_decode" => Some(USE_SKIP_DECODE),
        "redact" | "redacted" | "sensitive" | "secret" | "private" | "hidden" | "mask" => Some(USE_SECRET),
        _ => None,
    }
}

/// E053 for a record field's attributes, plus E104 for a skip attribute on a
/// field without a default. `field` is the field's name, for the context line.
pub fn check_field_attrs(field: &ast::FieldType, diagnostics: &mut Vec<Diagnostic>) {
    for attr in &field.attrs {
        let name = attr.name.as_str();
        if KNOWN_FIELD_ATTRS.contains(&name) { continue; }
        let hint = match field_attr_hint(attr) {
            Some(h) => h.to_string(),
            None => {
                let close = KNOWN_FIELD_ATTRS.iter()
                    .map(|c| (edit_distance(name, c), *c))
                    .min()
                    .filter(|(d, _)| *d * 3 <= name.len().max(3));
                match close {
                    Some((_, c)) => format!("Did you mean `@{c}`? An attribute the compiler does not read on a record field would protect nothing."),
                    None => format!(
                        "A record field reads these attributes: {}. Any other attribute would protect nothing, so it is refused.",
                        KNOWN_FIELD_ATTRS.iter().map(|a| format!("@{a}")).collect::<Vec<_>>().join(", ")
                    ),
                }
            }
        };
        let mut diag = Diagnostic::error(
            format!("unknown attribute `@{name}` on record field '{}'", field.name),
            hint,
            format!("@{name} {}", field.name),
        ).with_code("E053");
        if let Some(s) = &attr.span { diag.line = Some(s.line); diag.col = Some(s.col); }
        diagnostics.push(diag);
    }
    if field.default.is_none()
        && let Some(attr) = field.attrs.iter().find(|a| CODEC_FIELD_SKIPS.iter().any(|(n, _)| *n == a.name.as_str()))
    {
        let name = attr.name.as_str();
        let mut diag = Diagnostic::error(
            format!("field '{}' is `@{name}` but has no default", field.name),
            format!(
                "A skipped field is never read from input, so `T.decode` needs a value for it. Give it one: `@{name} {}: <Type> = <default>`.",
                field.name
            ),
            format!("@{name} {}", field.name),
        ).with_code("E104");
        if let Some(s) = &attr.span { diag.line = Some(s.line); diag.col = Some(s.col); }
        diagnostics.push(diag);
    }
}

/// R4 of row 22 (C-435): on an INPUT type — `: Decode` and not `Encode` —
/// every field is input, so a field-skip attribute contradicts the
/// declaration: the field could never be set. E053, the field-attribute
/// family. `@skip` / `@skip_decode` stay for two-way types (C-428).
pub fn check_input_type_field_skips(type_name: &str, field: &ast::FieldType, diagnostics: &mut Vec<Diagnostic>) {
    let Some(attr) = field.attrs.iter().find(|a| CODEC_FIELD_SKIPS.iter().any(|(n, _)| *n == a.name.as_str())) else { return };
    let name = attr.name.as_str();
    let mut diag = Diagnostic::error(
        format!("field '{}' of input type '{type_name}' is `@{name}`, but every field of a `: Decode` type is input", field.name),
        format!(
            "An input type lists what the client may set. Move '{}' to the type you build from it, or declare '{type_name}' `: Codec` if it is also written out.",
            field.name
        ),
        format!("@{name} {}", field.name),
    ).with_code("E053");
    if let Some(s) = &attr.span { diag.line = Some(s.line); diag.col = Some(s.col); }
    diagnostics.push(diag);
}

/// Every record field an `ast::TypeExpr` declares: a record's own fields and
/// each variant case's record payload.
pub fn declared_fields(ty: &ast::TypeExpr) -> Vec<&ast::FieldType> {
    match ty {
        ast::TypeExpr::Record { fields } | ast::TypeExpr::OpenRecord { fields } => fields.iter().collect(),
        ast::TypeExpr::Variant { cases, .. } => cases.iter().flat_map(|c| match c {
            ast::VariantCase::Record { fields, .. } => fields.iter().collect::<Vec<_>>(),
            _ => Vec::new(),
        }).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_base::intern::sym;

    fn attrs(names: &[&str]) -> Vec<ast::Attribute> {
        names
            .iter()
            .map(|n| ast::Attribute { name: sym(n), args: vec![], span: None })
            .collect()
    }

    fn diags(names: &[&str]) -> Vec<Diagnostic> {
        let mut d = Vec::new();
        check_attrs(&attrs(names), &mut d);
        d
    }

    #[test]
    fn every_known_attribute_is_silent() {
        assert!(diags(KNOWN_ATTRS).is_empty(), "a known attribute must never warn");
    }

    /// The two lists must stay disjoint: an attribute cannot be both read and
    /// unread, and a name drifting into both would make the second list stop
    /// meaning anything.
    #[test]
    fn the_read_and_unread_vocabularies_are_disjoint() {
        for a in DECLARED_BUT_UNREAD {
            assert!(!KNOWN_ATTRS.contains(a), "`{a}` is in both vocabularies");
        }
        assert!(diags(DECLARED_BUT_UNREAD).is_empty(), "an accepted-but-inert attribute must not warn");
    }

    #[test]
    fn an_unknown_attribute_warns() {
        let d = diags(&["totally_made_up_thing"]);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code, Some("E053"));
    }

    /// The case this exists for: one transposed letter in a deprecation,
    /// which used to compile silently while telling no caller anything.
    #[test]
    fn a_near_miss_suggests_the_real_one() {
        let d = diags(&["deprecatd"]);
        assert_eq!(d.len(), 1);
        assert!(d[0].hint.contains("`@deprecated`"), "hint was: {}", d[0].hint);
    }

    /// A wrong suggestion reads as authoritative, so distant names get the
    /// vocabulary instead of a guess.
    fn suggests_nothing(name: &str) -> bool {
        !diags(&[name])[0].hint.contains("Did you mean")
    }

    #[test]
    fn a_distant_name_gets_the_vocabulary_not_a_guess() {
        assert!(suggests_nothing("qqqqqqqqqqqq"));
        assert!(diags(&["qqqqqqqqqqqq"])[0].hint.contains("deprecated"));
    }

    #[test]
    fn each_unknown_attribute_is_reported_once() {
        assert_eq!(diags(&["one_bogus", "another_bogus", "deprecated"]).len(), 2);
    }

    fn field(attrs: Vec<ast::Attribute>, has_default: bool) -> ast::FieldType {
        ast::FieldType {
            name: sym("is_admin"),
            ty: ast::TypeExpr::Simple { name: sym("Bool") },
            default: if has_default { Some(ast::Expr::new(ast::ExprId::default(), None, ast::ExprKind::Bool { value: false })) } else { None },
            alias: None,
            attrs,
            comments: vec![],
            trailing_comments: vec![],
        }
    }

    fn field_diags(attr: ast::Attribute, has_default: bool) -> Vec<Diagnostic> {
        let mut d = Vec::new();
        check_field_attrs(&field(vec![attr], has_default), &mut d);
        d
    }

    fn named(n: &str, args: &[&str]) -> ast::Attribute {
        ast::Attribute {
            name: sym(n),
            args: args.iter().map(|a| ast::AttrArg { name: None, value: ast::AttrValue::Ident { name: sym(a) } }).collect(),
            span: None,
        }
    }

    /// The field vocabulary is silent; anything else is an ERROR (epoch 15).
    #[test]
    fn an_unknown_field_attribute_is_an_error_with_the_almide_spelling() {
        for a in KNOWN_FIELD_ATTRS { assert!(field_diags(named(a, &[]), true).is_empty(), "@{a}"); }
        for (n, args, want) in [
            ("serde", &["skip"][..], "`@skip`"), ("json_ignore", &[][..], "`@skip`"), ("transient", &[][..], "`@skip`"),
            ("serde", &["skip_deserializing"][..], "`@skip_decode`"), ("readonly", &[][..], "`@skip_decode`"),
            ("redact", &[][..], "Secret[String]"), ("sensitive", &[][..], "Secret[String]"), ("secret", &[][..], "Secret[String]"),
        ] {
            let d = field_diags(named(n, args), true);
            assert_eq!(d.len(), 1, "@{n}");
            assert_eq!(d[0].code, Some("E053"));
            assert!(matches!(d[0].level, almide_base::diagnostic::Level::Error), "@{n} must be an error");
            assert!(d[0].hint.contains(want), "@{n}: hint was {}", d[0].hint);
        }
    }

    #[test]
    fn a_skip_without_a_default_is_e104() {
        for (n, _) in CODEC_FIELD_SKIPS {
            let d = field_diags(named(n, &[]), false);
            assert_eq!(d.len(), 1);
            assert_eq!(d[0].code, Some("E104"));
            assert!(field_diags(named(n, &[]), true).is_empty());
        }
    }

}
