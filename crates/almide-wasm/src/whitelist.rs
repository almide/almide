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
