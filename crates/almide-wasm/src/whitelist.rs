//! The sized-integer conversion family's linked-impl admission — the
//! 2026-08 audit (stage 72): every module body is PURE SCALAR (the prim
//! scan shows only band/bor/bshl/f2i/i2f/f-cmp — no loads, no stores, no
//! handles), so no incumbent-layout coupling can exist. Option-returning
//! cells build their sums via some()/none constructors (this emitter's
//! own lowering) and sit in the SUM tier. Names are the registry rows of
//! int_sized / int_checked / int8..64_convert / uint8..64_convert /
//! float(64)_convert filtered to the `<ty>_(to|from)_<ty>[_checked|
//! _saturating]` + `<ty>_(min|max)_value` shape — minus every Float32
//! cell (that arc is separate; refusal walls them honestly).

/// Plain-signature members (scalars in, scalar/String out).
pub(crate) const SIZED_CONVERT_VERIFIED: &[&str] = &[
    "float64_to_int16", "float64_to_int32", "float64_to_int64", "float64_to_int8",
    "float64_to_string", "float64_to_uint16", "float64_to_uint32", "float64_to_uint64",
    "float64_to_uint8", "float_to_float64", "float_to_int", "float_to_int16",
    "float_to_int16_saturating", "float_to_int32", "float_to_int32_saturating",
    "float_to_int64", "float_to_int64_saturating", "float_to_int8", "float_to_int8_saturating",
    "float_to_uint16", "float_to_uint16_saturating", "float_to_uint32",
    "float_to_uint32_saturating", "float_to_uint64", "float_to_uint64_saturating",
    "float_to_uint8", "float_to_uint8_saturating", "int16_max_value", "int16_min_value",
    "int16_to_float64", "int16_to_int32", "int16_to_int64", "int16_to_int8",
    "int16_to_int8_saturating", "int16_to_string", "int16_to_uint16",
    "int16_to_uint16_saturating", "int16_to_uint32", "int16_to_uint32_saturating",
    "int16_to_uint64", "int16_to_uint64_saturating", "int16_to_uint8",
    "int16_to_uint8_saturating", "int32_max_value", "int32_min_value", "int32_to_float64",
    "int32_to_int16", "int32_to_int16_saturating", "int32_to_int64", "int32_to_int8",
    "int32_to_int8_saturating", "int32_to_string", "int32_to_uint16",
    "int32_to_uint16_saturating", "int32_to_uint32", "int32_to_uint32_saturating",
    "int32_to_uint64", "int32_to_uint64_saturating", "int32_to_uint8",
    "int32_to_uint8_saturating", "int64_max_value", "int64_min_value", "int64_to_float64",
    "int64_to_int16", "int64_to_int16_saturating", "int64_to_int32",
    "int64_to_int32_saturating", "int64_to_int8", "int64_to_int8_saturating",
    "int64_to_string", "int64_to_uint16", "int64_to_uint16_saturating", "int64_to_uint32",
    "int64_to_uint32_saturating", "int64_to_uint64", "int64_to_uint64_saturating",
    "int64_to_uint8", "int64_to_uint8_saturating", "int8_max_value", "int8_min_value",
    "int8_to_float64", "int8_to_int16", "int8_to_int32", "int8_to_int64", "int8_to_string",
    "int8_to_uint16", "int8_to_uint16_saturating", "int8_to_uint32",
    "int8_to_uint32_saturating", "int8_to_uint64", "int8_to_uint64_saturating",
    "int8_to_uint8", "int8_to_uint8_saturating", "int_from_int16", "int_from_int32",
    "int_from_int64", "int_from_int8", "int_from_uint16", "int_from_uint32", "int_from_uint64",
    "int_from_uint64_saturating", "int_from_uint8", "int_max_value", "int_min_value",
    "int_to_int16", "int_to_int16_saturating", "int_to_int32", "int_to_int32_saturating",
    "int_to_int64", "int_to_int8", "int_to_int8_saturating", "int_to_uint16",
    "int_to_uint16_saturating", "int_to_uint32", "int_to_uint32_saturating", "int_to_uint64",
    "int_to_uint64_saturating", "int_to_uint8", "int_to_uint8_saturating", "uint16_max_value",
    "uint16_min_value", "uint16_to_float64", "uint16_to_int16", "uint16_to_int16_saturating",
    "uint16_to_int32", "uint16_to_int64", "uint16_to_int8", "uint16_to_int8_saturating",
    "uint16_to_string", "uint16_to_uint32", "uint16_to_uint64", "uint16_to_uint8",
    "uint16_to_uint8_saturating", "uint32_max_value", "uint32_min_value", "uint32_to_float64",
    "uint32_to_int16", "uint32_to_int16_saturating", "uint32_to_int32",
    "uint32_to_int32_saturating", "uint32_to_int64", "uint32_to_int8",
    "uint32_to_int8_saturating", "uint32_to_string", "uint32_to_uint16",
    "uint32_to_uint16_saturating", "uint32_to_uint64", "uint32_to_uint8",
    "uint32_to_uint8_saturating", "uint64_max_value", "uint64_min_value", "uint64_to_float64",
    "uint64_to_int16", "uint64_to_int16_saturating", "uint64_to_int32",
    "uint64_to_int32_saturating", "uint64_to_int64", "uint64_to_int64_saturating",
    "uint64_to_int8", "uint64_to_int8_saturating", "uint64_to_string", "uint64_to_uint16",
    "uint64_to_uint16_saturating", "uint64_to_uint32", "uint64_to_uint32_saturating",
    "uint64_to_uint8", "uint64_to_uint8_saturating", "uint8_max_value", "uint8_min_value",
    "uint8_to_float64", "uint8_to_int16", "uint8_to_int32", "uint8_to_int64", "uint8_to_int8",
    "uint8_to_int8_saturating", "uint8_to_string", "uint8_to_uint16", "uint8_to_uint32",
    "uint8_to_uint64",
];

/// Scalar/text self-host impls admitted by the stage-72 tail audit:
/// float_core/float_extra (pure float prims), int_hex + int_rotate
/// (bit ops + own alloc_str builds), string_search/string_class/
/// string_trim (READ-ONLY loads on the digest-shared string layout +
/// own-buffer builds), base64_encode (byte-domain own builds),
/// datetime family (pure arithmetic + own alloc_str), hash_impl
/// (prim-MEDIATED alloc_list state — the list_repeat precedent; its
/// slot stores are the 8-byte Int class both layouts share). Every
/// claim is byte-verified by the burn-up before it counts.
pub(crate) const SCALAR_TEXT_VERIFIED: &[&str] = &[
    "float_abs", "float_floor", "float_round", "float_is_nan", "float_from_float64",
    "float_from_float32", "float_to_float32",
    "int_to_hex", "int_rotate_left", "int_rotate_right", "hash_fnv1a32",
    "hash_sha256", "hash_sha256_hex", "hex_encode", "hex_encode_upper", "hex_decode",
    "string_contains", "string_count", "string_trim_start", "string_trim_end",
    "string_is_alpha", "string_is_digit", "string_is_alphanumeric_uni", "string_is_upper",
    "string_is_lower",
    "base64_encode", "base64_encode_url", "hash_fnv1a32_bytes",
    "datetime_add_days", "datetime_add_hours", "datetime_add_minutes", "datetime_add_seconds", "datetime_day", "datetime_diff_seconds", "datetime_format", "datetime_from_parts", "datetime_from_unix", "datetime_hour", "datetime_is_after", "datetime_is_before", "datetime_minute", "datetime_month", "datetime_second", "datetime_to_iso", "datetime_to_unix", "datetime_weekday", "datetime_year",
    "string_is_whitespace", "string_to_bytes",
];

/// Same audit, Option-returning (constructor-built sums).
pub(crate) const SCALAR_TEXT_SUM_BUILDERS: &[&str] =
    &["string_index_of", "string_last_index_of", "base64_decode", "base64_decode_url"];

/// The Codec-derive encode splices: their bodies carry ZERO prims —
/// every Value is built through the public value.* surface, which THIS
/// emitter lowers natively, so the bodies are layout-AGNOSTIC (the
/// coupled-type proxy over-approximates for them and they sit in the
/// exempt tier). The decode module stays walled: its __is_null reads
/// the incumbent's tag position raw (`load32(h+4)`).
pub(crate) const CODEC_ENCODE_VERIFIED: &[&str] = &[
    "__encode_list_int", "__encode_list_float", "__encode_list_bool", "__encode_list_string",
    "__encode_option_int", "__encode_option_float", "__encode_option_bool",
    "__encode_option_string",
    // decode: prim-free EXCEPT __is_null, which the emitter replaces
    // with a native twin (see calls.rs) — the rest builds through the
    // public value/result surfaces.
    "__decode_list_int", "__decode_list_float", "__decode_list_bool", "__decode_list_string",
    "__decode_option_int", "__decode_option_float", "__decode_option_bool",
    "__decode_option_string", "__decode_default_int", "__decode_default_float",
    "__decode_default_bool", "__decode_default_string",
];

/// The bytes append/array/cursor/string family (stage-74 audit):
/// byte-domain bodies over the digest-shared Bytes layout (len IS the
/// byte count on BOTH sides — unlike lists), alloc_bytes/alloc_str/
/// prim-mediated list allocs for their own outputs, 8-byte Int/Float
/// slot stores only. The `_at` cursor forms return (Int, T?) tuples
/// BUILT AS LITERALS (no raw tuple ops) and sit in the exempt tier.
/// json_get_* are prim-free (public value surface only).
pub(crate) const BYTES_FAMILY_VERIFIED: &[&str] = &[
    "bytes_append_f32_be", "bytes_append_f32_le", "bytes_append_f64_be", "bytes_append_f64_le",
    "bytes_append_i16_be", "bytes_append_i16_le", "bytes_append_i32_be", "bytes_append_i32_le",
    "bytes_append_i64_be", "bytes_append_i64_le", "bytes_append_u16_be", "bytes_append_u16_le",
    "bytes_append_u32_be", "bytes_append_u32_le", "bytes_read_f16_le_array",
    "bytes_read_f32_be_array", "bytes_read_f32_le_array", "bytes_read_f64_be_array",
    "bytes_read_f64_le_array", "bytes_read_i16_be_array", "bytes_read_i16_le_array",
    "bytes_read_i32_be_array", "bytes_read_i32_le_array", "bytes_read_i64_be_array",
    "bytes_read_i64_le_array", "bytes_read_string_be", "bytes_read_u16_be_array",
    "bytes_read_u16_le_array", "bytes_read_u32_be_array", "bytes_read_u32_le_array",
    // bytes_typed.almd (Endian-argument wrappers, #1098): audited 2026-08-25 —
    // prim.alloc_bytes + load8/store8 on Bytes payloads only (len=bytes on
    // both legs), payload offset 12 = OUR PAYLOAD; the Endian ctors build a
    // 1-byte block only this module reads; reads/sets cross-call the C-229
    // native matrix; write_* return the fresh grown block (v1 rebind form).
    "bytes_endian_le", "bytes_endian_be", "bytes_read_uint16", "bytes_read_uint32",
    "bytes_read_int32", "bytes_read_float32", "bytes_write_uint16", "bytes_write_uint32",
    "bytes_write_int32", "bytes_write_float32", "bytes_set_uint16", "bytes_set_uint32",
    "bytes_set_int32", "bytes_set_float32",
    // bytes_append_multi.almd cursor tail (#1099): the __bam grow-append
    // shape — prim.load32(handle+4) is the Bytes LEN header (len = bytes
    // on both legs); same audit as the append_* family above.
    "bytes_write_bool", "bytes_write_string_be",
];

/// The exempt-tier members of the same audit (tuple/Option returners
/// with literal-built sums).
pub(crate) const BYTES_FAMILY_SUM: &[&str] = &[
    "bytes_read_bool_at", "bytes_read_f16_le_at", "bytes_read_f32_be_at", "bytes_read_f32_le_at",
    "bytes_read_f64_be_at", "bytes_read_f64_le_at", "bytes_read_i16_be_at", "bytes_read_i16_le_at",
    "bytes_read_i32_be_at", "bytes_read_i32_le_at", "bytes_read_i64_be_at", "bytes_read_i64_le_at",
    "bytes_read_string_at", "bytes_read_string_be_at", "bytes_read_u16_be_at",
    "bytes_read_u16_le_at", "bytes_read_u32_be_at", "bytes_read_u32_le_at", "bytes_read_u8_at",
    "bytes_take_at", "json_get_array", "json_get_bool", "json_get_float", "json_get_int",
    "json_get_string",
    // json_path.almd, READ side only (audited 2026-08-25): the path rep
    // is a plain List[String]; get walks value.field / value.as_array /
    // list.get — all native arms here, no raw layout reads. set/remove
    // are REJECTED: their pair walks read the incumbent's inline-pairs
    // Value layout (tag@h+4, count@h+8) — see PORT-MATRIX.
    "json_path_root", "json_path_field", "json_path_index", "json_path_get",
];

/// The vendored-libm family (C-305): every body is a FAITHFUL
/// transcription of libm 0.2.16 with constants via prim.ffrombits —
/// pure scalar prims throughout; math_trig's coefficient tables go
/// through prim-MEDIATED alloc_list_f64 with 8-byte Float slot stores
/// (the one list class both layouts share). One vendored libm on every
/// target is the bit-parity mechanism itself.
pub(crate) const MATH_VERIFIED: &[&str] = &[
    "math_abs", "math_atan", "math_choose", "math_cos", "math_e", "math_exp",
    "math_factorial", "math_fmax", "math_fmin", "math_fpow", "math_log", "math_log10",
    "math_log2", "math_log_gamma", "math_max", "math_min", "math_pi", "math_pow",
    "math_sign", "math_sin", "math_sqrt", "math_tan", "math_tanh",
];

/// Option-returning members (the checked trio cells).
pub(crate) const SIZED_CONVERT_SUM_BUILDERS: &[&str] = &[
    "float_to_int16_checked", "float_to_int32_checked", "float_to_int64_checked",
    "float_to_int8_checked", "float_to_uint16_checked", "float_to_uint32_checked",
    "float_to_uint64_checked", "float_to_uint8_checked", "int16_to_int8_checked",
    "int16_to_uint16_checked", "int16_to_uint32_checked", "int16_to_uint64_checked",
    "int16_to_uint8_checked", "int32_to_int16_checked", "int32_to_int8_checked",
    "int32_to_uint16_checked", "int32_to_uint32_checked", "int32_to_uint64_checked",
    "int32_to_uint8_checked", "int64_to_int16_checked", "int64_to_int32_checked",
    "int64_to_int8_checked", "int64_to_uint16_checked", "int64_to_uint32_checked",
    "int64_to_uint64_checked", "int64_to_uint8_checked", "int8_to_uint16_checked",
    "int8_to_uint32_checked", "int8_to_uint64_checked", "int8_to_uint8_checked",
    "int_from_uint64_checked", "int_to_int16_checked", "int_to_int32_checked",
    "int_to_int8_checked", "int_to_uint16_checked", "int_to_uint32_checked",
    "int_to_uint64_checked", "int_to_uint8_checked", "uint16_to_int16_checked",
    "uint16_to_int8_checked", "uint16_to_uint8_checked", "uint32_to_int16_checked",
    "uint32_to_int32_checked", "uint32_to_int8_checked", "uint32_to_uint16_checked",
    "uint32_to_uint8_checked", "uint64_to_int16_checked", "uint64_to_int32_checked",
    "uint64_to_int64_checked", "uint64_to_int8_checked", "uint64_to_uint16_checked",
    "uint64_to_uint32_checked", "uint64_to_uint8_checked", "uint8_to_int8_checked",
];
