//! An equality- or ordering-bound generic stdlib fn (`list.contains`,
//! `list.min`, `list.unique`, …) is on the native floor only through a
//! self-hosted body over a `List[String]` that specializes it (#4006, owner
//! ruling ○ 2026-11-04): a String carries no Float bits, so the runtime's
//! `PartialEq` / `Ord` on `AlmideString` is the language's comparison. The
//! admission is derived from the specialized body's declared element type in
//! one place (`native_floor::derive_for`), never from a list of names.

#[test]
fn a_string_list_specialization_is_admitted() {
    for call in ["list.contains_str", "list.min_str"] {
        assert_eq!(almide_mir::native_floor::refusal(call), None, "`{call}` is not on the native floor");
    }
}

/// Negative control: the generic itself (an Int or Float list reaches it by
/// that name) and a Float list's specialization stay refused.
#[test]
fn an_int_or_float_list_stays_refused() {
    for call in ["list.contains", "list.min", "list.unique", "list.unique_float"] {
        let why = almide_mir::native_floor::refusal(call);
        assert!(
            why.as_deref().is_some_and(|w| w.contains("equality or ordering")),
            "`{call}` must stay refused for its equality/ordering bound, got {why:?}"
        );
    }
}
