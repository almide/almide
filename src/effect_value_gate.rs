//! E109: a pure function runs an `effect fn` it reached through a function
//! value (#4151).
//!
//! E006 judges what the checker can see at a call: a direct call of an
//! `effect fn`, a name that holds one (#3274), and an `effect fn` named as an
//! argument in a pure context. A function value travels further than that —
//! into a record field, a list, a callback handed down through pure helpers —
//! and the pure function that finally calls it ran the effect with nobody
//! refusing it. The effect bit rides the value in the same flow analysis that
//! carries the category sets (ADR-0009 D2, ADR-0026 D1:
//! `almide_codegen::effect_flow`), and a pure function whose solved set holds
//! the bit concretely — not through a parameter of its own, which is charged
//! at its call sites instead — is refused here.
//!
//! The analysis is whole-program and runs on the lowered IR, so every route
//! that turns a checked program into something that runs (`build`, `run`,
//! `test`, both targets) and `check` call [`effect_value_diagnostics`] before
//! they go on.
//!
//! The same IR gate carries E110's flow half (row 22 phase 2, #4232):
//! request data decoded by hand into a type that is also sent out
//! (`almide_frontend::request_flow`). Both are whole-program flow verdicts
//! over the lowered program, so every route that judges one judges both.

use crate::diagnostic::Diagnostic;
use crate::ir::IrProgram;

/// The E109 diagnostics of `ir`, in function-name order (stable across runs).
/// `file` names the entry source; a violation in an imported module names its
/// module in the message and carries no span, since the span is that module's.
pub fn effect_value_diagnostics(ir: &IrProgram, file: &str) -> Vec<Diagnostic> {
    let mut out = effect_fn_value_diagnostics(ir, file);
    out.extend(hand_decode_diagnostics(ir, file));
    out
}

/// E110, flow half (#4232): request data reaching a hand-written decode into
/// a type that is also sent out.
pub fn hand_decode_diagnostics(ir: &IrProgram, file: &str) -> Vec<Diagnostic> {
    use almide_frontend::request_flow::HandDecode;
    almide_frontend::request_flow::hand_decode_violations(ir)
        .into_iter()
        .map(|v| {
            let (message, hint) = match &v.why {
                HandDecode::SentOut { ty, proto } => (
                    format!("`{}` decodes request data into `{ty}`, and `{ty}` is also sent out (it is `{proto}`)", v.decode),
                    format!(
                        "Every field of a type decoded from a request is input, so the client sets any field of `{ty}` by posting it. \
                         Declare what the client may set as its own type, `type {ty}Input: Decode = {{ ... }}`, decode into it \
                         (`http.decode_json[{ty}Input](req)`, or `{ty}Input.decode(..)`), and build `{ty}` from it in code"
                    ),
                ),
                HandDecode::NestedSentOut { outer, field, inner, proto } => (
                    format!("`{}` decodes request data into `{outer}`, and its field `{field}` holds `{inner}`, which is also sent out (it is `{proto}`)", v.decode),
                    format!("Every record inside a type decoded from a request is input too. Declare an input copy, `type {inner}Input: Decode = {{ ... }}`, use it in `{outer}`, and build `{inner}` from it in code"),
                ),
                HandDecode::Forwarded { tv, holder } => (
                    format!("request data reaches `{}` through type parameter `{tv}` of `{holder}`, and `{holder}` is not an input sink", v.decode),
                    format!("Mark `{}` `@input_sink` so each of its callers' types is judged as an input type, or decode into a `: Decode` input type directly", holder.rsplit('.').next().unwrap_or(holder)),
                ),
            };
            let mut d = Diagnostic::error(message, hint, v.function.clone())
                .with_code("E110")
                .with_note(format!("path: {}", v.path));
            match (&v.module, v.site) {
                (None, Some(span)) => d = d.at_span(file, span),
                (Some(m), _) => d = d.with_note(format!("declared in module `{m}`")),
                (None, None) => {}
            }
            d
        })
        .collect()
}

/// E109 alone.
pub fn effect_fn_value_diagnostics(ir: &IrProgram, file: &str) -> Vec<Diagnostic> {
    let mut violations = crate::codegen::effect_flow::effect_value_violations(ir);
    violations.sort_by(|a, b| a.function.cmp(&b.function));
    violations
        .into_iter()
        .map(|v| {
            let subject = if v.function.ends_with("<top-level>") {
                "a top-level `let`".to_string()
            } else {
                format!("pure function `{}`", v.function)
            };
            let message = format!("{subject} runs an effect function it does not call by name");
            let hint = format!(
                "A plain `fn` cannot run an `effect fn`, even one it receives as a value — a callback, a \
                 record field, a list element (ADR-0009 D2). Mark `{}` as `effect fn` if it is meant to \
                 do I/O, or pass it a function that does none",
                v.function.rsplit('.').next().unwrap_or(&v.function),
            );
            let mut d = Diagnostic::error(message, hint, v.function.clone())
                .with_code("E109")
                .with_note(format!("path: {}", v.path));
            match (&v.module, v.site) {
                (None, Some(span)) => d = d.at_span(file, span),
                (Some(m), _) => d = d.with_note(format!("declared in module `{m}`")),
                (None, None) => {}
            }
            d
        })
        .collect()
}
