// E110 — an untrusted-input sink decodes only into an input type (row 22,
// C-436). `include!`d by post_solve_validation.rs; it shares that module's
// scope and imports.
//
// A mass-assignment bug needs an untrusted source AND a target type whose
// field set is wider than what that source may set. The source is declared
// on the sink: a fn marked `@input_sink` (`http.decode_json`) decodes
// untrusted input into its `Decode`-bounded type parameters. The target's
// surface is declared on the type: a `: Decode` type that is not also
// `Encode` is an INPUT type, every field of which is input. A type that is
// also sent out (`: Codec`) has an input surface equal to its output surface
// by accident — the client reads `User` from `GET /me` and posts it back —
// so binding one at a sink is E110. Nested user records are input too and
// must be input types themselves; a stdlib-declared type is a leaf value.
//
// Judged post-solve, because the type parameter is often pinned by a later
// use (`ok(u) => save(u)`), not at the call.

/// One call to an `@input_sink` fn, pending the post-solve input-type check.
#[derive(Debug, Clone)]
pub(crate) struct InputSinkSite {
    /// The sink as resolved (`http.decode_json`).
    pub callee: String,
    /// The binding of one of the sink's input type parameters (inference
    /// vars intact).
    pub ty: Ty,
    /// The fn whose body holds the call, as its callers resolve it, so a
    /// forwarding `[T: Decode]` wrapper is judged by its own declaration.
    pub caller: Option<Sym>,
    pub span: Option<ast::Span>,
}

/// Why a binding is not an input type.
enum NotInput {
    /// The type is also written out (`Codec` / `Encode`).
    SentOut { ty: String, proto: &'static str },
    /// A record nested in the input type is also written out.
    NestedSentOut { outer: String, field: String, inner: String, proto: &'static str },
    /// A type parameter of a caller that is not itself an input sink.
    Forwarded { tv: String, caller: String },
}

impl Checker {
    /// Record each input-type binding of a call to an `@input_sink` fn.
    pub(crate) fn defer_input_sink_checks(&mut self, name: &str, qualified: Option<&str>, bindings: &HashMap<Sym, Ty>) {
        let found = [Some(name), qualified].into_iter().flatten()
            .find_map(|k| self.env.input_sinks.get(&sym(k)).map(|tvs| (k.to_string(), tvs.clone())));
        let Some((callee, tvs)) = found else { return };
        let span = self.call_span_hint.or(self.current_span);
        let caller = self.current_fn.as_ref().map(|(k, _)| *k);
        for tv in tvs {
            if let Some(b) = bindings.get(&tv) {
                self.deferred_input_sink_checks.push(InputSinkSite {
                    callee: callee.clone(), ty: b.clone(), caller, span,
                });
            }
        }
    }

    /// E002's hint for `T.encode` on a decode-only type or `T.decode` on an
    /// encode-only one (C-435): the method belongs to the direction the type
    /// did not declare. `None` when that is not why it is missing.
    pub(crate) fn missing_codec_direction_hint(&self, name: Sym, method: &str) -> Option<String> {
        use crate::canonicalize::protocols::{builtin_protocol, codec_directions, ENCODE_PROTOCOL, DECODE_PROTOCOL};
        let bare = name.as_str().rsplit('.').next().unwrap_or(name.as_str());
        let declared: Vec<Sym> = [name, sym(bare)].iter()
            .filter_map(|n| self.env.type_protocols.get(n))
            .flatten().copied().collect();
        let (enc, dec) = codec_directions(&declared);
        if !enc && !dec {
            return None;
        }
        let in_dir = |p: &str| builtin_protocol(p).is_some_and(|bp| bp.method_names().iter().any(|m| m == method));
        if !enc && in_dir(ENCODE_PROTOCOL) {
            return Some(format!(
                "`{bare}` is `: Decode` only — an input type, read from input and never written out, so it has no `{method}`. Declare `type {bare}: Codec` if it is also sent out, or build the type you send from it."
            ));
        }
        if !dec && in_dir(DECODE_PROTOCOL) {
            return Some(format!(
                "`{bare}` is `: Encode` only — written out, never read from input, so it has no `{method}`. Declare `type {bare}: Codec` if it is also read, or decode into a `: Decode` input type."
            ));
        }
        None
    }

    fn validate_input_sinks(&mut self) {
        let sites = std::mem::take(&mut self.deferred_input_sink_checks);
        // The negative control of the row-22 gate (tests/input_sink_test.rs):
        // admit a type that is also sent out, so the over-post runs.
        let admit_sent_out = almide_base::env::flag("ALMIDE_CONTROL_ADMIT_CODEC_AT_INPUT");
        let mut reported: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
        for site in sites {
            let ty = resolve_ty(&site.ty, &self.uf);
            let Some(why) = self.not_an_input_type(&ty, site.caller) else { continue };
            if admit_sent_out && matches!(why, NotInput::SentOut { .. } | NotInput::NestedSentOut { .. }) {
                continue;
            }
            let key = site.span.map(|s| (s.line, s.col)).unwrap_or((0, 0));
            if !reported.insert(key) {
                continue;
            }
            let mut diag = self.input_sink_diagnostic(&site.callee, &ty, why).with_code("E110");
            diag.file = self.source_file.clone();
            if let Some(s) = site.span {
                diag.line = Some(s.line);
                diag.col = Some(s.col);
            }
            self.diagnostics.push(diag);
        }
    }

    /// `None` when `ty` is an input type (or not judgeable here: an
    /// unresolved var is E025's, `Unknown` is error recovery).
    fn not_an_input_type(&self, ty: &Ty, caller: Option<Sym>) -> Option<NotInput> {
        match ty {
            Ty::TypeVar(tv) if !tv.as_str().starts_with('?') => {
                let forwards = caller
                    .and_then(|c| self.env.input_sinks.get(&c))
                    .is_some_and(|tvs| tvs.contains(tv));
                (!forwards).then(|| NotInput::Forwarded {
                    tv: tv.to_string(),
                    caller: caller.map(|c| c.to_string()).unwrap_or_else(|| "the caller".to_string()),
                })
            }
            Ty::Named(name, _) => {
                if let Some(proto) = self.sent_out_as(*name) {
                    return Some(NotInput::SentOut { ty: name.to_string(), proto });
                }
                let mut seen = std::collections::HashSet::new();
                self.nested_sent_out(*name, &mut seen)
            }
            Ty::Variant { name, .. } => self.not_an_input_type(&Ty::Named(*name, vec![]), caller),
            _ => None,
        }
    }

    /// The type's declaration, under the spelling the env holds it by.
    fn named_def(&self, name: Sym) -> Option<&Ty> {
        let bare = name.as_str().rsplit('.').next().unwrap_or(name.as_str());
        self.env.types.get(&name).or_else(|| self.env.types.get(&sym(bare)))
    }

    /// `Some("Codec")` / `Some("Encode")` when the user type is also written
    /// out: its declaration's protocol closure holds `Encode`.
    fn sent_out_as(&self, name: Sym) -> Option<&'static str> {
        use crate::canonicalize::protocols::{protocol_closure, ENCODE_PROTOCOL, PROTOCOL_PARTS};
        let bare = name.as_str().rsplit('.').next().unwrap_or(name.as_str());
        let declared: Vec<Sym> = [name, sym(bare)].iter()
            .filter_map(|n| self.env.type_protocols.get(n))
            .flatten().copied().collect();
        if !declared.iter().any(|p| protocol_closure(p.as_str()).contains(&ENCODE_PROTOCOL)) {
            return None;
        }
        // Name the composition the type declared when there is one.
        Some(PROTOCOL_PARTS.iter()
            .find(|(c, parts)| parts.contains(&ENCODE_PROTOCOL) && declared.iter().any(|d| d.as_str() == *c))
            .map(|(c, _)| *c)
            .unwrap_or(ENCODE_PROTOCOL))
    }

    /// A user record type nested anywhere inside input type `outer` that is
    /// also written out. A type declared by a stdlib module is a leaf value
    /// with no input surface of its own (its declaration says where it lives).
    fn nested_sent_out(&self, outer: Sym, seen: &mut std::collections::HashSet<Sym>) -> Option<NotInput> {
        if !seen.insert(outer) {
            return None;
        }
        // A record's fields, or every payload slot of a variant's cases.
        let fields: Vec<(String, Ty)> = match self.named_def(outer) {
            Some(Ty::Record { fields }) | Some(Ty::OpenRecord { fields }) =>
                fields.iter().map(|(n, t)| (n.to_string(), t.clone())).collect(),
            Some(Ty::Variant { cases, .. }) => cases.iter().flat_map(|c| match &c.payload {
                crate::types::VariantPayload::Unit => Vec::new(),
                crate::types::VariantPayload::Tuple(ts) => ts.iter().enumerate()
                    .map(|(i, t)| (format!("{}.{}", c.name, i), t.clone())).collect(),
                crate::types::VariantPayload::Record(fs) => fs.iter()
                    .map(|(n, t)| (format!("{}.{}", c.name, n), t.clone())).collect(),
            }).collect(),
            _ => return None,
        };
        for (field, fty) in fields {
            let mut leaves = Vec::new();
            input_leaf_nominals(&fty, &mut leaves);
            for leaf in leaves {
                if leaf == outer || self.named_def(leaf).is_none() || self.declared_by_stdlib(leaf) {
                    continue;
                }
                if let Some(proto) = self.sent_out_as(leaf) {
                    return Some(NotInput::NestedSentOut {
                        outer: outer.to_string(), field, inner: leaf.to_string(), proto,
                    });
                }
                if let Some(found) = self.nested_sent_out(leaf, seen) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn declared_by_stdlib(&self, name: Sym) -> bool {
        crate::canonicalize::protocols::declaring_module(&self.env, name.as_str())
            .is_some_and(|m| almide_lang::stdlib_info::is_bundled_module(&m))
    }

    fn input_sink_diagnostic(&self, callee: &str, ty: &Ty, why: NotInput) -> Diagnostic {
        match why {
            NotInput::SentOut { ty: t, proto } => err(
                format!("`{callee}` decodes untrusted input only into an input type, and `{t}` is also sent out (it is `{proto}`)"),
                format!(
                    "Declare what the client may set as its own type, then build `{t}` in code:\n          {}\n        Every field of a `: Decode` type is input. A field the client must not set stays out of it.",
                    self.input_type_example(callee, &t)
                ),
                format!("{callee}[{}]", ty.display()),
            ),
            NotInput::NestedSentOut { outer, field, inner, proto } => err(
                format!("`{callee}` decodes untrusted input into `{outer}`, and its field `{field}` holds `{inner}`, which is also sent out (it is `{proto}`)"),
                format!("Every record inside an input type is input too. Declare an input copy, `type {inner}Input: Decode = {{ ... }}`, use it in `{outer}`, and build `{inner}` from it in code."),
                format!("{callee}[{}]", ty.display()),
            ),
            NotInput::Forwarded { tv, caller } => err(
                format!("type parameter `{tv}` of `{caller}` reaches `{callee}`'s input type, and `{caller}` is not an input sink"),
                format!("Mark `{caller}` `@input_sink` so each of its callers' types is judged as an input type, or call `{callee}` with an input type directly."),
                format!("{callee}[{tv}]"),
            ),
        }
    }

    /// The E110 hint's example: an input type of `t`'s fields that have no
    /// default (a defaulted field is the one a server usually owns), and the
    /// call that builds `t` from it.
    fn input_type_example(&self, callee: &str, t: &str) -> String {
        let name = sym(t);
        let bare = t.rsplit('.').next().unwrap_or(t);
        let defaults = self.env.record_field_defaults.get(&name)
            .or_else(|| self.env.record_field_defaults.get(&sym(bare)))
            .cloned().unwrap_or_default();
        let fields: Vec<(String, String)> = match self.named_def(name) {
            Some(Ty::Record { fields }) => fields.iter()
                .filter(|(n, _)| !defaults.contains(n))
                .map(|(n, t)| (n.to_string(), t.display()))
                .collect(),
            _ => Vec::new(),
        };
        let decl = fields.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", ");
        let build = fields.iter().map(|(n, _)| format!("{n}: i.{n}")).collect::<Vec<_>>().join(", ");
        format!(
            "type {bare}Input: Decode = {{ {decl} }}\n          match {callee}[{bare}Input](req) {{\n            ok(i) => … {bare} {{ {build}, … }} …\n            err(bad) => bad,     // unknown key → 400\n          }}"
        )
    }
}

/// The nominal types a field type names, through the standard containers.
fn input_leaf_nominals(ty: &Ty, out: &mut Vec<Sym>) {
    match ty {
        Ty::Named(n, args) => {
            out.push(*n);
            for a in args { input_leaf_nominals(a, out); }
        }
        Ty::Variant { name, .. } => out.push(*name),
        Ty::Applied(_, args) | Ty::Tuple(args) => for a in args { input_leaf_nominals(a, out); },
        Ty::Record { fields } | Ty::OpenRecord { fields } => for (_, t) in fields { input_leaf_nominals(t, out); },
        _ => {}
    }
}
