//! The self-hosted stdlib runtime registry: (source, &[(impl_fn, call_name)]) tuples.
//! Extracted from render_wasm.rs to keep that file under the 1000-line limit. Paths are
//! one level deeper here, so include_str! uses ../../../../stdlib (vs ../../../ before).

pub fn self_host_runtime() -> &'static [(&'static str, &'static [(&'static str, &'static str)])] {
    &[
        (include_str!("../../../../stdlib/int_to_string.almd"), &[("int_to_string", "int.to_string")]),
        (include_str!("../../../../stdlib/string_len.almd"), &[("string_len", "string.len")]),
        (include_str!("../../../../stdlib/string_repeat.almd"), &[("string_repeat", "string.repeat")]),
        (include_str!("../../../../stdlib/string_is_empty.almd"), &[("string_is_empty", "string.is_empty")]),
        (
            include_str!("../../../../stdlib/math_int.almd"),
            &[
                ("math_abs", "math.abs"),
                ("math_max", "math.max"),
                ("math_min", "math.min"),
                ("math_sign", "math.sign"),
                ("math_pow", "math.pow"),
                ("math_factorial", "math.factorial"),
                ("math_choose", "math.choose"),
            ],
        ),
        (
            include_str!("../../../../stdlib/list_modify.almd"),
            &[
                ("list_set", "list.set"),
                ("list_swap", "list.swap"),
                ("list_insert", "list.insert"),
                ("list_remove_at", "list.remove_at"),
                ("list_tail", "list.tail"),
            ],
        ),
        (include_str!("../../../../stdlib/list_len.almd"), &[("list_len", "list.len")]),
        (include_str!("../../../../stdlib/list_is_empty.almd"), &[("list_is_empty", "list.is_empty")]),
        (include_str!("../../../../stdlib/list_sum.almd"), &[("list_sum", "list.sum")]),
        (include_str!("../../../../stdlib/list_sort.almd"), &[("list_sort", "list.sort")]),
        (include_str!("../../../../stdlib/list_unique.almd"), &[("list_unique", "list.unique")]),
        (include_str!("../../../../stdlib/list_dedup.almd"), &[("list_dedup", "list.dedup")]),
        (
            include_str!("../../../../stdlib/list_intersperse.almd"),
            &[("list_intersperse", "list.intersperse")],
        ),
        (
            include_str!("../../../../stdlib/int_wrap.almd"),
            &[
                ("int_wrap_add", "int.wrap_add"),
                ("int_wrap_mul", "int.wrap_mul"),
                ("int_to_u32", "int.to_u32"),
                ("int_to_u8", "int.to_u8"),
            ],
        ),
        (
            include_str!("../../../../stdlib/int_sized.almd"),
            &[
                ("int_to_int8", "int.to_int8"),
                ("int_to_int16", "int.to_int16"),
                ("int_to_int32", "int.to_int32"),
                ("int_to_int64", "int.to_int64"),
                ("int_to_int8_saturating", "int.to_int8_saturating"),
                ("int_to_int16_saturating", "int.to_int16_saturating"),
                ("int_to_int32_saturating", "int.to_int32_saturating"),
                ("int_to_uint64", "int.to_uint64"),
                ("int_from_int64", "int.from_int64"),
                ("int_from_uint64", "int.from_uint64"),
            ],
        ),
        (include_str!("../../../../stdlib/string_slice.almd"), &[("string_slice", "string.slice")]),
        (
            include_str!("../../../../stdlib/string_is_digit.almd"),
            &[("string_is_digit", "string.is_digit")],
        ),
        (
            include_str!("../../../../stdlib/string_from_codepoint.almd"),
            &[("string_from_codepoint", "string.from_codepoint")],
        ),
        (
            include_str!("../../../../stdlib/string_codepoint.almd"),
            &[("string_codepoint", "string.codepoint")],
        ),
        (
            include_str!("../../../../stdlib/string_take_drop.almd"),
            &[
                ("string_take", "string.take"),
                ("string_take_end", "string.take_end"),
                ("string_drop", "string.drop"),
                ("string_drop_end", "string.drop_end"),
            ],
        ),
        (
            include_str!("../../../../stdlib/string_to_bytes.almd"),
            &[("string_to_bytes", "string.to_bytes")],
        ),
        (
            include_str!("../../../../stdlib/string_trim.almd"),
            &[
                ("string_trim", "string.trim"),
                ("string_trim_start", "string.trim_start"),
                ("string_trim_end", "string.trim_end"),
            ],
        ),
        (include_str!("../../../../stdlib/string_reverse.almd"), &[("string_reverse", "string.reverse")]),
        (
            include_str!("../../../../stdlib/string_replace.almd"),
            &[
                ("string_replace", "string.replace"),
                ("string_replace_first", "string.replace_first"),
            ],
        ),
        (
            include_str!("../../../../stdlib/string_pad.almd"),
            &[("string_pad_start", "string.pad_start"), ("string_pad_end", "string.pad_end")],
        ),
        (include_str!("../../../../stdlib/list_get_or.almd"), &[("list_get_or", "list.get_or")]),
        (
            include_str!("../../../../stdlib/int_bitcount.almd"),
            &[
                ("int_pop_count", "int.pop_count"),
                ("int_count_trailing_zeros", "int.count_trailing_zeros"),
                ("int_count_leading_zeros", "int.count_leading_zeros"),
                ("int_bit_width", "int.bit_width"),
                ("int_log2_floor", "int.log2_floor"),
                ("int_log2_ceil", "int.log2_ceil"),
                ("int_next_power_of_two", "int.next_power_of_two"),
                ("int_prev_power_of_two", "int.prev_power_of_two"),
            ],
        ),
        (
            include_str!("../../../../stdlib/int_bits.almd"),
            &[
                ("int_band", "int.band"),
                ("int_bor", "int.bor"),
                ("int_bxor", "int.bxor"),
                ("int_bshl", "int.bshl"),
                ("int_bshr", "int.bshr"),
                ("int_bnot", "int.bnot"),
                ("int_byte_swap", "int.byte_swap"),
                ("int_bit_reverse", "int.bit_reverse"),
            ],
        ),
        (include_str!("../../../../stdlib/int_hex.almd"), &[("int_to_hex", "int.to_hex")]),
        (
            include_str!("../../../../stdlib/int_scalar.almd"),
            &[
                ("int_abs", "int.abs"),
                ("int_min", "int.min"),
                ("int_max", "int.max"),
                ("int_clamp", "int.clamp"),
            ],
        ),
        (
            include_str!("../../../../stdlib/list_get.almd"),
            &[("list_get", "list.get"), ("list_first", "list.first"), ("list_last", "list.last")],
        ),
        (
            include_str!("../../../../stdlib/list_search.almd"),
            &[
                ("list_contains", "list.contains"),
                ("list_index_of", "list.index_of"),
                ("list_binary_search", "list.binary_search"),
            ],
        ),
        (include_str!("../../../../stdlib/list_reverse.almd"), &[("list_reverse", "list.reverse")]),
        (
            include_str!("../../../../stdlib/list_make.almd"),
            &[("list_range", "list.range"), ("list_repeat", "list.repeat")],
        ),
        (
            include_str!("../../../../stdlib/list_take_drop.almd"),
            &[
                ("list_take", "list.take"),
                ("list_drop", "list.drop"),
                ("list_slice", "list.slice"),
            ],
        ),
        (
            include_str!("../../../../stdlib/list_fold.almd"),
            &[
                ("list_product", "list.product"),
                ("list_max", "list.max"),
                ("list_min", "list.min"),
            ],
        ),
        (
            include_str!("../../../../stdlib/string_search.almd"),
            &[
                ("string_starts_with", "string.starts_with"),
                ("string_ends_with", "string.ends_with"),
                ("string_contains", "string.contains"),
                ("string_count", "string.count"),
                ("string_index_of", "string.index_of"),
                ("string_last_index_of", "string.last_index_of"),
            ],
        ),
    ]
}
