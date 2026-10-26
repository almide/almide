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
    "float64_to_string",
    "float_to_float64", "float_to_int",
    "float_to_uint64", "float_to_uint64_saturating",
    "int16_max_value", "int16_min_value",
    "int16_to_string",
    "int32_max_value", "int32_min_value",
    "int32_to_string",
    "int64_max_value", "int64_min_value",
    "int64_to_string",
    "int8_max_value", "int8_min_value",
    "int8_to_string",
    "int_from_int16", "int_from_int32",
    "int_from_int64", "int_from_int8", "int_from_uint16", "int_from_uint32", "int_from_uint64",
    "int_from_uint64_saturating", "int_from_uint8", "int_max_value", "int_min_value",
    "int_to_int16", "int_to_int16_saturating", "int_to_int32", "int_to_int32_saturating",
    "int_to_int64", "int_to_int8", "int_to_int8_saturating", "int_to_uint16",
    "int_to_uint16_saturating", "int_to_uint32", "int_to_uint32_saturating", "int_to_uint64",
    "int_to_uint64_saturating", "int_to_uint8", "int_to_uint8_saturating", "uint16_max_value",
    "uint16_min_value",
    "uint16_to_string",
    "uint32_max_value", "uint32_min_value",
    "uint32_to_string",
    "uint64_max_value", "uint64_min_value", "uint64_to_float64",
    "uint64_to_string",
    "uint8_max_value", "uint8_min_value",
    "uint8_to_string",
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
    // int_bitcount / int_bits / float_bits / int_to_float (#1423 stage 4,
    // audited 2026-09-03): the bit-introspection family is the prim
    // bitwise floor only (bshr/band/bshl/bor over the i64 word — the
    // int_rotate class), int_bits_to_f32 is the f32 reinterpret prim
    // and int_to_float32 the direct i64 → f32 convert (single rounding,
    // native's `n as f32`; the Float32 rides the widened f64 carrier).
    // No loads, no stores, no handles. Parity evidence:
    // spec/wasm_cross/int_bit_family.almd.
    "int_pop_count", "int_count_trailing_zeros", "int_count_leading_zeros", "int_bit_width",
    "int_log2_floor", "int_log2_ceil", "int_next_power_of_two", "int_prev_power_of_two",
    "int_byte_swap", "int_bit_reverse", "int_bits_to_f32", "int_to_float32",
    "hash_sha256", "hash_sha256_hex", "hex_encode", "hex_encode_upper",
    "string_contains", "string_count", "string_trim_start", "string_trim_end",
    "string_is_alpha", "string_is_digit", "string_is_alphanumeric_uni", "string_is_upper",
    "string_is_lower",
    "base64_encode", "base64_encode_url",
    "datetime_add_days", "datetime_add_hours", "datetime_add_minutes", "datetime_add_seconds", "datetime_day", "datetime_format", "datetime_hour", "datetime_minute", "datetime_month", "datetime_second", "datetime_to_iso", "datetime_weekday", "datetime_year",
    "string_is_whitespace",
];

/// Same audit, Option/Result-returning (constructor-built sums).
/// hex_decode (#1423 stage 4): its `Result[Bytes, String]` is built via
/// ok()/err() — read-only loads on the digest-shared string layout and
/// stores into its own prim.alloc_bytes buffer, the base64_decode class
/// (it sat in the plain tier, where the coupled-type proxy walled it).
pub(crate) const SCALAR_TEXT_SUM_BUILDERS: &[&str] = &[
    "string_index_of", "string_last_index_of", "base64_decode", "base64_decode_url", "hex_decode",
];

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
    // The list-default quartet (develop's DDD-arc addition, synced with
    // the front pin): pure composition of already-admitted fns —
    // value.__field + the native-twinned __is_null + __decode_list_T.
    "__decode_default_list_int", "__decode_default_list_float",
    "__decode_default_list_bool", "__decode_default_list_string",
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
    "bytes_read_i64_le_array", "bytes_read_u16_be_array",
    "bytes_read_u16_le_array", "bytes_read_u32_be_array", "bytes_read_u32_le_array",
    // bytes_append_multi.almd cursor tail (#1099): the __bam grow-append
    // shape — prim.load32(handle+4) is the Bytes LEN header (len = bytes
    // on both legs); same audit as the append_* family above.
    "bytes_write_bool",
    // bytes_core.almd search/edit/predicate family (#1423 stage 4, audited
    // 2026-09-03): read-only load8 walks over the digest-shared Bytes
    // payload (handle+12 = OUR PAYLOAD, load32(h+4) = the byte length on
    // both legs), the editors (insert / remove_at / reverse / xor /
    // map_each) store into their own prim.alloc_bytes buffer only, the
    // predicates and cmp/skip/eof are scalar results, data_ptr is the
    // payload address (never printed cross-leg). map_each's callback is
    // a language-level closure call this emitter lowers itself. Parity
    // evidence: spec/wasm_cross/bytes_search_edit_family.almd.
    "bytes_cmp", "bytes_contains", "bytes_data_ptr", "bytes_ends_with", "bytes_eof",
    "bytes_insert", "bytes_is_empty", "bytes_is_valid_utf8", "bytes_map_each",
    "bytes_remove_at", "bytes_reverse", "bytes_skip", "bytes_starts_with", "bytes_xor",
];

/// The exempt-tier members of the same audit (tuple/Option returners
/// with literal-built sums).
pub(crate) const BYTES_FAMILY_SUM: &[&str] = &[
    // bytes_cursor.almd `_at` readers: (Int, T?) tuples built as literals.
    "bytes_read_bool_at", "bytes_read_f16_le_at", "bytes_read_f32_be_at", "bytes_read_f32_le_at",
    "bytes_read_f64_be_at", "bytes_read_f64_le_at", "bytes_read_i16_be_at", "bytes_read_i16_le_at",
    "bytes_read_i32_be_at", "bytes_read_i32_le_at", "bytes_read_i64_be_at", "bytes_read_i64_le_at",
    "bytes_read_u16_be_at",
    "bytes_read_u16_le_at", "bytes_read_u32_be_at", "bytes_read_u32_le_at", "bytes_read_u8_at",
    "bytes_take_at",
    // bytes_core.almd index_of (#1423 stage 4): the same read-only walk as
    // contains, its `Int?` built via some()/none in tail position.
    "bytes_index_of",
    // regex_engine.almd (audited 2026-08-25): the backtracking engine —
    // byte walks on the digest-shared String layout (load32(h+4) = len
    // in BYTES both legs, handle+12 = OUR payload), prim-MEDIATED
    // alloc_list/alloc_str scratch (the hash_impl precedent: 8-byte Int
    // slots are layout-identical), save/caps stores land in its own
    // scratch blocks only.
    "regex_is_match", "regex_full_match", "regex_find_all", "regex_replace",
    "regex_replace_first", "regex_split",
    // the Regex surface's engine half (#3942): the validator and its abort
    // (prim.die, as float clamp), the scan captures_all and replace_with
    // read, index_of's code-point count — the same byte walks and scratch.
    "regex_check", "regex_new", "regex_index_of", "regex_scan",
    // bytes_rawptr.almd / bytes_skip.almd (audited 2026-08-25):
    // raw-address bridge over load8/store8 on Bytes data regions
    // (len = bytes both legs), handle+12 = OUR PAYLOAD; the empty-buffer
    // NULL guard is the 2026-08-10 fuzz doctrine.
    "bytes_as_ptr", "bytes_as_mut_ptr", "bytes_from_raw_ptr", "bytes_copy_to_ptr",
    // ffi.to_int (R-ffi-01): the identity ptr_to_int cast. The other ffi
    // bodies are prim-free calls to the bridge above (ffi_bridge.almd).
    "ffi_to_int",
    // http_response.almd (audited 2026-08-25): PURE Almide over the
    // published List[String] rep — list/string surfaces only, the RFC
    // 9110 ASCII fold spelled with chars/index_of/slice. get_header
    // (Option ret) sits in the SUM tier below.
    "http_response", "http_json", "http_redirect", "http_with_headers",
    "http_status", "http_body", "http_body_bytes", "http_set_header",
    // The #1791 read side (audited 2026-09-03): the same list/string
    // surface plus int.parse / map.new / map.contains / map.set.
    "http_status_code", "http_headers_flat", "http_header_values",
    // http_request.almd (#2588): the request rep, the same List[String]
    // discipline and list/string/map surfaces as http_response; req_header /
    // param (Option ret) sit in the SUM tier below.
    "http_new_request", "http_req_method", "http_req_path", "http_req_body",
    "http_req_with_params", "http_req_with_path", "http_set_body",
    // The request's HttpMessage twins (R-http-12): the same list surface.
    "http_req_header_values", "http_req_headers_flat", "http_req_set_header", "http_req_set_body",
    // random_int.almd (audited 2026-08-25): prim.alloc_bytes scratch +
    // prim.random_get (the op-32 entropy boundary) + pure span math —
    // the VALUE is nondeterministic by contract (C-112 pins the range).
    "random_int",
    // random_bytes.almd (C-394): the same op-32 boundary in literal 256- and
    // 8-byte draws into a fresh n-byte Bytes, the tail by prim.store8 from an
    // 8-byte scratch, a T6 prim.die on a negative n; C-394 pins the lengths
    // and two draws differing, not the bytes.
    "random_bytes",
    // json_path.almd, READ side only (audited 2026-08-25): the value.Path
    // rep is a plain List[String]; get walks value.__field / value.as_array /
    // list.get — all native arms here, no raw layout reads. set/remove
    // are REJECTED: their pair walks read the incumbent's inline-pairs
    // Value layout (tag@h+4, count@h+8) — see PORT-MATRIX.
    "json_path_index",
    // value.__set_path_steps (the wasm body of value.set_path's walk): pure
    // Almide over value.as_object / as_array and the constructors, no layout
    // reads — unlike json_path_set_go below the REJECTED line.
    "value_set_path_steps",
    "http_get_header", "http_req_header", "http_param",
    // regex_engine.almd Option returners (same audit).
    "regex_find", "regex_captures",
    // zlib_inflate.almd / zlib_deflate.almd (#1700, audited 2026-09-01):
    // the C-326 gzip decoder promoted to stdlib plus the stored-block
    // encoder. Every Result is built via language-level ok()/err(); the
    // buffers are bytes.zeros/bytes.push/bytes.get (public surface — the
    // exact construction the C-326 fixture already runs byte-identical on
    // the structural leg); no prim access at all. Parity evidence:
    // spec/wasm_cross/zlib_selfhost.almd (known-answer decode + round
    // trips, C-331).
    "zlib_gunzip", "zlib_inflate", "zlib_decompress",
    "zlib_deflate", "zlib_deflate_level", "zlib_compress", "zlib_compress_level",
    "zlib_gzip",
];

/// The http call handle (#2633, audited 2026-09-26): stdlib/http_call.almd
/// — language surface only (string interpolation and slicing, list.map/fold,
/// int.parse, `??`, ok()/err()/some()/none ctors); the only leaves are the op-53..=58 host calls this emitter lowers
/// itself (calls.rs), and `HttpCall` is this emitter's own one-slot block.
/// Result / Option returns and the Map headers param trip the coupled proxy;
/// the bodies never touch a raw layout.
pub(crate) const HTTP_CALL_SUM: &[&str] = &[
    "__hc_start_impl", "__hc_poll_impl",
    "__hc_read_new_impl", "__hc_wait_impl", "__hc_cancel_impl",
];

/// The http.serve family (#2650, audited 2026-09-26): stdlib/http_serve.almd —
/// PURE language surface over the published List[String] HttpRequest /
/// HttpResponse reps (list.get/drop/chunk/find, string.split/index_of/take/
/// drop/chars, map.new/set, option.flat_map, a `while`
/// loop, the handler through call_indirect); the only leaves are the
/// op-70/71/72 host calls the emitter lowers itself (calls.rs). Result /
/// Option returns build through language-level constructors.
pub(crate) const HTTP_SERVE_SUM: &[&str] = &[
    "http_serve", "http_req_method", "http_req_path", "http_req_body", "http_req_header",
];

/// The subprocess family (#2589, ADR-0025, audited 2026-10-03):
/// stdlib/process_wasm.almd — language surface only (string interpolation and
/// slicing, list.map/fold, int.parse/to_string, `??`, the ProcessStatus record
/// literal, ok()); the only leaves are the op-80..=90 host calls this emitter
/// lowers itself (calls.rs). Result returns build through language-level
/// constructors; the bodies never touch a raw layout.
pub(crate) const PROCESS_SUM: &[&str] = &[
    "__process_exec_impl", "__process_output_impl", "__process_run_impl",
    "__process_kill_impl", "__process_pid_impl",
];

/// The vendored-libm family (C-305): every body is a FAITHFUL
/// transcription of libm 0.2.16 with constants via prim.ffrombits —
/// pure scalar prims throughout; math_trig's coefficient tables go
/// through prim-MEDIATED alloc_list_f64 with 8-byte Float slot stores
/// (the one list class both layouts share). One vendored libm on every
/// target is the bit-parity mechanism itself.
pub(crate) const MATH_VERIFIED: &[&str] = &[
    "math_atan", "math_choose", "math_cos", "math_e", "math_exp",
    "math_factorial", "math_fpow", "math_log", "math_log10",
    "math_log2", "math_log_gamma", "math_pi", "math_pow",
    "math_sign", "math_sin", "math_sqrt", "math_tan", "math_tanh",
];

/// Option-returning members (the checked trio cells).
pub(crate) const SIZED_CONVERT_SUM_BUILDERS: &[&str] = &[
    "int_from_uint64_checked", "int_to_int16_checked", "int_to_int32_checked",
    "int_to_int8_checked", "int_to_uint16_checked", "int_to_uint32_checked",
    "int_to_uint64_checked", "int_to_uint8_checked",
];

/// The matrix compositions (#1423 stage 4): stdlib/matrix_fused.almd is
/// PRIM-FREE — every body is written at the stdlib `Matrix` signature and
/// composes public `matrix.*` calls, each of which THIS emitter lowers as
/// a native arm on its own flat layout (matrix.rs / matrix_elem.rs /
/// matrix_prod.rs / matrix_shape.rs). No block is read or written by the
/// body itself, so no incumbent layout can leak in. Native spells each of
/// these as the same composition (matrix.rs: mul_scaled = scale∘mul, the
/// f32 family delegating to the f64 ops). fma / fma3 left this list for
/// their own one-pass arm (matrix_elem.rs `lower_matrix_fma`, #3644): the
/// composition walked each whole operand before truncating to the overlap.
/// Parity evidence: spec/wasm_cross/matrix_composition_family.almd.
pub(crate) const MATRIX_COMPOSITIONS: &[&str] = &[
    "matrix_mul_scaled",
    "matrix_append_rows",
];

/// Every admitted tier: its members and whether its bodies are exempt
/// from the coupled-type proxy (the SUM tiers build their sums through
/// language-level constructors this emitter lowers with its own layout).
const TIERS: &[(&[&str], bool)] = &[
    (SIZED_CONVERT_VERIFIED, false),
    (SIZED_CONVERT_SUM_BUILDERS, true),
    (SCALAR_TEXT_VERIFIED, false),
    (SCALAR_TEXT_SUM_BUILDERS, true),
    (MATH_VERIFIED, false),
    (CODEC_ENCODE_VERIFIED, true),
    (BYTES_FAMILY_VERIFIED, false),
    (BYTES_FAMILY_SUM, true),
    (HTTP_CALL_SUM, true),
    (HTTP_SERVE_SUM, true),
    (PROCESS_SUM, true),
    (MATRIX_COMPOSITIONS, false),
];

/// The admission of a registry impl through the tiers above: `None` when
/// no tier lists it, else `Some(exempt)` — exempt when ANY listing tier is.
/// A body declared with `@implements` whose file never reaches `prim.` is
/// admitted, exempt, without a tier (#3620): it builds every value through
/// surface this emitter lowers on its own layout, which is what each tier's
/// audit establishes by hand. The tiers are legacy lists that only shrink.
pub(crate) fn tier_of(impl_fn: &str) -> Option<bool> {
    let mut hits = TIERS.iter().filter(|(members, _)| members.contains(&impl_fn)).peekable();
    if hits.peek().is_none() {
        return almide_types::implements::prim_free(impl_fn).then_some(true);
    }
    Some(hits.any(|(_, exempt)| *exempt))
}
