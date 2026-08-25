# The Port Matrix (generated — scripts/gen-port-matrix.py)

Registry impls: 1236 — linked 435, rejected 9, unreached 792.

An `unreached` row is an impl no admission decision has touched:
its surface either has a NATIVE arm in the emitter or stays an
honest wall in the burn-up histogram. Nothing links silently.

| impl | surface | decision |
|---|---|---|
| __decode_default_bool | __decode_default_bool | linked (codec) |
| __decode_default_float | __decode_default_float | linked (codec) |
| __decode_default_int | __decode_default_int | linked (codec) |
| __decode_default_string | __decode_default_string | linked (codec) |
| __decode_list_bool | __decode_list_bool | linked (codec) |
| __decode_list_float | __decode_list_float | linked (codec) |
| __decode_list_int | __decode_list_int | linked (codec) |
| __decode_list_string | __decode_list_string | linked (codec) |
| __decode_option_bool | __decode_option_bool | linked (codec) |
| __decode_option_float | __decode_option_float | linked (codec) |
| __decode_option_int | __decode_option_int | linked (codec) |
| __decode_option_string | __decode_option_string | linked (codec) |
| __encode_list_bool | __encode_list_bool | linked (codec) |
| __encode_list_float | __encode_list_float | linked (codec) |
| __encode_list_int | __encode_list_int | linked (codec) |
| __encode_list_string | __encode_list_string | linked (codec) |
| __encode_option_bool | __encode_option_bool | linked (codec) |
| __encode_option_float | __encode_option_float | linked (codec) |
| __encode_option_int | __encode_option_int | linked (codec) |
| __encode_option_string | __encode_option_string | linked (codec) |
| __eprintln_str | eprintln | unreached (honest wall / native arm covers the surface) |
| __list_append1 | __list_append1 | unreached (honest wall / native arm covers the surface) |
| __list_append1_rc | __list_append1_rc | unreached (honest wall / native arm covers the surface) |
| __list_concat | __list_concat | unreached (honest wall / native arm covers the surface) |
| __list_concat_rc | __list_concat_rc | unreached (honest wall / native arm covers the surface) |
| __str_append1 | __str_append1 | unreached (honest wall / native arm covers the surface) |
| __str_concat | __str_concat | unreached (honest wall / native arm covers the surface) |
| base64_decode | base64.decode | linked (scalar/text SUM) |
| base64_decode_url | base64.decode_url | linked (scalar/text SUM) |
| base64_encode | base64.encode | linked (scalar/text) |
| base64_encode_url | base64.encode_url | linked (scalar/text) |
| bool_to_string | bool.to_string | unreached (honest wall / native arm covers the surface) |
| bytes_append | bytes.append | unreached (honest wall / native arm covers the surface) |
| bytes_append_f32_be | bytes.append_f32_be | linked (bytes family) |
| bytes_append_f32_le | bytes.append_f32_le | linked (bytes family) |
| bytes_append_f64_be | bytes.append_f64_be | linked (bytes family) |
| bytes_append_f64_le | bytes.append_f64_le | linked (bytes family) |
| bytes_append_i16_be | bytes.append_i16_be | linked (bytes family) |
| bytes_append_i16_le | bytes.append_i16_le | linked (bytes family) |
| bytes_append_i32_be | bytes.append_i32_be | linked (bytes family) |
| bytes_append_i32_le | bytes.append_i32_le | linked (bytes family) |
| bytes_append_i64_be | bytes.append_i64_be | linked (bytes family) |
| bytes_append_i64_le | bytes.append_i64_le | linked (bytes family) |
| bytes_append_u16_be | bytes.append_u16_be | linked (bytes family) |
| bytes_append_u16_le | bytes.append_u16_le | linked (bytes family) |
| bytes_append_u32_be | bytes.append_u32_be | linked (bytes family) |
| bytes_append_u32_le | bytes.append_u32_le | linked (bytes family) |
| bytes_as_mut_ptr | bytes.as_mut_ptr | linked (bytes family SUM) |
| bytes_as_ptr | bytes.as_ptr | linked (bytes family SUM) |
| bytes_chunks | bytes.chunks | unreached (honest wall / native arm covers the surface) |
| bytes_clear | bytes.clear | unreached (honest wall / native arm covers the surface) |
| bytes_cmp | bytes.cmp | unreached (honest wall / native arm covers the surface) |
| bytes_concat | bytes.concat | unreached (honest wall / native arm covers the surface) |
| bytes_contains | bytes.contains | unreached (honest wall / native arm covers the surface) |
| bytes_copy_from | bytes.copy_from | unreached (honest wall / native arm covers the surface) |
| bytes_copy_to_ptr | bytes.copy_to_ptr | linked (bytes family SUM) |
| bytes_copy_within | bytes.copy_within | unreached (honest wall / native arm covers the surface) |
| bytes_data_ptr | bytes.data_ptr | unreached (honest wall / native arm covers the surface) |
| bytes_ends_with | bytes.ends_with | unreached (honest wall / native arm covers the surface) |
| bytes_eof | bytes.eof | unreached (honest wall / native arm covers the surface) |
| bytes_fill | bytes.fill | unreached (honest wall / native arm covers the surface) |
| bytes_from_list | bytes.from_list | unreached (honest wall / native arm covers the surface) |
| bytes_from_raw_ptr | bytes.from_raw_ptr | linked (bytes family SUM) |
| bytes_from_string | bytes.from_string | unreached (honest wall / native arm covers the surface) |
| bytes_get | bytes.get | unreached (honest wall / native arm covers the surface) |
| bytes_get_or | bytes.get_or | unreached (honest wall / native arm covers the surface) |
| bytes_heap_restore | bytes.heap_restore | linked (bytes family SUM) |
| bytes_heap_save | bytes.heap_save | linked (bytes family SUM) |
| bytes_index | bytes.index | unreached (honest wall / native arm covers the surface) |
| bytes_index_of | bytes.index_of | unreached (honest wall / native arm covers the surface) |
| bytes_insert | bytes.insert | unreached (honest wall / native arm covers the surface) |
| bytes_is_empty | bytes.is_empty | unreached (honest wall / native arm covers the surface) |
| bytes_is_valid_utf8 | bytes.is_valid_utf8 | unreached (honest wall / native arm covers the surface) |
| bytes_len | bytes.len | unreached (honest wall / native arm covers the surface) |
| bytes_lines | bytes.lines | unreached (honest wall / native arm covers the surface) |
| bytes_map_each | bytes.map_each | unreached (honest wall / native arm covers the surface) |
| bytes_new | bytes.new | unreached (honest wall / native arm covers the surface) |
| bytes_pad_left | bytes.pad_left | unreached (honest wall / native arm covers the surface) |
| bytes_pad_right | bytes.pad_right | unreached (honest wall / native arm covers the surface) |
| bytes_read_bool | bytes.read_bool | unreached (honest wall / native arm covers the surface) |
| bytes_read_bool_at | bytes.read_bool_at | linked (bytes family SUM) |
| bytes_read_f16_le | bytes.read_f16_le | unreached (honest wall / native arm covers the surface) |
| bytes_read_f16_le_array | bytes.read_f16_le_array | linked (bytes family) |
| bytes_read_f16_le_at | bytes.read_f16_le_at | linked (bytes family SUM) |
| bytes_read_f32_be | bytes.read_f32_be | unreached (honest wall / native arm covers the surface) |
| bytes_read_f32_be_array | bytes.read_f32_be_array | linked (bytes family) |
| bytes_read_f32_be_at | bytes.read_f32_be_at | linked (bytes family SUM) |
| bytes_read_f32_le | bytes.read_f32_le | unreached (honest wall / native arm covers the surface) |
| bytes_read_f32_le_array | bytes.read_f32_le_array | linked (bytes family) |
| bytes_read_f32_le_at | bytes.read_f32_le_at | linked (bytes family SUM) |
| bytes_read_f64_be | bytes.read_f64_be | unreached (honest wall / native arm covers the surface) |
| bytes_read_f64_be_array | bytes.read_f64_be_array | linked (bytes family) |
| bytes_read_f64_be_at | bytes.read_f64_be_at | linked (bytes family SUM) |
| bytes_read_f64_le | bytes.read_f64_le | unreached (honest wall / native arm covers the surface) |
| bytes_read_f64_le_array | bytes.read_f64_le_array | linked (bytes family) |
| bytes_read_f64_le_at | bytes.read_f64_le_at | linked (bytes family SUM) |
| bytes_read_float32 | bytes.read_float32 | linked (bytes family) |
| bytes_read_i16_be | bytes.read_i16_be | unreached (honest wall / native arm covers the surface) |
| bytes_read_i16_be_array | bytes.read_i16_be_array | linked (bytes family) |
| bytes_read_i16_be_at | bytes.read_i16_be_at | linked (bytes family SUM) |
| bytes_read_i16_le | bytes.read_i16_le | unreached (honest wall / native arm covers the surface) |
| bytes_read_i16_le_array | bytes.read_i16_le_array | linked (bytes family) |
| bytes_read_i16_le_at | bytes.read_i16_le_at | linked (bytes family SUM) |
| bytes_read_i32_be | bytes.read_i32_be | unreached (honest wall / native arm covers the surface) |
| bytes_read_i32_be_array | bytes.read_i32_be_array | linked (bytes family) |
| bytes_read_i32_be_at | bytes.read_i32_be_at | linked (bytes family SUM) |
| bytes_read_i32_le | bytes.read_i32_le | unreached (honest wall / native arm covers the surface) |
| bytes_read_i32_le_array | bytes.read_i32_le_array | linked (bytes family) |
| bytes_read_i32_le_at | bytes.read_i32_le_at | linked (bytes family SUM) |
| bytes_read_i64_be | bytes.read_i64_be | unreached (honest wall / native arm covers the surface) |
| bytes_read_i64_be_array | bytes.read_i64_be_array | linked (bytes family) |
| bytes_read_i64_be_at | bytes.read_i64_be_at | linked (bytes family SUM) |
| bytes_read_i64_le | bytes.read_i64_le | unreached (honest wall / native arm covers the surface) |
| bytes_read_i64_le_array | bytes.read_i64_le_array | linked (bytes family) |
| bytes_read_i64_le_at | bytes.read_i64_le_at | linked (bytes family SUM) |
| bytes_read_int32 | bytes.read_int32 | linked (bytes family) |
| bytes_read_length_prefixed_strings_le | bytes.read_length_prefixed_strings_le | REJECTED: 8-byte List[String] slot stores (store_str at i*8) — native decoder instead |
| bytes_read_string_at | bytes.read_string_at | linked (bytes family SUM) |
| bytes_read_string_be | bytes.read_string_be | linked (bytes family) |
| bytes_read_string_be_at | bytes.read_string_be_at | linked (bytes family SUM) |
| bytes_read_u16_be | bytes.read_u16_be | unreached (honest wall / native arm covers the surface) |
| bytes_read_u16_be_array | bytes.read_u16_be_array | linked (bytes family) |
| bytes_read_u16_be_at | bytes.read_u16_be_at | linked (bytes family SUM) |
| bytes_read_u16_le | bytes.read_u16_le | unreached (honest wall / native arm covers the surface) |
| bytes_read_u16_le_array | bytes.read_u16_le_array | linked (bytes family) |
| bytes_read_u16_le_at | bytes.read_u16_le_at | linked (bytes family SUM) |
| bytes_read_u32_be | bytes.read_u32_be | unreached (honest wall / native arm covers the surface) |
| bytes_read_u32_be_array | bytes.read_u32_be_array | linked (bytes family) |
| bytes_read_u32_be_at | bytes.read_u32_be_at | linked (bytes family SUM) |
| bytes_read_u32_le | bytes.read_u32_le | unreached (honest wall / native arm covers the surface) |
| bytes_read_u32_le_array | bytes.read_u32_le_array | linked (bytes family) |
| bytes_read_u32_le_at | bytes.read_u32_le_at | linked (bytes family SUM) |
| bytes_read_u8 | bytes.read_u8 | unreached (honest wall / native arm covers the surface) |
| bytes_read_u8_at | bytes.read_u8_at | linked (bytes family SUM) |
| bytes_read_uint16 | bytes.read_uint16 | linked (bytes family) |
| bytes_read_uint32 | bytes.read_uint32 | linked (bytes family) |
| bytes_remove_at | bytes.remove_at | unreached (honest wall / native arm covers the surface) |
| bytes_repeat | bytes.repeat | unreached (honest wall / native arm covers the surface) |
| bytes_reverse | bytes.reverse | unreached (honest wall / native arm covers the surface) |
| bytes_set | bytes.set | unreached (honest wall / native arm covers the surface) |
| bytes_set_at | bytes.set_at | unreached (honest wall / native arm covers the surface) |
| bytes_set_f32_be | bytes.set_f32_be | unreached (honest wall / native arm covers the surface) |
| bytes_set_f32_le | bytes.set_f32_le | unreached (honest wall / native arm covers the surface) |
| bytes_set_f64_be | bytes.set_f64_be | unreached (honest wall / native arm covers the surface) |
| bytes_set_f64_le | bytes.set_f64_le | unreached (honest wall / native arm covers the surface) |
| bytes_set_float32 | bytes.set_float32 | linked (bytes family) |
| bytes_set_i16_be | bytes.set_i16_be | unreached (honest wall / native arm covers the surface) |
| bytes_set_i16_le | bytes.set_i16_le | unreached (honest wall / native arm covers the surface) |
| bytes_set_i32_be | bytes.set_i32_be | unreached (honest wall / native arm covers the surface) |
| bytes_set_i32_le | bytes.set_i32_le | unreached (honest wall / native arm covers the surface) |
| bytes_set_i64_be | bytes.set_i64_be | unreached (honest wall / native arm covers the surface) |
| bytes_set_i64_le | bytes.set_i64_le | unreached (honest wall / native arm covers the surface) |
| bytes_set_int32 | bytes.set_int32 | linked (bytes family) |
| bytes_set_u16_be | bytes.set_u16_be | unreached (honest wall / native arm covers the surface) |
| bytes_set_u16_le | bytes.set_u16_le | unreached (honest wall / native arm covers the surface) |
| bytes_set_u32_be | bytes.set_u32_be | unreached (honest wall / native arm covers the surface) |
| bytes_set_u32_le | bytes.set_u32_le | unreached (honest wall / native arm covers the surface) |
| bytes_set_u8 | bytes.set_u8 | unreached (honest wall / native arm covers the surface) |
| bytes_set_uint16 | bytes.set_uint16 | linked (bytes family) |
| bytes_set_uint32 | bytes.set_uint32 | linked (bytes family) |
| bytes_skip | bytes.skip | unreached (honest wall / native arm covers the surface) |
| bytes_skip_length_prefixed_le | bytes.skip_length_prefixed_le | linked (bytes family SUM) |
| bytes_slice | bytes.slice | unreached (honest wall / native arm covers the surface) |
| bytes_split | bytes.split | unreached (honest wall / native arm covers the surface) |
| bytes_starts_with | bytes.starts_with | unreached (honest wall / native arm covers the surface) |
| bytes_take_at | bytes.take_at | linked (bytes family SUM) |
| bytes_to_list | bytes.to_list | unreached (honest wall / native arm covers the surface) |
| bytes_to_string | bytes.to_string | unreached (honest wall / native arm covers the surface) |
| bytes_to_string_lossy | bytes.to_string_lossy | unreached (honest wall / native arm covers the surface) |
| bytes_write_bool | bytes.write_bool | linked (bytes family) |
| bytes_write_float32 | bytes.write_float32 | linked (bytes family) |
| bytes_write_int32 | bytes.write_int32 | linked (bytes family) |
| bytes_write_string_be | bytes.write_string_be | linked (bytes family) |
| bytes_write_uint16 | bytes.write_uint16 | linked (bytes family) |
| bytes_write_uint32 | bytes.write_uint32 | linked (bytes family) |
| bytes_xor | bytes.xor | unreached (honest wall / native arm covers the surface) |
| datetime_add_days | datetime.add_days | linked (scalar/text) |
| datetime_add_hours | datetime.add_hours | linked (scalar/text) |
| datetime_add_minutes | datetime.add_minutes | linked (scalar/text) |
| datetime_add_seconds | datetime.add_seconds | linked (scalar/text) |
| datetime_day | datetime.day | linked (scalar/text) |
| datetime_diff_seconds | datetime.diff_seconds | linked (scalar/text) |
| datetime_format | datetime.format | linked (scalar/text) |
| datetime_from_parts | datetime.from_parts | linked (scalar/text) |
| datetime_from_unix | datetime.from_unix | linked (scalar/text) |
| datetime_hour | datetime.hour | linked (scalar/text) |
| datetime_is_after | datetime.is_after | linked (scalar/text) |
| datetime_is_before | datetime.is_before | linked (scalar/text) |
| datetime_minute | datetime.minute | linked (scalar/text) |
| datetime_monotonic_ns | datetime.monotonic_ns | unreached (honest wall / native arm covers the surface) |
| datetime_month | datetime.month | linked (scalar/text) |
| datetime_now | datetime.now | unreached (honest wall / native arm covers the surface) |
| datetime_parse_iso | datetime.parse_iso | unreached (honest wall / native arm covers the surface) |
| datetime_second | datetime.second | linked (scalar/text) |
| datetime_to_iso | datetime.to_iso | linked (scalar/text) |
| datetime_to_unix | datetime.to_unix | linked (scalar/text) |
| datetime_weekday | datetime.weekday | linked (scalar/text) |
| datetime_year | datetime.year | linked (scalar/text) |
| env_args | env.args | unreached (honest wall / native arm covers the surface) |
| env_cwd | env.cwd | unreached (honest wall / native arm covers the surface) |
| env_get | env.get | unreached (honest wall / native arm covers the surface) |
| env_millis | env.millis | unreached (honest wall / native arm covers the surface) |
| env_os | env.os | unreached (honest wall / native arm covers the surface) |
| env_temp_dir | env.temp_dir | unreached (honest wall / native arm covers the surface) |
| env_unix_timestamp | env.unix_timestamp | unreached (honest wall / native arm covers the surface) |
| error_chain | error.chain | unreached (honest wall / native arm covers the surface) |
| error_context | error.context | unreached (honest wall / native arm covers the surface) |
| error_message | error.message | unreached (honest wall / native arm covers the surface) |
| fan_any | fan.any_map | unreached (honest wall / native arm covers the surface) |
| fan_any_ff | fan.any_map_ff | unreached (honest wall / native arm covers the surface) |
| fan_any_fi | fan.any_map_fi | unreached (honest wall / native arm covers the surface) |
| fan_any_fs | fan.any_map_fs | unreached (honest wall / native arm covers the surface) |
| fan_any_if | fan.any_map_if | unreached (honest wall / native arm covers the surface) |
| fan_any_is | fan.any_map_is | unreached (honest wall / native arm covers the surface) |
| fan_any_sf | fan.any_map_sf | unreached (honest wall / native arm covers the surface) |
| fan_any_si | fan.any_map_si | unreached (honest wall / native arm covers the surface) |
| fan_any_ss | fan.any_map_ss | unreached (honest wall / native arm covers the surface) |
| fan_map | fan.map | unreached (honest wall / native arm covers the surface) |
| fan_map_ff | fan.map_ff | unreached (honest wall / native arm covers the surface) |
| fan_map_fi | fan.map_fi | unreached (honest wall / native arm covers the surface) |
| fan_map_fs | fan.map_fs | unreached (honest wall / native arm covers the surface) |
| fan_map_if | fan.map_if | unreached (honest wall / native arm covers the surface) |
| fan_map_is | fan.map_is | unreached (honest wall / native arm covers the surface) |
| fan_map_sf | fan.map_sf | unreached (honest wall / native arm covers the surface) |
| fan_map_si | fan.map_si | unreached (honest wall / native arm covers the surface) |
| fan_map_ss | fan.map_ss | unreached (honest wall / native arm covers the surface) |
| float32_to_float64 | float32.to_float64 | unreached (honest wall / native arm covers the surface) |
| float32_to_int16 | float32.to_int16 | unreached (honest wall / native arm covers the surface) |
| float32_to_int32 | float32.to_int32 | unreached (honest wall / native arm covers the surface) |
| float32_to_int64 | float32.to_int64 | unreached (honest wall / native arm covers the surface) |
| float32_to_int8 | float32.to_int8 | unreached (honest wall / native arm covers the surface) |
| float32_to_uint16 | float32.to_uint16 | unreached (honest wall / native arm covers the surface) |
| float32_to_uint32 | float32.to_uint32 | unreached (honest wall / native arm covers the surface) |
| float32_to_uint64 | float32.to_uint64 | unreached (honest wall / native arm covers the surface) |
| float32_to_uint8 | float32.to_uint8 | unreached (honest wall / native arm covers the surface) |
| float64_to_float32 | float64.to_float32 | unreached (honest wall / native arm covers the surface) |
| float64_to_int16 | float64.to_int16 | linked (sized-convert) |
| float64_to_int32 | float64.to_int32 | linked (sized-convert) |
| float64_to_int64 | float64.to_int64 | linked (sized-convert) |
| float64_to_int8 | float64.to_int8 | linked (sized-convert) |
| float64_to_string | float64.to_string | linked (sized-convert) |
| float64_to_uint16 | float64.to_uint16 | linked (sized-convert) |
| float64_to_uint32 | float64.to_uint32 | linked (sized-convert) |
| float64_to_uint64 | float64.to_uint64 | linked (sized-convert) |
| float64_to_uint8 | float64.to_uint8 | linked (sized-convert) |
| float_abs | float.abs | linked (scalar/text) |
| float_ceil | float.ceil | unreached (honest wall / native arm covers the surface) |
| float_clamp | float.clamp | unreached (honest wall / native arm covers the surface) |
| float_floor | float.floor | linked (scalar/text) |
| float_from_float32 | float.from_float32 | linked (scalar/text) |
| float_from_float64 | float.from_float64 | linked (scalar/text) |
| float_from_int | float.from_int | unreached (honest wall / native arm covers the surface) |
| float_is_infinite | float.is_infinite | unreached (honest wall / native arm covers the surface) |
| float_is_nan | float.is_nan | linked (scalar/text) |
| float_max | float.max | unreached (honest wall / native arm covers the surface) |
| float_min | float.min | unreached (honest wall / native arm covers the surface) |
| float_parse | float.parse | linked (calls.rs SUM tier) |
| float_round | float.round | linked (scalar/text) |
| float_sign | float.sign | unreached (honest wall / native arm covers the surface) |
| float_sqrt | float.sqrt | unreached (honest wall / native arm covers the surface) |
| float_to_bits | float.to_bits | unreached (honest wall / native arm covers the surface) |
| float_to_fixed | float.to_fixed | linked (calls.rs VERIFIED) |
| float_to_float32 | float.to_float32 | linked (scalar/text) |
| float_to_float32_checked | float.to_float32_checked | unreached (honest wall / native arm covers the surface) |
| float_to_float64 | float.to_float64 | linked (sized-convert) |
| float_to_int | float.to_int | linked (sized-convert) |
| float_to_int16 | float.to_int16 | linked (sized-convert) |
| float_to_int16_checked | float.to_int16_checked | linked (sized-convert SUM) |
| float_to_int16_saturating | float.to_int16_saturating | linked (sized-convert) |
| float_to_int32 | float.to_int32 | linked (sized-convert) |
| float_to_int32_checked | float.to_int32_checked | linked (sized-convert SUM) |
| float_to_int32_saturating | float.to_int32_saturating | linked (sized-convert) |
| float_to_int64 | float.to_int64 | linked (sized-convert) |
| float_to_int64_checked | float.to_int64_checked | linked (sized-convert SUM) |
| float_to_int64_saturating | float.to_int64_saturating | linked (sized-convert) |
| float_to_int8 | float.to_int8 | linked (sized-convert) |
| float_to_int8_checked | float.to_int8_checked | linked (sized-convert SUM) |
| float_to_int8_saturating | float.to_int8_saturating | linked (sized-convert) |
| float_to_string | float.to_string | linked (calls.rs VERIFIED) |
| float_to_string_compound | float.to_string_compound | linked (calls.rs VERIFIED) |
| float_to_uint16 | float.to_uint16 | linked (sized-convert) |
| float_to_uint16_checked | float.to_uint16_checked | linked (sized-convert SUM) |
| float_to_uint16_saturating | float.to_uint16_saturating | linked (sized-convert) |
| float_to_uint32 | float.to_uint32 | linked (sized-convert) |
| float_to_uint32_checked | float.to_uint32_checked | linked (sized-convert SUM) |
| float_to_uint32_saturating | float.to_uint32_saturating | linked (sized-convert) |
| float_to_uint64 | float.to_uint64 | linked (sized-convert) |
| float_to_uint64_checked | float.to_uint64_checked | linked (sized-convert SUM) |
| float_to_uint64_saturating | float.to_uint64_saturating | linked (sized-convert) |
| float_to_uint8 | float.to_uint8 | linked (sized-convert) |
| float_to_uint8_checked | float.to_uint8_checked | linked (sized-convert SUM) |
| float_to_uint8_saturating | float.to_uint8_saturating | linked (sized-convert) |
| fs_append | fs.append | unreached (honest wall / native arm covers the surface) |
| fs_copy | fs.copy | unreached (honest wall / native arm covers the surface) |
| fs_create_temp_dir | fs.create_temp_dir | unreached (honest wall / native arm covers the surface) |
| fs_create_temp_file | fs.create_temp_file | unreached (honest wall / native arm covers the surface) |
| fs_exists | fs.exists | unreached (honest wall / native arm covers the surface) |
| fs_fallible_fold_lines_msi | fs.__fallible_fold_lines_msi | unreached (honest wall / native arm covers the surface) |
| fs_file_size | fs.file_size | unreached (honest wall / native arm covers the surface) |
| fs_fold_lines_chunked_i | fs.fold_lines_chunked_i | unreached (honest wall / native arm covers the surface) |
| fs_fold_lines_chunked_ls | fs.fold_lines_chunked_ls | unreached (honest wall / native arm covers the surface) |
| fs_fold_lines_chunked_msi | fs.fold_lines_chunked_msi | unreached (honest wall / native arm covers the surface) |
| fs_fold_lines_msi | fs.fold_lines_msi | unreached (honest wall / native arm covers the surface) |
| fs_fold_lines_range_ls | fs.fold_lines_range_ls | unreached (honest wall / native arm covers the surface) |
| fs_glob | fs.glob | unreached (honest wall / native arm covers the surface) |
| fs_is_dir | fs.is_dir | unreached (honest wall / native arm covers the surface) |
| fs_is_file | fs.is_file | unreached (honest wall / native arm covers the surface) |
| fs_is_symlink | fs.is_symlink | unreached (honest wall / native arm covers the surface) |
| fs_list_dir | fs.list_dir | unreached (honest wall / native arm covers the surface) |
| fs_mkdir_p | fs.mkdir_p | unreached (honest wall / native arm covers the surface) |
| fs_modified_at | fs.modified_at | unreached (honest wall / native arm covers the surface) |
| fs_read_bytes | fs.read_bytes | unreached (honest wall / native arm covers the surface) |
| fs_read_bytes_if_exists | fs.read_bytes_if_exists | unreached (honest wall / native arm covers the surface) |
| fs_read_bytes_raw | fs.read_bytes_raw | unreached (honest wall / native arm covers the surface) |
| fs_read_bytes_raw_if_exists | fs.read_bytes_raw_if_exists | unreached (honest wall / native arm covers the surface) |
| fs_read_lines | fs.read_lines | unreached (honest wall / native arm covers the surface) |
| fs_read_lines_if_exists | fs.read_lines_if_exists | unreached (honest wall / native arm covers the surface) |
| fs_read_text | fs.read_text | unreached (honest wall / native arm covers the surface) |
| fs_read_text_if_exists | fs.read_text_if_exists | unreached (honest wall / native arm covers the surface) |
| fs_remove | fs.remove | unreached (honest wall / native arm covers the surface) |
| fs_remove_all | fs.remove_all | unreached (honest wall / native arm covers the surface) |
| fs_rename | fs.rename | unreached (honest wall / native arm covers the surface) |
| fs_stat | fs.stat | unreached (honest wall / native arm covers the surface) |
| fs_temp_dir | fs.temp_dir | unreached (honest wall / native arm covers the surface) |
| fs_walk | fs.walk | unreached (honest wall / native arm covers the surface) |
| fs_write | fs.write | unreached (honest wall / native arm covers the surface) |
| fs_write_bytes | fs.write_bytes | unreached (honest wall / native arm covers the surface) |
| fs_write_bytes_raw | fs.write_bytes_raw | unreached (honest wall / native arm covers the surface) |
| hash_fnv1a32 | hash.fnv1a32 | linked (scalar/text) |
| hash_fnv1a32_bytes | hash.fnv1a32_bytes | linked (scalar/text) |
| hash_sha256 | hash.sha256 | linked (scalar/text) |
| hash_sha256_hex | hash.sha256_hex | linked (scalar/text) |
| hex_decode | hex.decode | linked (scalar/text) |
| hex_encode | hex.encode | linked (scalar/text) |
| hex_encode_upper | hex.encode_upper | linked (scalar/text) |
| http_body | http.body | linked (bytes family SUM) |
| http_get_header | http.get_header | unreached (honest wall / native arm covers the surface) |
| http_json | http.json | linked (bytes family SUM) |
| http_redirect | http.redirect | linked (bytes family SUM) |
| http_response | http.response | linked (bytes family SUM) |
| http_set_header | http.set_header | linked (bytes family SUM) |
| http_status | http.status | linked (bytes family SUM) |
| http_url_decode | http.url_decode | unreached (honest wall / native arm covers the surface) |
| http_with_headers | http.with_headers | linked (bytes family SUM) |
| int16_max_value | int16.max_value | linked (sized-convert) |
| int16_min_value | int16.min_value | linked (sized-convert) |
| int16_to_float32 | int16.to_float32 | unreached (honest wall / native arm covers the surface) |
| int16_to_float64 | int16.to_float64 | linked (sized-convert) |
| int16_to_int32 | int16.to_int32 | linked (sized-convert) |
| int16_to_int64 | int16.to_int64 | linked (sized-convert) |
| int16_to_int8 | int16.to_int8 | linked (sized-convert) |
| int16_to_int8_checked | int16.to_int8_checked | linked (sized-convert SUM) |
| int16_to_int8_saturating | int16.to_int8_saturating | linked (sized-convert) |
| int16_to_string | int16.to_string | linked (sized-convert) |
| int16_to_uint16 | int16.to_uint16 | linked (sized-convert) |
| int16_to_uint16_checked | int16.to_uint16_checked | linked (sized-convert SUM) |
| int16_to_uint16_saturating | int16.to_uint16_saturating | linked (sized-convert) |
| int16_to_uint32 | int16.to_uint32 | linked (sized-convert) |
| int16_to_uint32_checked | int16.to_uint32_checked | linked (sized-convert SUM) |
| int16_to_uint32_saturating | int16.to_uint32_saturating | linked (sized-convert) |
| int16_to_uint64 | int16.to_uint64 | linked (sized-convert) |
| int16_to_uint64_checked | int16.to_uint64_checked | linked (sized-convert SUM) |
| int16_to_uint64_saturating | int16.to_uint64_saturating | linked (sized-convert) |
| int16_to_uint8 | int16.to_uint8 | linked (sized-convert) |
| int16_to_uint8_checked | int16.to_uint8_checked | linked (sized-convert SUM) |
| int16_to_uint8_saturating | int16.to_uint8_saturating | linked (sized-convert) |
| int32_max_value | int32.max_value | linked (sized-convert) |
| int32_min_value | int32.min_value | linked (sized-convert) |
| int32_to_float32 | int32.to_float32 | unreached (honest wall / native arm covers the surface) |
| int32_to_float64 | int32.to_float64 | linked (sized-convert) |
| int32_to_int16 | int32.to_int16 | linked (sized-convert) |
| int32_to_int16_checked | int32.to_int16_checked | linked (sized-convert SUM) |
| int32_to_int16_saturating | int32.to_int16_saturating | linked (sized-convert) |
| int32_to_int64 | int32.to_int64 | linked (sized-convert) |
| int32_to_int8 | int32.to_int8 | linked (sized-convert) |
| int32_to_int8_checked | int32.to_int8_checked | linked (sized-convert SUM) |
| int32_to_int8_saturating | int32.to_int8_saturating | linked (sized-convert) |
| int32_to_string | int32.to_string | linked (sized-convert) |
| int32_to_uint16 | int32.to_uint16 | linked (sized-convert) |
| int32_to_uint16_checked | int32.to_uint16_checked | linked (sized-convert SUM) |
| int32_to_uint16_saturating | int32.to_uint16_saturating | linked (sized-convert) |
| int32_to_uint32 | int32.to_uint32 | linked (sized-convert) |
| int32_to_uint32_checked | int32.to_uint32_checked | linked (sized-convert SUM) |
| int32_to_uint32_saturating | int32.to_uint32_saturating | linked (sized-convert) |
| int32_to_uint64 | int32.to_uint64 | linked (sized-convert) |
| int32_to_uint64_checked | int32.to_uint64_checked | linked (sized-convert SUM) |
| int32_to_uint64_saturating | int32.to_uint64_saturating | linked (sized-convert) |
| int32_to_uint8 | int32.to_uint8 | linked (sized-convert) |
| int32_to_uint8_checked | int32.to_uint8_checked | linked (sized-convert SUM) |
| int32_to_uint8_saturating | int32.to_uint8_saturating | linked (sized-convert) |
| int64_max_value | int64.max_value | linked (sized-convert) |
| int64_min_value | int64.min_value | linked (sized-convert) |
| int64_to_float32 | int64.to_float32 | unreached (honest wall / native arm covers the surface) |
| int64_to_float64 | int64.to_float64 | linked (sized-convert) |
| int64_to_int16 | int64.to_int16 | linked (sized-convert) |
| int64_to_int16_checked | int64.to_int16_checked | linked (sized-convert SUM) |
| int64_to_int16_saturating | int64.to_int16_saturating | linked (sized-convert) |
| int64_to_int32 | int64.to_int32 | linked (sized-convert) |
| int64_to_int32_checked | int64.to_int32_checked | linked (sized-convert SUM) |
| int64_to_int32_saturating | int64.to_int32_saturating | linked (sized-convert) |
| int64_to_int8 | int64.to_int8 | linked (sized-convert) |
| int64_to_int8_checked | int64.to_int8_checked | linked (sized-convert SUM) |
| int64_to_int8_saturating | int64.to_int8_saturating | linked (sized-convert) |
| int64_to_string | int64.to_string | linked (sized-convert) |
| int64_to_uint16 | int64.to_uint16 | linked (sized-convert) |
| int64_to_uint16_checked | int64.to_uint16_checked | linked (sized-convert SUM) |
| int64_to_uint16_saturating | int64.to_uint16_saturating | linked (sized-convert) |
| int64_to_uint32 | int64.to_uint32 | linked (sized-convert) |
| int64_to_uint32_checked | int64.to_uint32_checked | linked (sized-convert SUM) |
| int64_to_uint32_saturating | int64.to_uint32_saturating | linked (sized-convert) |
| int64_to_uint64 | int64.to_uint64 | linked (sized-convert) |
| int64_to_uint64_checked | int64.to_uint64_checked | linked (sized-convert SUM) |
| int64_to_uint64_saturating | int64.to_uint64_saturating | linked (sized-convert) |
| int64_to_uint8 | int64.to_uint8 | linked (sized-convert) |
| int64_to_uint8_checked | int64.to_uint8_checked | linked (sized-convert SUM) |
| int64_to_uint8_saturating | int64.to_uint8_saturating | linked (sized-convert) |
| int8_max_value | int8.max_value | linked (sized-convert) |
| int8_min_value | int8.min_value | linked (sized-convert) |
| int8_to_float32 | int8.to_float32 | unreached (honest wall / native arm covers the surface) |
| int8_to_float64 | int8.to_float64 | linked (sized-convert) |
| int8_to_int16 | int8.to_int16 | linked (sized-convert) |
| int8_to_int32 | int8.to_int32 | linked (sized-convert) |
| int8_to_int64 | int8.to_int64 | linked (sized-convert) |
| int8_to_string | int8.to_string | linked (sized-convert) |
| int8_to_uint16 | int8.to_uint16 | linked (sized-convert) |
| int8_to_uint16_checked | int8.to_uint16_checked | linked (sized-convert SUM) |
| int8_to_uint16_saturating | int8.to_uint16_saturating | linked (sized-convert) |
| int8_to_uint32 | int8.to_uint32 | linked (sized-convert) |
| int8_to_uint32_checked | int8.to_uint32_checked | linked (sized-convert SUM) |
| int8_to_uint32_saturating | int8.to_uint32_saturating | linked (sized-convert) |
| int8_to_uint64 | int8.to_uint64 | linked (sized-convert) |
| int8_to_uint64_checked | int8.to_uint64_checked | linked (sized-convert SUM) |
| int8_to_uint64_saturating | int8.to_uint64_saturating | linked (sized-convert) |
| int8_to_uint8 | int8.to_uint8 | linked (sized-convert) |
| int8_to_uint8_checked | int8.to_uint8_checked | linked (sized-convert SUM) |
| int8_to_uint8_saturating | int8.to_uint8_saturating | linked (sized-convert) |
| int_abs | int.abs | unreached (honest wall / native arm covers the surface) |
| int_band | int.band | unreached (honest wall / native arm covers the surface) |
| int_bit_reverse | int.bit_reverse | unreached (honest wall / native arm covers the surface) |
| int_bit_width | int.bit_width | unreached (honest wall / native arm covers the surface) |
| int_bits_to_f32 | int.bits_to_f32 | unreached (honest wall / native arm covers the surface) |
| int_bits_to_float | int.bits_to_float | unreached (honest wall / native arm covers the surface) |
| int_bnot | int.bnot | unreached (honest wall / native arm covers the surface) |
| int_bor | int.bor | unreached (honest wall / native arm covers the surface) |
| int_bshl | int.bshl | unreached (honest wall / native arm covers the surface) |
| int_bshr | int.bshr | unreached (honest wall / native arm covers the surface) |
| int_bxor | int.bxor | unreached (honest wall / native arm covers the surface) |
| int_byte_swap | int.byte_swap | unreached (honest wall / native arm covers the surface) |
| int_clamp | int.clamp | unreached (honest wall / native arm covers the surface) |
| int_count_leading_zeros | int.count_leading_zeros | unreached (honest wall / native arm covers the surface) |
| int_count_trailing_zeros | int.count_trailing_zeros | unreached (honest wall / native arm covers the surface) |
| int_from_hex | int.from_hex | linked (calls.rs SUM tier) |
| int_from_int16 | int.from_int16 | linked (sized-convert) |
| int_from_int32 | int.from_int32 | linked (sized-convert) |
| int_from_int64 | int.from_int64 | linked (sized-convert) |
| int_from_int8 | int.from_int8 | linked (sized-convert) |
| int_from_uint16 | int.from_uint16 | linked (sized-convert) |
| int_from_uint32 | int.from_uint32 | linked (sized-convert) |
| int_from_uint64 | int.from_uint64 | linked (sized-convert) |
| int_from_uint64_checked | int.from_uint64_checked | linked (sized-convert SUM) |
| int_from_uint64_saturating | int.from_uint64_saturating | linked (sized-convert) |
| int_from_uint8 | int.from_uint8 | linked (sized-convert) |
| int_log2_ceil | int.log2_ceil | unreached (honest wall / native arm covers the surface) |
| int_log2_floor | int.log2_floor | unreached (honest wall / native arm covers the surface) |
| int_max | int.max | unreached (honest wall / native arm covers the surface) |
| int_max_value | int.max_value | linked (sized-convert) |
| int_min | int.min | unreached (honest wall / native arm covers the surface) |
| int_min_value | int.min_value | linked (sized-convert) |
| int_next_power_of_two | int.next_power_of_two | unreached (honest wall / native arm covers the surface) |
| int_pop_count | int.pop_count | unreached (honest wall / native arm covers the surface) |
| int_prev_power_of_two | int.prev_power_of_two | unreached (honest wall / native arm covers the surface) |
| int_rotate_left | int.rotate_left | linked (scalar/text) |
| int_rotate_right | int.rotate_right | linked (scalar/text) |
| int_to_float | int.to_float | unreached (honest wall / native arm covers the surface) |
| int_to_float32 | int.to_float32 | unreached (honest wall / native arm covers the surface) |
| int_to_float32_checked | int.to_float32_checked | unreached (honest wall / native arm covers the surface) |
| int_to_float64 | int.to_float64 | unreached (honest wall / native arm covers the surface) |
| int_to_hex | int.to_hex | linked (scalar/text) |
| int_to_int16 | int.to_int16 | linked (sized-convert) |
| int_to_int16_checked | int.to_int16_checked | linked (sized-convert SUM) |
| int_to_int16_saturating | int.to_int16_saturating | linked (sized-convert) |
| int_to_int32 | int.to_int32 | linked (sized-convert) |
| int_to_int32_checked | int.to_int32_checked | linked (sized-convert SUM) |
| int_to_int32_saturating | int.to_int32_saturating | linked (sized-convert) |
| int_to_int64 | int.to_int64 | linked (sized-convert) |
| int_to_int8 | int.to_int8 | linked (sized-convert) |
| int_to_int8_checked | int.to_int8_checked | linked (sized-convert SUM) |
| int_to_int8_saturating | int.to_int8_saturating | linked (sized-convert) |
| int_to_string | int.to_string | linked (calls.rs VERIFIED) |
| int_to_u32 | int.to_u32 | unreached (honest wall / native arm covers the surface) |
| int_to_u8 | int.to_u8 | unreached (honest wall / native arm covers the surface) |
| int_to_uint16 | int.to_uint16 | linked (sized-convert) |
| int_to_uint16_checked | int.to_uint16_checked | linked (sized-convert SUM) |
| int_to_uint16_saturating | int.to_uint16_saturating | linked (sized-convert) |
| int_to_uint32 | int.to_uint32 | linked (sized-convert) |
| int_to_uint32_checked | int.to_uint32_checked | linked (sized-convert SUM) |
| int_to_uint32_saturating | int.to_uint32_saturating | linked (sized-convert) |
| int_to_uint64 | int.to_uint64 | linked (sized-convert) |
| int_to_uint64_checked | int.to_uint64_checked | linked (sized-convert SUM) |
| int_to_uint64_saturating | int.to_uint64_saturating | linked (sized-convert) |
| int_to_uint8 | int.to_uint8 | linked (sized-convert) |
| int_to_uint8_checked | int.to_uint8_checked | linked (sized-convert SUM) |
| int_to_uint8_saturating | int.to_uint8_saturating | linked (sized-convert) |
| int_wrap_add | int.wrap_add | unreached (honest wall / native arm covers the surface) |
| int_wrap_mul | int.wrap_mul | unreached (honest wall / native arm covers the surface) |
| io_print | io.print | unreached (honest wall / native arm covers the surface) |
| io_read_all | io.read_all | unreached (honest wall / native arm covers the surface) |
| io_read_byte | io.read_byte | unreached (honest wall / native arm covers the surface) |
| io_read_line | io.read_line | unreached (honest wall / native arm covers the surface) |
| io_read_n_bytes | io.read_n_bytes | unreached (honest wall / native arm covers the surface) |
| io_write | io.write | unreached (honest wall / native arm covers the surface) |
| io_write_bytes | io.write_bytes | unreached (honest wall / native arm covers the surface) |
| json_get_array | json.get_array | linked (bytes family SUM) |
| json_get_bool | json.get_bool | linked (bytes family SUM) |
| json_get_float | json.get_float | linked (bytes family SUM) |
| json_get_int | json.get_int | linked (bytes family SUM) |
| json_get_string | json.get_string | linked (bytes family SUM) |
| json_parse | json.parse | linked (calls.rs SUM tier) |
| json_path_field | json.field | unreached (honest wall / native arm covers the surface) |
| json_path_get | json.get_path | unreached (honest wall / native arm covers the surface) |
| json_path_index | json.index | unreached (honest wall / native arm covers the surface) |
| json_path_remove | json.remove_path | REJECTED: incumbent inline-pairs Value layout — native helper $jp_remove instead |
| json_path_root | json.root | unreached (honest wall / native arm covers the surface) |
| json_path_set | json.set_path | REJECTED: incumbent inline-pairs Value layout (tag@h+4, count@h+8) — native helper $jp_set instead |
| json_stringify | json.stringify | unreached (honest wall / native arm covers the surface) |
| json_stringify_pretty | json.stringify_pretty | REJECTED: incumbent len-as-tag Value layout — native helper $vjson_pretty instead |
| list_all | list.all | unreached (honest wall / native arm covers the surface) |
| list_all_str | list.all_str | unreached (honest wall / native arm covers the surface) |
| list_any | list.any | unreached (honest wall / native arm covers the surface) |
| list_any_str | list.any_str | unreached (honest wall / native arm covers the surface) |
| list_binary_search | list.binary_search | unreached (honest wall / native arm covers the surface) |
| list_chunk | list.chunk | unreached (honest wall / native arm covers the surface) |
| list_chunk_str | list.chunk_str | unreached (honest wall / native arm covers the surface) |
| list_contains | list.contains | unreached (honest wall / native arm covers the surface) |
| list_contains_float | list.contains_float | unreached (honest wall / native arm covers the surface) |
| list_contains_hshare | list.contains_hshare | unreached (honest wall / native arm covers the surface) |
| list_contains_str | list.contains_str | unreached (honest wall / native arm covers the surface) |
| list_count | list.count | unreached (honest wall / native arm covers the surface) |
| list_count_str | list.count_str | unreached (honest wall / native arm covers the surface) |
| list_dedup | list.dedup | unreached (honest wall / native arm covers the surface) |
| list_dedup_float | list.dedup_float | unreached (honest wall / native arm covers the surface) |
| list_dedup_hshare | list.dedup_hshare | unreached (honest wall / native arm covers the surface) |
| list_dedup_str | list.dedup_str | unreached (honest wall / native arm covers the surface) |
| list_drop | list.drop | unreached (honest wall / native arm covers the surface) |
| list_drop_end | list.drop_end | unreached (honest wall / native arm covers the surface) |
| list_drop_end_heapelem | list.drop_end_heapelem | unreached (honest wall / native arm covers the surface) |
| list_drop_end_str | list.drop_end_str | unreached (honest wall / native arm covers the surface) |
| list_drop_end_value | list.drop_end_value | unreached (honest wall / native arm covers the surface) |
| list_drop_hshare | list.drop_hshare | unreached (honest wall / native arm covers the surface) |
| list_drop_liststr | list.drop_liststr | unreached (honest wall / native arm covers the surface) |
| list_drop_str | list.drop_str | unreached (honest wall / native arm covers the surface) |
| list_drop_while | list.drop_while | unreached (honest wall / native arm covers the surface) |
| list_drop_while_str | list.drop_while_str | unreached (honest wall / native arm covers the surface) |
| list_enumerate | list.enumerate | unreached (honest wall / native arm covers the surface) |
| list_enumerate_h | list.enumerate_h | unreached (honest wall / native arm covers the surface) |
| list_enumerate_str | list.enumerate_str | unreached (honest wall / native arm covers the surface) |
| list_eq_bool | list.eq_bool | unreached (honest wall / native arm covers the surface) |
| list_eq_float | list.eq_float | unreached (honest wall / native arm covers the surface) |
| list_eq_int | list.eq_int | unreached (honest wall / native arm covers the surface) |
| list_eq_list_float | list.eq_list_float | unreached (honest wall / native arm covers the surface) |
| list_eq_list_int | list.eq_list_int | unreached (honest wall / native arm covers the surface) |
| list_eq_list_str | list.eq_list_str | unreached (honest wall / native arm covers the surface) |
| list_eq_opt_int | list.eq_opt_int | unreached (honest wall / native arm covers the surface) |
| list_eq_opt_str | list.eq_opt_str | unreached (honest wall / native arm covers the surface) |
| list_eq_res_int | list.eq_res_int | unreached (honest wall / native arm covers the surface) |
| list_eq_str | list.eq_str | unreached (honest wall / native arm covers the surface) |
| list_eq_value | list.eq_value | unreached (honest wall / native arm covers the surface) |
| list_filter | list.filter | unreached (honest wall / native arm covers the surface) |
| list_filter_map | list.filter_map | unreached (honest wall / native arm covers the surface) |
| list_filter_rc | list.filter_rc | unreached (honest wall / native arm covers the surface) |
| list_filter_str | list.filter_str | unreached (honest wall / native arm covers the surface) |
| list_find | list.find | unreached (honest wall / native arm covers the surface) |
| list_find_index | list.find_index | unreached (honest wall / native arm covers the surface) |
| list_find_int_str | list.find_int_str | unreached (honest wall / native arm covers the surface) |
| list_find_str | list.find_str | unreached (honest wall / native arm covers the surface) |
| list_first | list.first | unreached (honest wall / native arm covers the surface) |
| list_first_hshare | list.first_hshare | unreached (honest wall / native arm covers the surface) |
| list_first_liststr | list.first_liststr | unreached (honest wall / native arm covers the surface) |
| list_first_str | list.first_str | unreached (honest wall / native arm covers the surface) |
| list_first_value | list.first_value | unreached (honest wall / native arm covers the surface) |
| list_flat_map | list.flat_map | unreached (honest wall / native arm covers the surface) |
| list_flat_map_h | list.flat_map_h | unreached (honest wall / native arm covers the surface) |
| list_flat_map_str | list.flat_map_str | unreached (honest wall / native arm covers the surface) |
| list_flatten | list.flatten | unreached (honest wall / native arm covers the surface) |
| list_flatten_rc | list.flatten_rc | unreached (honest wall / native arm covers the surface) |
| list_fold | list.fold | unreached (honest wall / native arm covers the surface) |
| list_fold_hrec | list.fold_hrec | unreached (honest wall / native arm covers the surface) |
| list_fold_hsca | list.fold_hsca | unreached (honest wall / native arm covers the surface) |
| list_fold_ols | list.fold_ols | unreached (honest wall / native arm covers the surface) |
| list_fold_str | list.fold_str | unreached (honest wall / native arm covers the surface) |
| list_fold_str_hacc | list.fold_str_hacc | unreached (honest wall / native arm covers the surface) |
| list_fold_str_msi | list.fold_str_msi | unreached (honest wall / native arm covers the surface) |
| list_get | list.get | unreached (honest wall / native arm covers the surface) |
| list_get_hshare | list.get_hshare | unreached (honest wall / native arm covers the surface) |
| list_get_liststr | list.get_liststr | unreached (honest wall / native arm covers the surface) |
| list_get_or | list.get_or | unreached (honest wall / native arm covers the surface) |
| list_get_or_hshare | list.get_or_hshare | unreached (honest wall / native arm covers the surface) |
| list_get_or_str | list.get_or_str | unreached (honest wall / native arm covers the surface) |
| list_get_or_value | list.get_or_value | unreached (honest wall / native arm covers the surface) |
| list_get_str | list.get_str | unreached (honest wall / native arm covers the surface) |
| list_get_value | list.get_value | unreached (honest wall / native arm covers the surface) |
| list_group_by | list.group_by | unreached (honest wall / native arm covers the surface) |
| list_index_of | list.index_of | unreached (honest wall / native arm covers the surface) |
| list_index_of_float | list.index_of_float | unreached (honest wall / native arm covers the surface) |
| list_index_of_hshare | list.index_of_hshare | unreached (honest wall / native arm covers the surface) |
| list_index_of_str | list.index_of_str | unreached (honest wall / native arm covers the surface) |
| list_insert | list.insert | unreached (honest wall / native arm covers the surface) |
| list_insert_heapelem | list.insert_heapelem | unreached (honest wall / native arm covers the surface) |
| list_insert_str | list.insert_str | unreached (honest wall / native arm covers the surface) |
| list_insert_value | list.insert_value | unreached (honest wall / native arm covers the surface) |
| list_intersperse | list.intersperse | unreached (honest wall / native arm covers the surface) |
| list_intersperse_str | list.intersperse_str | unreached (honest wall / native arm covers the surface) |
| list_is_empty | list.is_empty | unreached (honest wall / native arm covers the surface) |
| list_join | list.join | unreached (honest wall / native arm covers the surface) |
| list_last | list.last | unreached (honest wall / native arm covers the surface) |
| list_last_hshare | list.last_hshare | unreached (honest wall / native arm covers the surface) |
| list_last_liststr | list.last_liststr | unreached (honest wall / native arm covers the surface) |
| list_last_str | list.last_str | unreached (honest wall / native arm covers the surface) |
| list_last_value | list.last_value | unreached (honest wall / native arm covers the surface) |
| list_len | list.len | unreached (honest wall / native arm covers the surface) |
| list_length | list.length | unreached (honest wall / native arm covers the surface) |
| list_map | list.map | unreached (honest wall / native arm covers the surface) |
| list_map_s2h | list.map_s2h | unreached (honest wall / native arm covers the surface) |
| list_map_str | list.map_str | unreached (honest wall / native arm covers the surface) |
| list_max | list.max | unreached (honest wall / native arm covers the surface) |
| list_max_float | list.max_float | unreached (honest wall / native arm covers the surface) |
| list_max_lint | list.max_lint | unreached (honest wall / native arm covers the surface) |
| list_max_lstr | list.max_lstr | unreached (honest wall / native arm covers the surface) |
| list_max_oint | list.max_oint | unreached (honest wall / native arm covers the surface) |
| list_max_str | list.max_str | unreached (honest wall / native arm covers the surface) |
| list_max_tss | list.max_tss | unreached (honest wall / native arm covers the surface) |
| list_max_tsstr | list.max_tsstr | unreached (honest wall / native arm covers the surface) |
| list_min | list.min | unreached (honest wall / native arm covers the surface) |
| list_min_float | list.min_float | unreached (honest wall / native arm covers the surface) |
| list_min_lint | list.min_lint | unreached (honest wall / native arm covers the surface) |
| list_min_lstr | list.min_lstr | unreached (honest wall / native arm covers the surface) |
| list_min_oint | list.min_oint | unreached (honest wall / native arm covers the surface) |
| list_min_str | list.min_str | unreached (honest wall / native arm covers the surface) |
| list_min_tss | list.min_tss | unreached (honest wall / native arm covers the surface) |
| list_min_tsstr | list.min_tsstr | unreached (honest wall / native arm covers the surface) |
| list_partition | list.partition | unreached (honest wall / native arm covers the surface) |
| list_partition_rc | list.partition_rc | unreached (honest wall / native arm covers the surface) |
| list_pop | list.pop | unreached (honest wall / native arm covers the surface) |
| list_product | list.product | unreached (honest wall / native arm covers the surface) |
| list_range | list.range | linked (calls.rs VERIFIED) |
| list_reduce | list.reduce | unreached (honest wall / native arm covers the surface) |
| list_reduce_str | list.reduce_str | unreached (honest wall / native arm covers the surface) |
| list_remove_at | list.remove_at | unreached (honest wall / native arm covers the surface) |
| list_remove_at_heapelem | list.remove_at_heapelem | unreached (honest wall / native arm covers the surface) |
| list_remove_at_str | list.remove_at_str | unreached (honest wall / native arm covers the surface) |
| list_remove_at_value | list.remove_at_value | unreached (honest wall / native arm covers the surface) |
| list_repeat | list.repeat | linked (calls.rs VERIFIED) |
| list_repeat_rc | list.repeat_rc | unreached (honest wall / native arm covers the surface) |
| list_reverse | list.reverse | unreached (honest wall / native arm covers the surface) |
| list_reverse_str | list.reverse_str | unreached (honest wall / native arm covers the surface) |
| list_scan | list.scan | unreached (honest wall / native arm covers the surface) |
| list_scan_str | list.scan_str | unreached (honest wall / native arm covers the surface) |
| list_set | list.set | unreached (honest wall / native arm covers the surface) |
| list_set_heapelem | list.set_heapelem | unreached (honest wall / native arm covers the surface) |
| list_set_str | list.set_str | unreached (honest wall / native arm covers the surface) |
| list_set_value | list.set_value | unreached (honest wall / native arm covers the surface) |
| list_slice | list.slice | unreached (honest wall / native arm covers the surface) |
| list_slice_hshare | list.slice_hshare | unreached (honest wall / native arm covers the surface) |
| list_slice_str | list.slice_str | unreached (honest wall / native arm covers the surface) |
| list_sort | list.sort | unreached (honest wall / native arm covers the surface) |
| list_sort_by | list.sort_by | unreached (honest wall / native arm covers the surface) |
| list_sort_by_float | list.sort_by_float | unreached (honest wall / native arm covers the surface) |
| list_sort_by_float_rc | list.sort_by_float_rc | unreached (honest wall / native arm covers the surface) |
| list_sort_by_keys | list.sort_by_keys | unreached (honest wall / native arm covers the surface) |
| list_sort_by_rc | list.sort_by_rc | unreached (honest wall / native arm covers the surface) |
| list_sort_by_str_key | list.sort_by_str_key | unreached (honest wall / native arm covers the surface) |
| list_sort_by_str_key_rc | list.sort_by_str_key_rc | unreached (honest wall / native arm covers the surface) |
| list_sort_float | list.sort_float | unreached (honest wall / native arm covers the surface) |
| list_sort_lint | list.sort_lint | unreached (honest wall / native arm covers the surface) |
| list_sort_oint | list.sort_oint | unreached (honest wall / native arm covers the surface) |
| list_sort_str | list.sort_str | unreached (honest wall / native arm covers the surface) |
| list_sort_tss | list.sort_tss | unreached (honest wall / native arm covers the surface) |
| list_sort_tsstr | list.sort_tsstr | unreached (honest wall / native arm covers the surface) |
| list_sum | list.sum | unreached (honest wall / native arm covers the surface) |
| list_swap | list.swap | unreached (honest wall / native arm covers the surface) |
| list_swap_heapelem | list.swap_heapelem | unreached (honest wall / native arm covers the surface) |
| list_swap_str | list.swap_str | unreached (honest wall / native arm covers the surface) |
| list_swap_value | list.swap_value | unreached (honest wall / native arm covers the surface) |
| list_tail | list.tail | unreached (honest wall / native arm covers the surface) |
| list_tail_heapelem | list.tail_heapelem | unreached (honest wall / native arm covers the surface) |
| list_tail_str | list.tail_str | unreached (honest wall / native arm covers the surface) |
| list_tail_value | list.tail_value | unreached (honest wall / native arm covers the surface) |
| list_take | list.take | unreached (honest wall / native arm covers the surface) |
| list_take_end | list.take_end | unreached (honest wall / native arm covers the surface) |
| list_take_end_heapelem | list.take_end_heapelem | unreached (honest wall / native arm covers the surface) |
| list_take_end_str | list.take_end_str | unreached (honest wall / native arm covers the surface) |
| list_take_end_value | list.take_end_value | unreached (honest wall / native arm covers the surface) |
| list_take_hshare | list.take_hshare | unreached (honest wall / native arm covers the surface) |
| list_take_liststr | list.take_liststr | unreached (honest wall / native arm covers the surface) |
| list_take_str | list.take_str | unreached (honest wall / native arm covers the surface) |
| list_take_while | list.take_while | unreached (honest wall / native arm covers the surface) |
| list_take_while_str | list.take_while_str | unreached (honest wall / native arm covers the surface) |
| list_to_string | list.to_string | unreached (honest wall / native arm covers the surface) |
| list_to_string_b | list.to_string_b | unreached (honest wall / native arm covers the surface) |
| list_to_string_f | list.to_string_f | unreached (honest wall / native arm covers the surface) |
| list_to_string_ll | list.to_string_ll | unreached (honest wall / native arm covers the surface) |
| list_to_string_llf | list.to_string_llf | unreached (honest wall / native arm covers the surface) |
| list_to_string_lmh | list.to_string_lmh | unreached (honest wall / native arm covers the surface) |
| list_to_string_lmlo | list.to_string_lmlo | unreached (honest wall / native arm covers the surface) |
| list_to_string_lo | list.to_string_lo | unreached (honest wall / native arm covers the surface) |
| list_to_string_lob | list.to_string_lob | unreached (honest wall / native arm covers the surface) |
| list_to_string_lr | list.to_string_lr | unreached (honest wall / native arm covers the surface) |
| list_to_string_lsi | list.to_string_lsi | unreached (honest wall / native arm covers the surface) |
| list_to_string_s | list.to_string_s | unreached (honest wall / native arm covers the surface) |
| list_unique | list.unique | unreached (honest wall / native arm covers the surface) |
| list_unique_by | list.unique_by | unreached (honest wall / native arm covers the surface) |
| list_unique_by_sk | list.unique_by_sk | unreached (honest wall / native arm covers the surface) |
| list_unique_float | list.unique_float | unreached (honest wall / native arm covers the surface) |
| list_unique_hshare | list.unique_hshare | unreached (honest wall / native arm covers the surface) |
| list_unique_str | list.unique_str | unreached (honest wall / native arm covers the surface) |
| list_update | list.update | unreached (honest wall / native arm covers the surface) |
| list_update_heapelem | list.update_heapelem | unreached (honest wall / native arm covers the surface) |
| list_update_str | list.update_str | unreached (honest wall / native arm covers the surface) |
| list_update_value | list.update_value | unreached (honest wall / native arm covers the surface) |
| list_window | list.window | unreached (honest wall / native arm covers the surface) |
| list_window_str | list.window_str | unreached (honest wall / native arm covers the surface) |
| list_windows | list.windows | unreached (honest wall / native arm covers the surface) |
| list_windows_str | list.windows_str | unreached (honest wall / native arm covers the surface) |
| list_with_capacity | list.with_capacity | unreached (honest wall / native arm covers the surface) |
| list_zip | list.zip | unreached (honest wall / native arm covers the surface) |
| list_zip_h | list.zip_h | unreached (honest wall / native arm covers the surface) |
| list_zip_hs | list.zip_hs | unreached (honest wall / native arm covers the surface) |
| list_zip_rc | list.zip_rc | unreached (honest wall / native arm covers the surface) |
| list_zip_sh | list.zip_sh | unreached (honest wall / native arm covers the surface) |
| list_zip_with | list.zip_with | unreached (honest wall / native arm covers the surface) |
| list_zip_with_str | list.zip_with_str | unreached (honest wall / native arm covers the surface) |
| map_all | map.all | unreached (honest wall / native arm covers the surface) |
| map_all_skv | map.all_skv | unreached (honest wall / native arm covers the surface) |
| map_all_str | map.all_str | unreached (honest wall / native arm covers the surface) |
| map_any | map.any | unreached (honest wall / native arm covers the surface) |
| map_any_skv | map.any_skv | unreached (honest wall / native arm covers the surface) |
| map_any_str | map.any_str | unreached (honest wall / native arm covers the surface) |
| map_contains | map.contains | unreached (honest wall / native arm covers the surface) |
| map_contains_hval | map.contains_hval | unreached (honest wall / native arm covers the surface) |
| map_contains_skv | map.contains_skv | unreached (honest wall / native arm covers the surface) |
| map_contains_srec | map.contains_srec | unreached (honest wall / native arm covers the surface) |
| map_contains_str | map.contains_str | unreached (honest wall / native arm covers the surface) |
| map_count | map.count | unreached (honest wall / native arm covers the surface) |
| map_count_skv | map.count_skv | unreached (honest wall / native arm covers the surface) |
| map_count_str | map.count_str | unreached (honest wall / native arm covers the surface) |
| map_entries_core | map.entries | unreached (honest wall / native arm covers the surface) |
| map_entries_hvalt | map.entries_hvalt | unreached (honest wall / native arm covers the surface) |
| map_entries_skv | map.entries_skv | unreached (honest wall / native arm covers the surface) |
| map_entries_str | map.entries_str | unreached (honest wall / native arm covers the surface) |
| map_eq_hval | map.eq_hval | unreached (honest wall / native arm covers the surface) |
| map_eq_ivh | map.eq_ivh | unreached (honest wall / native arm covers the surface) |
| map_eq_skv | map.eq_skv | unreached (honest wall / native arm covers the surface) |
| map_filter | map.filter | unreached (honest wall / native arm covers the surface) |
| map_filter_skv | map.filter_skv | unreached (honest wall / native arm covers the surface) |
| map_filter_str | map.filter_str | unreached (honest wall / native arm covers the surface) |
| map_find | map.find | unreached (honest wall / native arm covers the surface) |
| map_find_skv | map.find_skv | unreached (honest wall / native arm covers the surface) |
| map_fold | map.fold | unreached (honest wall / native arm covers the surface) |
| map_fold_skv | map.fold_skv | unreached (honest wall / native arm covers the surface) |
| map_fold_skv_hacc | map.fold_skv_hacc | unreached (honest wall / native arm covers the surface) |
| map_fold_skv_msi | map.fold_skv_msi | unreached (honest wall / native arm covers the surface) |
| map_fold_str | map.fold_str | unreached (honest wall / native arm covers the surface) |
| map_fold_str_msi | map.fold_str_msi | unreached (honest wall / native arm covers the surface) |
| map_fold_str_sacc | map.fold_str_sacc | unreached (honest wall / native arm covers the surface) |
| map_from_list | map.from_list | unreached (honest wall / native arm covers the surface) |
| map_from_list_hobj | map.from_list_hobj | unreached (honest wall / native arm covers the surface) |
| map_from_list_hval | map.from_list_hval | unreached (honest wall / native arm covers the surface) |
| map_from_list_if | map.from_list_if | unreached (honest wall / native arm covers the surface) |
| map_from_list_ivh | map.from_list_ivh | unreached (honest wall / native arm covers the surface) |
| map_from_list_mlo | map.from_list_mlo | unreached (honest wall / native arm covers the surface) |
| map_from_list_msv | map.from_list_msv | unreached (honest wall / native arm covers the surface) |
| map_from_list_skv | map.from_list_skv | unreached (honest wall / native arm covers the surface) |
| map_from_list_srec | map.from_list_srec | unreached (honest wall / native arm covers the surface) |
| map_from_list_str | map.from_list_str | unreached (honest wall / native arm covers the surface) |
| map_from_list_vtag | map.from_list_vtag | unreached (honest wall / native arm covers the surface) |
| map_get | map.get | unreached (honest wall / native arm covers the surface) |
| map_get_hval | map.get_hval | unreached (honest wall / native arm covers the surface) |
| map_get_ivh | map.get_ivh | unreached (honest wall / native arm covers the surface) |
| map_get_or | map.get_or | unreached (honest wall / native arm covers the surface) |
| map_get_or_hval | map.get_or_hval | unreached (honest wall / native arm covers the surface) |
| map_get_or_msv | map.get_or_msv | unreached (honest wall / native arm covers the surface) |
| map_get_or_skv | map.get_or_skv | unreached (honest wall / native arm covers the surface) |
| map_get_or_str | map.get_or_str | unreached (honest wall / native arm covers the surface) |
| map_get_skv | map.get_skv | unreached (honest wall / native arm covers the surface) |
| map_get_srec | map.get_srec | unreached (honest wall / native arm covers the surface) |
| map_get_str | map.get_str | unreached (honest wall / native arm covers the surface) |
| map_get_vtag | map.get_vtag | unreached (honest wall / native arm covers the surface) |
| map_is_empty | map.is_empty | unreached (honest wall / native arm covers the surface) |
| map_is_empty_skv | map.is_empty_skv | unreached (honest wall / native arm covers the surface) |
| map_is_empty_str | map.is_empty_str | unreached (honest wall / native arm covers the surface) |
| map_keys | map.keys | unreached (honest wall / native arm covers the surface) |
| map_keys_hval | map.keys_hval | unreached (honest wall / native arm covers the surface) |
| map_keys_skv | map.keys_skv | unreached (honest wall / native arm covers the surface) |
| map_keys_str | map.keys_str | unreached (honest wall / native arm covers the surface) |
| map_len | map.len | unreached (honest wall / native arm covers the surface) |
| map_len_hval | map.len_hval | unreached (honest wall / native arm covers the surface) |
| map_len_ivh | map.len_ivh | unreached (honest wall / native arm covers the surface) |
| map_len_skv | map.len_skv | unreached (honest wall / native arm covers the surface) |
| map_len_str | map.len_str | unreached (honest wall / native arm covers the surface) |
| map_map | map.map | unreached (honest wall / native arm covers the surface) |
| map_map_ivh2core | map.map_ivh2core | unreached (honest wall / native arm covers the surface) |
| map_map_skv | map.map_skv | unreached (honest wall / native arm covers the surface) |
| map_map_skv2hvalt | map.map_skv2hvalt | unreached (honest wall / native arm covers the surface) |
| map_map_skv2str | map.map_skv2str | unreached (honest wall / native arm covers the surface) |
| map_map_str2skv | map.map_str2skv | unreached (honest wall / native arm covers the surface) |
| map_merge | map.merge | unreached (honest wall / native arm covers the surface) |
| map_merge_skv | map.merge_skv | unreached (honest wall / native arm covers the surface) |
| map_merge_str | map.merge_str | unreached (honest wall / native arm covers the surface) |
| map_new | map.new | unreached (honest wall / native arm covers the surface) |
| map_new_hval | map.new_hval | unreached (honest wall / native arm covers the surface) |
| map_new_ivh | map.new_ivh | unreached (honest wall / native arm covers the surface) |
| map_new_mlo | map.new_mlo | unreached (honest wall / native arm covers the surface) |
| map_new_msv | map.new_msv | unreached (honest wall / native arm covers the surface) |
| map_new_skv | map.new_skv | unreached (honest wall / native arm covers the surface) |
| map_new_str | map.new_str | unreached (honest wall / native arm covers the surface) |
| map_remove | map.remove | unreached (honest wall / native arm covers the surface) |
| map_remove_msv | map.remove_msv | unreached (honest wall / native arm covers the surface) |
| map_remove_skv | map.remove_skv | unreached (honest wall / native arm covers the surface) |
| map_remove_str | map.remove_str | unreached (honest wall / native arm covers the surface) |
| map_set | map.set | unreached (honest wall / native arm covers the surface) |
| map_set_hval | map.set_hval | unreached (honest wall / native arm covers the surface) |
| map_set_ivh | map.set_ivh | unreached (honest wall / native arm covers the surface) |
| map_set_mlo | map.set_mlo | unreached (honest wall / native arm covers the surface) |
| map_set_msv | map.set_msv | unreached (honest wall / native arm covers the surface) |
| map_set_skv | map.set_skv | unreached (honest wall / native arm covers the surface) |
| map_set_srec | map.set_srec | unreached (honest wall / native arm covers the surface) |
| map_set_str | map.set_str | unreached (honest wall / native arm covers the surface) |
| map_to_string | map.to_string | unreached (honest wall / native arm covers the surface) |
| map_to_string_hval | map.to_string_hval | unreached (honest wall / native arm covers the surface) |
| map_to_string_if | map.to_string_if | unreached (honest wall / native arm covers the surface) |
| map_to_string_ivh | map.to_string_ivh | unreached (honest wall / native arm covers the surface) |
| map_to_string_mlo | map.to_string_mlo | unreached (honest wall / native arm covers the surface) |
| map_to_string_sb | map.to_string_sb | unreached (honest wall / native arm covers the surface) |
| map_to_string_sf | map.to_string_sf | unreached (honest wall / native arm covers the surface) |
| map_to_string_ss | map.to_string_ss | unreached (honest wall / native arm covers the surface) |
| map_update | map.update | unreached (honest wall / native arm covers the surface) |
| map_update_skv | map.update_skv | unreached (honest wall / native arm covers the surface) |
| map_update_str | map.update_str | unreached (honest wall / native arm covers the surface) |
| map_upsert | map.upsert | unreached (honest wall / native arm covers the surface) |
| map_upsert_skv | map.upsert_skv | unreached (honest wall / native arm covers the surface) |
| map_values | map.values | unreached (honest wall / native arm covers the surface) |
| map_values_skv | map.values_skv | unreached (honest wall / native arm covers the surface) |
| map_values_str | map.values_str | unreached (honest wall / native arm covers the surface) |
| math_abs | math.abs | linked (libm) |
| math_atan | math.atan | linked (libm) |
| math_choose | math.choose | linked (libm) |
| math_cos | math.cos | linked (libm) |
| math_e | math.e | linked (libm) |
| math_exp | math.exp | linked (libm) |
| math_factorial | math.factorial | linked (libm) |
| math_fmax | math.fmax | linked (libm) |
| math_fmin | math.fmin | linked (libm) |
| math_fpow | math.fpow | linked (libm) |
| math_log | math.log | linked (libm) |
| math_log10 | math.log10 | linked (libm) |
| math_log2 | math.log2 | linked (libm) |
| math_log_gamma | math.log_gamma | linked (libm) |
| math_max | math.max | linked (libm) |
| math_min | math.min | linked (libm) |
| math_pi | math.pi | linked (libm) |
| math_pow | math.pow | linked (libm) |
| math_sign | math.sign | linked (libm) |
| math_sin | math.sin | linked (libm) |
| math_sqrt | math.sqrt | linked (libm) |
| math_tan | math.tan | linked (libm) |
| math_tanh | math.tanh | linked (libm) |
| matrix_add | matrix.add | unreached (honest wall / native arm covers the surface) |
| matrix_attention_weights | matrix.attention_weights | unreached (honest wall / native arm covers the surface) |
| matrix_broadcast_add_row | matrix.broadcast_add_row | unreached (honest wall / native arm covers the surface) |
| matrix_causal_mask_add | matrix.causal_mask_add | unreached (honest wall / native arm covers the surface) |
| matrix_cols | matrix.cols | unreached (honest wall / native arm covers the surface) |
| matrix_concat_cols | matrix.concat_cols | unreached (honest wall / native arm covers the surface) |
| matrix_concat_cols_many | matrix.concat_cols_many | unreached (honest wall / native arm covers the surface) |
| matrix_conv1d | matrix.conv1d | unreached (honest wall / native arm covers the surface) |
| matrix_div | matrix.div | unreached (honest wall / native arm covers the surface) |
| matrix_dot_row | matrix.dot_row | unreached (honest wall / native arm covers the surface) |
| matrix_from_bytes_f16_le | matrix.from_bytes_f16_le | unreached (honest wall / native arm covers the surface) |
| matrix_from_bytes_f32_le | matrix.from_bytes_f32_le | unreached (honest wall / native arm covers the surface) |
| matrix_from_bytes_f64_le | matrix.from_bytes_f64_le | unreached (honest wall / native arm covers the surface) |
| matrix_from_lists | matrix.from_lists | unreached (honest wall / native arm covers the surface) |
| matrix_from_q1_0_bytes | matrix.from_q1_0_bytes | unreached (honest wall / native arm covers the surface) |
| matrix_gather_rows | matrix.gather_rows | unreached (honest wall / native arm covers the surface) |
| matrix_gelu | matrix.gelu | unreached (honest wall / native arm covers the surface) |
| matrix_get | matrix.get | unreached (honest wall / native arm covers the surface) |
| matrix_layer_norm_rows | matrix.layer_norm_rows | unreached (honest wall / native arm covers the surface) |
| matrix_linear_row | matrix.linear_row | unreached (honest wall / native arm covers the surface) |
| matrix_linear_row_no_bias | matrix.linear_row_no_bias | unreached (honest wall / native arm covers the surface) |
| matrix_map | matrix.map | unreached (honest wall / native arm covers the surface) |
| matrix_masked_multi_head_attention | matrix.masked_multi_head_attention | unreached (honest wall / native arm covers the surface) |
| matrix_mul | matrix.mul | unreached (honest wall / native arm covers the surface) |
| matrix_mul_f32 | matrix.mul_f32 | unreached (honest wall / native arm covers the surface) |
| matrix_mul_f32_scaled | matrix.mul_f32_scaled | unreached (honest wall / native arm covers the surface) |
| matrix_mul_f32_t | matrix.mul_f32_t | unreached (honest wall / native arm covers the surface) |
| matrix_mul_f32_t_scaled | matrix.mul_f32_t_scaled | unreached (honest wall / native arm covers the surface) |
| matrix_mul_scaled | matrix.mul_scaled | unreached (honest wall / native arm covers the surface) |
| matrix_multi_head_attention | matrix.multi_head_attention | unreached (honest wall / native arm covers the surface) |
| matrix_neg | matrix.neg | unreached (honest wall / native arm covers the surface) |
| matrix_ones | matrix.ones | unreached (honest wall / native arm covers the surface) |
| matrix_ones_f32 | matrix.ones_f32 | unreached (honest wall / native arm covers the surface) |
| matrix_pow | matrix.pow | unreached (honest wall / native arm covers the surface) |
| matrix_rms_norm_rows | matrix.rms_norm_rows | unreached (honest wall / native arm covers the surface) |
| matrix_rope_rotate | matrix.rope_rotate | unreached (honest wall / native arm covers the surface) |
| matrix_rope_rotate_at | matrix.rope_rotate_at | unreached (honest wall / native arm covers the surface) |
| matrix_rope_rotate_neox_at | matrix.rope_rotate_neox_at | unreached (honest wall / native arm covers the surface) |
| matrix_row_dot | matrix.row_dot | unreached (honest wall / native arm covers the surface) |
| matrix_rows | matrix.rows | unreached (honest wall / native arm covers the surface) |
| matrix_scale | matrix.scale | unreached (honest wall / native arm covers the surface) |
| matrix_scaled_dot_product_attention | matrix.scaled_dot_product_attention | unreached (honest wall / native arm covers the surface) |
| matrix_select_rows_f32 | matrix.select_rows_f32 | unreached (honest wall / native arm covers the surface) |
| matrix_select_rows_q1_0 | matrix.select_rows_q1_0 | unreached (honest wall / native arm covers the surface) |
| matrix_select_rows_q8_0_dq | matrix.select_rows_q8_0_dq | unreached (honest wall / native arm covers the surface) |
| matrix_shape | matrix.shape | unreached (honest wall / native arm covers the surface) |
| matrix_silu_mul | matrix.silu_mul | unreached (honest wall / native arm covers the surface) |
| matrix_slice_rows | matrix.slice_rows | unreached (honest wall / native arm covers the surface) |
| matrix_softmax_rows | matrix.softmax_rows | unreached (honest wall / native arm covers the surface) |
| matrix_split_cols_even | matrix.split_cols_even | unreached (honest wall / native arm covers the surface) |
| matrix_sub | matrix.sub | unreached (honest wall / native arm covers the surface) |
| matrix_swiglu_gate | matrix.swiglu_gate | unreached (honest wall / native arm covers the surface) |
| matrix_to_bytes_f32_le | matrix.to_bytes_f32_le | unreached (honest wall / native arm covers the surface) |
| matrix_to_bytes_f64_le | matrix.to_bytes_f64_le | unreached (honest wall / native arm covers the surface) |
| matrix_to_lists | matrix.to_lists | unreached (honest wall / native arm covers the surface) |
| matrix_transpose | matrix.transpose | unreached (honest wall / native arm covers the surface) |
| matrix_zeros | matrix.zeros | unreached (honest wall / native arm covers the surface) |
| matrix_zeros_f32 | matrix.zeros_f32 | unreached (honest wall / native arm covers the surface) |
| option_collect | option.collect | unreached (honest wall / native arm covers the surface) |
| option_collect_map | option.collect_map | unreached (honest wall / native arm covers the surface) |
| option_filter | option.filter | unreached (honest wall / native arm covers the surface) |
| option_filter_h | option.filter_h | unreached (honest wall / native arm covers the surface) |
| option_flat_map | option.flat_map | unreached (honest wall / native arm covers the surface) |
| option_flatten | option.flatten | unreached (honest wall / native arm covers the surface) |
| option_flatten_h | option.flatten_h | unreached (honest wall / native arm covers the surface) |
| option_is_none | option.is_none | unreached (honest wall / native arm covers the surface) |
| option_is_some | option.is_some | unreached (honest wall / native arm covers the surface) |
| option_listint_unwrap_or | option.listint_unwrap_or | unreached (honest wall / native arm covers the surface) |
| option_liststr_unwrap_or | option.liststr_unwrap_or | unreached (honest wall / native arm covers the surface) |
| option_listvalue_unwrap_or | option.listvalue_unwrap_or | unreached (honest wall / native arm covers the surface) |
| option_map | option.map | unreached (honest wall / native arm covers the surface) |
| option_map_h | option.map_h | unreached (honest wall / native arm covers the surface) |
| option_or_else | option.or_else | unreached (honest wall / native arm covers the surface) |
| option_or_else_h | option.or_else_h | unreached (honest wall / native arm covers the surface) |
| option_to_list | option.to_list | unreached (honest wall / native arm covers the surface) |
| option_to_list_rc | option.to_list_rc | unreached (honest wall / native arm covers the surface) |
| option_to_result | option.to_result | unreached (honest wall / native arm covers the surface) |
| option_to_result_h | option.to_result_h | unreached (honest wall / native arm covers the surface) |
| option_to_result_ve | option.to_result_ve | unreached (honest wall / native arm covers the surface) |
| option_to_string | option.to_string | unreached (honest wall / native arm covers the surface) |
| option_to_string_b | option.to_string_b | unreached (honest wall / native arm covers the surface) |
| option_to_string_f | option.to_string_f | unreached (honest wall / native arm covers the surface) |
| option_to_string_lb | option.to_string_lb | unreached (honest wall / native arm covers the surface) |
| option_to_string_lf | option.to_string_lf | unreached (honest wall / native arm covers the surface) |
| option_to_string_li | option.to_string_li | unreached (honest wall / native arm covers the surface) |
| option_to_string_ls | option.to_string_ls | unreached (honest wall / native arm covers the surface) |
| option_to_string_msi | option.to_string_msi | unreached (honest wall / native arm covers the surface) |
| option_to_string_ob | option.to_string_ob | unreached (honest wall / native arm covers the surface) |
| option_to_string_oi | option.to_string_oi | unreached (honest wall / native arm covers the surface) |
| option_to_string_ooi | option.to_string_ooi | unreached (honest wall / native arm covers the surface) |
| option_to_string_ooli | option.to_string_ooli | unreached (honest wall / native arm covers the surface) |
| option_to_string_os | option.to_string_os | unreached (honest wall / native arm covers the surface) |
| option_to_string_ri | option.to_string_ri | unreached (honest wall / native arm covers the surface) |
| option_to_string_rli | option.to_string_rli | unreached (honest wall / native arm covers the surface) |
| option_to_string_rs | option.to_string_rs | unreached (honest wall / native arm covers the surface) |
| option_to_string_s | option.to_string_s | unreached (honest wall / native arm covers the surface) |
| option_unwrap_or | option.unwrap_or | unreached (honest wall / native arm covers the surface) |
| option_unwrap_or_else | option.unwrap_or_else | unreached (honest wall / native arm covers the surface) |
| option_unwrap_or_else_h | option.unwrap_or_else_h | unreached (honest wall / native arm covers the surface) |
| option_unwrap_or_hx | option.unwrap_or_hx | unreached (honest wall / native arm covers the surface) |
| option_unwrap_or_str | option.unwrap_or_str | unreached (honest wall / native arm covers the surface) |
| option_value_unwrap_or | option.value_unwrap_or | unreached (honest wall / native arm covers the surface) |
| option_zip | option.zip | unreached (honest wall / native arm covers the surface) |
| process_args | process.args | unreached (honest wall / native arm covers the surface) |
| random_choice | random.choice | unreached (honest wall / native arm covers the surface) |
| random_choice_pair | random.choice_pair | unreached (honest wall / native arm covers the surface) |
| random_choice_str | random.choice_str | unreached (honest wall / native arm covers the surface) |
| random_float | random.float | unreached (honest wall / native arm covers the surface) |
| random_int | random.int | linked (bytes family SUM) |
| random_shuffle | random.shuffle | unreached (honest wall / native arm covers the surface) |
| random_shuffle_pair | random.shuffle_pair | unreached (honest wall / native arm covers the surface) |
| random_shuffle_str | random.shuffle_str | unreached (honest wall / native arm covers the surface) |
| regex_captures | regex.captures | unreached (honest wall / native arm covers the surface) |
| regex_find | regex.find | unreached (honest wall / native arm covers the surface) |
| regex_find_all | regex.find_all | linked (bytes family SUM) |
| regex_full_match | regex.full_match | linked (bytes family SUM) |
| regex_is_match | regex.is_match | linked (bytes family SUM) |
| regex_replace | regex.replace | linked (bytes family SUM) |
| regex_replace_first | regex.replace_first | linked (bytes family SUM) |
| regex_split | regex.split | linked (bytes family SUM) |
| result_filter | result.filter | unreached (honest wall / native arm covers the surface) |
| result_flat_map | result.flat_map | unreached (honest wall / native arm covers the surface) |
| result_flat_map_h | result.flat_map_h | unreached (honest wall / native arm covers the surface) |
| result_flat_unwrap_or | result.flat_unwrap_or | unreached (honest wall / native arm covers the surface) |
| result_flatten | result.flatten | unreached (honest wall / native arm covers the surface) |
| result_is_err | result.is_err | unreached (honest wall / native arm covers the surface) |
| result_is_err_h | result.is_err_h | unreached (honest wall / native arm covers the surface) |
| result_is_ok | result.is_ok | unreached (honest wall / native arm covers the surface) |
| result_is_ok_h | result.is_ok_h | unreached (honest wall / native arm covers the surface) |
| result_list_value_unwrap_or | result.list_value_unwrap_or | unreached (honest wall / native arm covers the surface) |
| result_map | result.map | unreached (honest wall / native arm covers the surface) |
| result_map_err | result.map_err | unreached (honest wall / native arm covers the surface) |
| result_map_err_h | result.map_err_h | unreached (honest wall / native arm covers the surface) |
| result_map_h | result.map_h | unreached (honest wall / native arm covers the surface) |
| result_map_s2h | result.map_s2h | unreached (honest wall / native arm covers the surface) |
| result_or_else | result.or_else | unreached (honest wall / native arm covers the surface) |
| result_partition | result.partition | REJECTED: raw list/tuple internals (load_str/store_str + list header reads) |
| result_str_unwrap_or | result.str_unwrap_or | unreached (honest wall / native arm covers the surface) |
| result_to_err_option | result.to_err_option | unreached (honest wall / native arm covers the surface) |
| result_to_err_option_h | result.to_err_option_h | unreached (honest wall / native arm covers the surface) |
| result_to_list | result.to_list | unreached (honest wall / native arm covers the surface) |
| result_to_option | result.to_option | unreached (honest wall / native arm covers the surface) |
| result_to_option_h | result.to_option_h | unreached (honest wall / native arm covers the surface) |
| result_to_string | result.to_string | unreached (honest wall / native arm covers the surface) |
| result_to_string_b | result.to_string_b | unreached (honest wall / native arm covers the surface) |
| result_to_string_f | result.to_string_f | unreached (honest wall / native arm covers the surface) |
| result_to_string_lb | result.to_string_lb | unreached (honest wall / native arm covers the surface) |
| result_to_string_lf | result.to_string_lf | unreached (honest wall / native arm covers the surface) |
| result_to_string_li | result.to_string_li | unreached (honest wall / native arm covers the surface) |
| result_to_string_ls | result.to_string_ls | unreached (honest wall / native arm covers the surface) |
| result_to_string_msi | result.to_string_msi | unreached (honest wall / native arm covers the surface) |
| result_to_string_oi | result.to_string_oi | unreached (honest wall / native arm covers the surface) |
| result_to_string_oli | result.to_string_oli | unreached (honest wall / native arm covers the surface) |
| result_to_string_os | result.to_string_os | unreached (honest wall / native arm covers the surface) |
| result_to_string_osl | result.to_string_osl | unreached (honest wall / native arm covers the surface) |
| result_to_string_ri | result.to_string_ri | unreached (honest wall / native arm covers the surface) |
| result_to_string_ss | result.to_string_ss | unreached (honest wall / native arm covers the surface) |
| result_unwrap_or | result.unwrap_or | unreached (honest wall / native arm covers the surface) |
| result_unwrap_or_else | result.unwrap_or_else | unreached (honest wall / native arm covers the surface) |
| result_unwrap_or_else_h | result.unwrap_or_else_h | unreached (honest wall / native arm covers the surface) |
| result_unwrap_or_hx | result.unwrap_or_hx | unreached (honest wall / native arm covers the surface) |
| result_value_unwrap_or | result.value_unwrap_or | unreached (honest wall / native arm covers the surface) |
| result_zip | result.zip | unreached (honest wall / native arm covers the surface) |
| set_all | set.all | unreached (honest wall / native arm covers the surface) |
| set_all_str | set.all_str | unreached (honest wall / native arm covers the surface) |
| set_any | set.any | unreached (honest wall / native arm covers the surface) |
| set_any_str | set.any_str | unreached (honest wall / native arm covers the surface) |
| set_contains | set.contains | unreached (honest wall / native arm covers the surface) |
| set_contains_srec | set.contains_srec | unreached (honest wall / native arm covers the surface) |
| set_contains_str | set.contains_str | unreached (honest wall / native arm covers the surface) |
| set_difference | set.difference | unreached (honest wall / native arm covers the surface) |
| set_difference_str | set.difference_str | unreached (honest wall / native arm covers the surface) |
| set_eq_str | set.eq_str | unreached (honest wall / native arm covers the surface) |
| set_filter | set.filter | unreached (honest wall / native arm covers the surface) |
| set_filter_str | set.filter_str | unreached (honest wall / native arm covers the surface) |
| set_fold | set.fold | unreached (honest wall / native arm covers the surface) |
| set_fold_str | set.fold_str | unreached (honest wall / native arm covers the surface) |
| set_fold_str_hacc | set.fold_str_hacc | unreached (honest wall / native arm covers the surface) |
| set_from_list | set.from_list | unreached (honest wall / native arm covers the surface) |
| set_from_list_srec | set.from_list_srec | unreached (honest wall / native arm covers the surface) |
| set_from_list_str | set.from_list_str | unreached (honest wall / native arm covers the surface) |
| set_insert | set.insert | unreached (honest wall / native arm covers the surface) |
| set_insert_srec | set.insert_srec | unreached (honest wall / native arm covers the surface) |
| set_insert_str | set.insert_str | unreached (honest wall / native arm covers the surface) |
| set_intersection | set.intersection | unreached (honest wall / native arm covers the surface) |
| set_intersection_str | set.intersection_str | unreached (honest wall / native arm covers the surface) |
| set_is_disjoint | set.is_disjoint | unreached (honest wall / native arm covers the surface) |
| set_is_disjoint_str | set.is_disjoint_str | unreached (honest wall / native arm covers the surface) |
| set_is_empty | set.is_empty | unreached (honest wall / native arm covers the surface) |
| set_is_subset | set.is_subset | unreached (honest wall / native arm covers the surface) |
| set_is_subset_str | set.is_subset_str | unreached (honest wall / native arm covers the surface) |
| set_len | set.len | unreached (honest wall / native arm covers the surface) |
| set_map | set.map | unreached (honest wall / native arm covers the surface) |
| set_map_i2s | set.map_i2s | unreached (honest wall / native arm covers the surface) |
| set_new | set.new | unreached (honest wall / native arm covers the surface) |
| set_new_str | set.new_str | unreached (honest wall / native arm covers the surface) |
| set_remove | set.remove | unreached (honest wall / native arm covers the surface) |
| set_remove_str | set.remove_str | unreached (honest wall / native arm covers the surface) |
| set_symmetric_difference | set.symmetric_difference | unreached (honest wall / native arm covers the surface) |
| set_symmetric_difference_str | set.symmetric_difference_str | unreached (honest wall / native arm covers the surface) |
| set_to_list | set.to_list | unreached (honest wall / native arm covers the surface) |
| set_to_list_str | set.to_list_str | unreached (honest wall / native arm covers the surface) |
| set_to_string | set.to_string | unreached (honest wall / native arm covers the surface) |
| set_to_string_s | set.to_string_s | unreached (honest wall / native arm covers the surface) |
| set_union | set.union | unreached (honest wall / native arm covers the surface) |
| set_union_str | set.union_str | unreached (honest wall / native arm covers the surface) |
| string_capitalize | string.capitalize | unreached (honest wall / native arm covers the surface) |
| string_char_at | string.get | unreached (honest wall / native arm covers the surface) |
| string_chars | string.chars | unreached (honest wall / native arm covers the surface) |
| string_clear | string.clear | unreached (honest wall / native arm covers the surface) |
| string_cmp | string.cmp | unreached (honest wall / native arm covers the surface) |
| string_codepoint | string.codepoint | unreached (honest wall / native arm covers the surface) |
| string_contains | string.contains | linked (scalar/text) |
| string_count | string.count | linked (scalar/text) |
| string_drop | string.drop | unreached (honest wall / native arm covers the surface) |
| string_drop_end | string.drop_end | linked (calls.rs VERIFIED) |
| string_ends_with | string.ends_with | unreached (honest wall / native arm covers the surface) |
| string_eq | string.eq | unreached (honest wall / native arm covers the surface) |
| string_first | string.first | unreached (honest wall / native arm covers the surface) |
| string_from_bytes | string.from_bytes | REJECTED: raw list-header read (len=count vs bytes) — from_bytes composes from_list + linked lossy instead |
| string_from_codepoint | string.from_codepoint | linked (calls.rs VERIFIED) |
| string_index_of | string.index_of | linked (scalar/text SUM) |
| string_is_alpha | string.is_alpha | linked (scalar/text) |
| string_is_alphanumeric_uni | string.is_alphanumeric | linked (scalar/text) |
| string_is_digit | string.is_digit | linked (scalar/text) |
| string_is_empty | string.is_empty | unreached (honest wall / native arm covers the surface) |
| string_is_lower | string.is_lower | linked (scalar/text) |
| string_is_upper | string.is_upper | linked (scalar/text) |
| string_is_whitespace | string.is_whitespace | linked (scalar/text) |
| string_join | string.join | unreached (honest wall / native arm covers the surface) |
| string_last | string.last | unreached (honest wall / native arm covers the surface) |
| string_last_index_of | string.last_index_of | linked (scalar/text SUM) |
| string_len | string.len | unreached (honest wall / native arm covers the surface) |
| string_length | string.length | unreached (honest wall / native arm covers the surface) |
| string_lines | string.lines | unreached (honest wall / native arm covers the surface) |
| string_pad_end | string.pad_end | unreached (honest wall / native arm covers the surface) |
| string_pad_start | string.pad_start | unreached (honest wall / native arm covers the surface) |
| string_quote | string.quote | unreached (honest wall / native arm covers the surface) |
| string_repeat | string.repeat | unreached (honest wall / native arm covers the surface) |
| string_replace | string.replace | unreached (honest wall / native arm covers the surface) |
| string_replace_first | string.replace_first | unreached (honest wall / native arm covers the surface) |
| string_reverse | string.reverse | unreached (honest wall / native arm covers the surface) |
| string_run_length_encode | string.run_length_encode | unreached (honest wall / native arm covers the surface) |
| string_slice | string.slice | unreached (honest wall / native arm covers the surface) |
| string_slice2 | string.slice2 | unreached (honest wall / native arm covers the surface) |
| string_split | string.split | unreached (honest wall / native arm covers the surface) |
| string_split_once | string.split_once | unreached (honest wall / native arm covers the surface) |
| string_starts_with | string.starts_with | unreached (honest wall / native arm covers the surface) |
| string_strip_prefix | string.strip_prefix | unreached (honest wall / native arm covers the surface) |
| string_strip_suffix | string.strip_suffix | unreached (honest wall / native arm covers the surface) |
| string_take | string.take | unreached (honest wall / native arm covers the surface) |
| string_take_end | string.take_end | linked (calls.rs VERIFIED) |
| string_to_bytes | string.to_bytes | linked (scalar/text) |
| string_to_int | int.parse | linked (calls.rs SUM tier) |
| string_to_lower | string.to_lower | linked (calls.rs VERIFIED) |
| string_to_upper | string.to_upper | linked (calls.rs VERIFIED) |
| string_trim | string.trim | linked (calls.rs VERIFIED) |
| string_trim_end | string.trim_end | linked (scalar/text) |
| string_trim_start | string.trim_start | linked (scalar/text) |
| testing_assert_approx | testing.assert_approx | unreached (honest wall / native arm covers the surface) |
| testing_assert_contains | testing.assert_contains | unreached (honest wall / native arm covers the surface) |
| testing_assert_err | testing.assert_err | unreached (honest wall / native arm covers the surface) |
| testing_assert_err_sc | testing.assert_err_sc | unreached (honest wall / native arm covers the surface) |
| testing_assert_gt | testing.assert_gt | unreached (honest wall / native arm covers the surface) |
| testing_assert_lt | testing.assert_lt | unreached (honest wall / native arm covers the surface) |
| testing_assert_none | testing.assert_none | unreached (honest wall / native arm covers the surface) |
| testing_assert_ok | testing.assert_ok | unreached (honest wall / native arm covers the surface) |
| testing_assert_ok_sc | testing.assert_ok_sc | unreached (honest wall / native arm covers the surface) |
| testing_assert_some | testing.assert_some | unreached (honest wall / native arm covers the surface) |
| uint16_max_value | uint16.max_value | linked (sized-convert) |
| uint16_min_value | uint16.min_value | linked (sized-convert) |
| uint16_to_float32 | uint16.to_float32 | unreached (honest wall / native arm covers the surface) |
| uint16_to_float64 | uint16.to_float64 | linked (sized-convert) |
| uint16_to_int16 | uint16.to_int16 | linked (sized-convert) |
| uint16_to_int16_checked | uint16.to_int16_checked | linked (sized-convert SUM) |
| uint16_to_int16_saturating | uint16.to_int16_saturating | linked (sized-convert) |
| uint16_to_int32 | uint16.to_int32 | linked (sized-convert) |
| uint16_to_int64 | uint16.to_int64 | linked (sized-convert) |
| uint16_to_int8 | uint16.to_int8 | linked (sized-convert) |
| uint16_to_int8_checked | uint16.to_int8_checked | linked (sized-convert SUM) |
| uint16_to_int8_saturating | uint16.to_int8_saturating | linked (sized-convert) |
| uint16_to_string | uint16.to_string | linked (sized-convert) |
| uint16_to_uint32 | uint16.to_uint32 | linked (sized-convert) |
| uint16_to_uint64 | uint16.to_uint64 | linked (sized-convert) |
| uint16_to_uint8 | uint16.to_uint8 | linked (sized-convert) |
| uint16_to_uint8_checked | uint16.to_uint8_checked | linked (sized-convert SUM) |
| uint16_to_uint8_saturating | uint16.to_uint8_saturating | linked (sized-convert) |
| uint32_max_value | uint32.max_value | linked (sized-convert) |
| uint32_min_value | uint32.min_value | linked (sized-convert) |
| uint32_to_float32 | uint32.to_float32 | unreached (honest wall / native arm covers the surface) |
| uint32_to_float64 | uint32.to_float64 | linked (sized-convert) |
| uint32_to_int16 | uint32.to_int16 | linked (sized-convert) |
| uint32_to_int16_checked | uint32.to_int16_checked | linked (sized-convert SUM) |
| uint32_to_int16_saturating | uint32.to_int16_saturating | linked (sized-convert) |
| uint32_to_int32 | uint32.to_int32 | linked (sized-convert) |
| uint32_to_int32_checked | uint32.to_int32_checked | linked (sized-convert SUM) |
| uint32_to_int32_saturating | uint32.to_int32_saturating | linked (sized-convert) |
| uint32_to_int64 | uint32.to_int64 | linked (sized-convert) |
| uint32_to_int8 | uint32.to_int8 | linked (sized-convert) |
| uint32_to_int8_checked | uint32.to_int8_checked | linked (sized-convert SUM) |
| uint32_to_int8_saturating | uint32.to_int8_saturating | linked (sized-convert) |
| uint32_to_string | uint32.to_string | linked (sized-convert) |
| uint32_to_uint16 | uint32.to_uint16 | linked (sized-convert) |
| uint32_to_uint16_checked | uint32.to_uint16_checked | linked (sized-convert SUM) |
| uint32_to_uint16_saturating | uint32.to_uint16_saturating | linked (sized-convert) |
| uint32_to_uint64 | uint32.to_uint64 | linked (sized-convert) |
| uint32_to_uint8 | uint32.to_uint8 | linked (sized-convert) |
| uint32_to_uint8_checked | uint32.to_uint8_checked | linked (sized-convert SUM) |
| uint32_to_uint8_saturating | uint32.to_uint8_saturating | linked (sized-convert) |
| uint64_max_value | uint64.max_value | linked (sized-convert) |
| uint64_min_value | uint64.min_value | linked (sized-convert) |
| uint64_to_float32 | uint64.to_float32 | unreached (honest wall / native arm covers the surface) |
| uint64_to_float64 | uint64.to_float64 | linked (sized-convert) |
| uint64_to_int16 | uint64.to_int16 | linked (sized-convert) |
| uint64_to_int16_checked | uint64.to_int16_checked | linked (sized-convert SUM) |
| uint64_to_int16_saturating | uint64.to_int16_saturating | linked (sized-convert) |
| uint64_to_int32 | uint64.to_int32 | linked (sized-convert) |
| uint64_to_int32_checked | uint64.to_int32_checked | linked (sized-convert SUM) |
| uint64_to_int32_saturating | uint64.to_int32_saturating | linked (sized-convert) |
| uint64_to_int64 | uint64.to_int64 | linked (sized-convert) |
| uint64_to_int64_checked | uint64.to_int64_checked | linked (sized-convert SUM) |
| uint64_to_int64_saturating | uint64.to_int64_saturating | linked (sized-convert) |
| uint64_to_int8 | uint64.to_int8 | linked (sized-convert) |
| uint64_to_int8_checked | uint64.to_int8_checked | linked (sized-convert SUM) |
| uint64_to_int8_saturating | uint64.to_int8_saturating | linked (sized-convert) |
| uint64_to_string | uint64.to_string | linked (sized-convert) |
| uint64_to_uint16 | uint64.to_uint16 | linked (sized-convert) |
| uint64_to_uint16_checked | uint64.to_uint16_checked | linked (sized-convert SUM) |
| uint64_to_uint16_saturating | uint64.to_uint16_saturating | linked (sized-convert) |
| uint64_to_uint32 | uint64.to_uint32 | linked (sized-convert) |
| uint64_to_uint32_checked | uint64.to_uint32_checked | linked (sized-convert SUM) |
| uint64_to_uint32_saturating | uint64.to_uint32_saturating | linked (sized-convert) |
| uint64_to_uint8 | uint64.to_uint8 | linked (sized-convert) |
| uint64_to_uint8_checked | uint64.to_uint8_checked | linked (sized-convert SUM) |
| uint64_to_uint8_saturating | uint64.to_uint8_saturating | linked (sized-convert) |
| uint8_max_value | uint8.max_value | linked (sized-convert) |
| uint8_min_value | uint8.min_value | linked (sized-convert) |
| uint8_to_float32 | uint8.to_float32 | unreached (honest wall / native arm covers the surface) |
| uint8_to_float64 | uint8.to_float64 | linked (sized-convert) |
| uint8_to_int16 | uint8.to_int16 | linked (sized-convert) |
| uint8_to_int32 | uint8.to_int32 | linked (sized-convert) |
| uint8_to_int64 | uint8.to_int64 | linked (sized-convert) |
| uint8_to_int8 | uint8.to_int8 | linked (sized-convert) |
| uint8_to_int8_checked | uint8.to_int8_checked | linked (sized-convert SUM) |
| uint8_to_int8_saturating | uint8.to_int8_saturating | linked (sized-convert) |
| uint8_to_string | uint8.to_string | linked (sized-convert) |
| uint8_to_uint16 | uint8.to_uint16 | linked (sized-convert) |
| uint8_to_uint32 | uint8.to_uint32 | linked (sized-convert) |
| uint8_to_uint64 | uint8.to_uint64 | linked (sized-convert) |
| value_array | value.array | unreached (honest wall / native arm covers the surface) |
| value_as_array | value.as_array | unreached (honest wall / native arm covers the surface) |
| value_as_bool | value.as_bool | unreached (honest wall / native arm covers the surface) |
| value_as_float | value.as_float | unreached (honest wall / native arm covers the surface) |
| value_as_int | value.as_int | unreached (honest wall / native arm covers the surface) |
| value_as_string | value.as_string | unreached (honest wall / native arm covers the surface) |
| value_bool | value.bool | unreached (honest wall / native arm covers the surface) |
| value_eq | value.eq | REJECTED: incumbent len-as-tag Value layout — native helper $value_eq instead |
| value_field | value.field | unreached (honest wall / native arm covers the surface) |
| value_float | value.float | unreached (honest wall / native arm covers the surface) |
| value_int | value.int | unreached (honest wall / native arm covers the surface) |
| value_keys | value.keys | unreached (honest wall / native arm covers the surface) |
| value_merge | value.merge | REJECTED: incumbent len-as-tag Value layout — native helper $value_merge instead |
| value_null | value.null | unreached (honest wall / native arm covers the surface) |
| value_object | value.object | unreached (honest wall / native arm covers the surface) |
| value_omit | value.omit | unreached (honest wall / native arm covers the surface) |
| value_pick | value.pick | REJECTED: raw Value internals |
| value_str | value.str | unreached (honest wall / native arm covers the surface) |
| value_stringify | value.stringify | unreached (honest wall / native arm covers the surface) |
| value_to_camel_case | value.to_camel_case | unreached (honest wall / native arm covers the surface) |
| value_to_snake_case | value.to_snake_case | unreached (honest wall / native arm covers the surface) |
