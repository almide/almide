//! The ops whose native runtime takes its source either way (#3398): the
//! list range ops and the whole-map reads.
//!
//! `list.take(xs, n)` and its family only READ their source, so they borrow it
//! (`&[T]`, #3397): a source read again afterwards is never cloned whole. Where
//! the source is NOT read again, the borrow costs a clone of every element the
//! op keeps — 2× on a `list.drop(xs, 1)` tail recursion over strings. Each op
//! here has an `_owned` twin in `runtime/rs/src/list.rs` (`map.rs` for the map
//! reads) that takes the source by value and moves the kept elements out.
//!
//! Who decides which entry a call site gets:
//! - CloneInsertion (and TailCallOpt, for the loop params whose moves it owns)
//!   leave the source a bare `Var` where their last-use analysis says a move
//!   is legal, and a `Borrow` everywhere else.
//! - BorrowLowering spells the result: a source that is an owned value (that
//!   bare `Var`, or a temporary) calls the `_owned` twin by value; one that is a
//!   reference in Rust (a by-reference param or binder, a global, a shared
//!   cell) stays borrowed.
//!
//! Gated by `tests/list_read_only_source_borrow_test.rs`.

/// `(borrowing runtime symbol, owned twin)`.
pub const OWNED_SOURCE_TWINS: &[(&str, &str)] = &[
    ("almide_rt_list_take", "almide_rt_list_take_owned"),
    ("almide_rt_list_drop", "almide_rt_list_drop_owned"),
    ("almide_rt_list_take_while", "almide_rt_list_take_while_owned"),
    ("almide_rt_list_drop_while", "almide_rt_list_drop_while_owned"),
    ("almide_rt_list_find", "almide_rt_list_find_owned"),
    ("almide_rt_list_slice", "almide_rt_list_slice_owned"),
    ("almide_rt_list_take_end", "almide_rt_list_take_end_owned"),
    ("almide_rt_list_drop_end", "almide_rt_list_drop_end_owned"),
    // Whole-map reads: the twin moves every key / value out of a source that
    // is not read again instead of cloning each one (`runtime/rs/src/map.rs`).
    ("almide_rt_map_keys", "almide_rt_map_keys_owned"),
    ("almide_rt_map_values", "almide_rt_map_values_owned"),
    ("almide_rt_map_entries", "almide_rt_map_entries_owned"),
    ("almide_rt_map_map_values", "almide_rt_map_map_values_owned"),
];

/// The owned twin of a borrowing range op, if it has one.
pub fn owned_twin(symbol: &str) -> Option<&'static str> {
    OWNED_SOURCE_TWINS.iter().find(|(b, _)| *b == symbol).map(|(_, o)| *o)
}

/// Is `symbol` a range op whose first argument may be moved instead of
/// borrowed?
pub fn takes_source_either_way(symbol: &str) -> bool {
    owned_twin(symbol).is_some()
}
