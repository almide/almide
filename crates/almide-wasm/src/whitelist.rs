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
    "int_to_hex", "int_rotate_left", "int_rotate_right", "hash_fnv1a32",
    "string_contains", "string_count", "string_trim_start", "string_trim_end",
    "string_is_alpha", "string_is_digit", "string_is_alphanumeric_uni",
    "base64_encode", "base64_decode", "datetime_to_iso", "datetime_from_parts",
];

/// Same audit, Option-returning (constructor-built sums).
pub(crate) const SCALAR_TEXT_SUM_BUILDERS: &[&str] = &["string_index_of"];

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
