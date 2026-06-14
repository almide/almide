//! datetime module + decimal helpers — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use super::values;
use wasm_encoder::Instruction;

// ── Time unit constants ────────────────────────────────────────────────────────
const SECS_PER_DAY: i64 = 86400;
const SECS_PER_DAY_M1: i64 = 86399; // SECS_PER_DAY - 1; used for floor-div of negative timestamps
const SECS_PER_HOUR: i64 = 3600;
const SECS_PER_MINUTE: i64 = 60;
const NANOS_PER_SEC: i64 = 1_000_000_000;

// ── Julian Day Number (forward JDN) constants ─────────────────────────────────
// Algorithm: a=(14-month)/12, y=year+4800-a, m=month+12*a-3
// jdn = day + (153*m+2)/5 + 365*y + y/4 - y/100 + y/400 - 32045
const JDN_MONTH_ADJ: i64 = 14;       // a = (14 - month) / 12
const MONTHS_PER_YEAR: i64 = 12;
const JDN_YEAR_BIAS: i64 = 4800;     // y = year + 4800 - a
const JDN_MONTH_SHIFT: i64 = 3;      // m = month + 12*a - 3
const JDN_DAYS_COEFF: i64 = 153;     // coefficient in (153*m+2)/5
const JDN_DAYS_ADJ: i64 = 2;         // addend in (153*m+2)/5
const JDN_DAYS_DIV: i64 = 5;         // divisor in (153*m+2)/5
const DAYS_PER_YEAR: i64 = 365;
const JDN_LEAP_DIV: i64 = 4;         // y/4 leap-year term
const JDN_CENTURY_DIV: i64 = 100;    // y/100 century term
const JDN_LEAP_EXCEPTION_DIV: i64 = 400; // y/400 exception term
const JDN_EPOCH_ADJ: i64 = 32045;    // constant subtracted at end of forward JDN
const JDN_UNIX_EPOCH: i64 = 2440588; // JDN of 1970-01-01 (Unix epoch)

// ── Inverse JDN (Richards 2013) constants ─────────────────────────────────────
// Converts JDN back to (year, month, day).
const JDN_INV_C1: i64 = 1401;        // civil-calendar day offset
const JDN_INV_C2: i64 = 274277;      // Gregorian correction numerator
const JDN_DAYS_PER_400YR: i64 = 146097; // days in a 400-year Gregorian cycle
const JDN_GREG_CORR: i64 = 3;        // factor in Gregorian correction (C2/C3*3/4)
const JDN_INV_C3: i64 = 38;          // correction offset subtracted after Gregorian step
const JDN_DAYS_PER_4YR: i64 = 1461;  // days in a 4-year Julian cycle
const JDN_MONTH_OFS: i64 = 2;        // month-numbering offset in h/153+2
const JDN_INV_YEAR_BIAS: i64 = 4716; // year bias subtracted in inverse JDN

// ── Weekday constants ─────────────────────────────────────────────────────────
// 0=Sunday, 1=Monday, 2=Tuesday, 3=Wednesday, 4=Thursday, 5=Friday, 6=Saturday
const DAYS_PER_WEEK: i64 = 7;
const WEEKDAY_EPOCH_OFS: i64 = 4;    // Jan 1 1970 was a Thursday (day 4 in 0=Sun scheme)
const WD_TUE: i64 = 2;
const WD_WED: i64 = 3;
const WD_THU: i64 = 4;               // also reused as WEEKDAY_EPOCH_OFS (same day)
const WD_FRI: i64 = 5;

// ── Decimal digit helpers ─────────────────────────────────────────────────────
const DECIMAL_BASE: i64 = 10;
const ASCII_ZERO: i64 = 48;          // '0'; used as i64 for digit encode/decode

// ── ASCII character codes for ISO-8601 separators (i32 for i32_store8) ───────
const ASCII_HYPHEN: i32 = 45;        // '-'
const ASCII_COLON: i32 = 58;         // ':'
const ASCII_T: i32 = 84;             // 'T'
const ASCII_Z: i32 = 90;             // 'Z'

// ── Memory / allocation sizes ─────────────────────────────────────────────────
const ISO_STRING_LEN: i32 = 20;      // byte length of "YYYY-MM-DDTHH:MM:SSZ"
const ISO_MIN_INPUT_LEN: i32 = 19;   // minimum i32_load(0) check for parse_iso
const RESULT_ERR_ALLOC: i32 = 8;     // bytes for Err Result: [tag:i32][ptr:i32]
const RESULT_OK_ALLOC: i32 = 12;     // bytes for Ok Result: [tag:i32][timestamp:i64]
const ALIGN8_BUF_BYTES: i32 = 16;    // allocation buf to guarantee 8-byte alignment
const ALIGN8_MASK_LOW: i32 = 7;      // round-up addend: ptr + 7 & -8
const ALIGN8_MASK_NEG: i32 = -8;     // alignment mask (0xFFFFFFF8)

impl FuncCompiler<'_> {
    pub(super) fn emit_datetime_call(&mut self, func: &str, args: &[IrExpr]) {
        match func {
            "from_parts" => {
                // datetime.from_parts(year, month, day, hour, minute, second) → Int
                // JDN algorithm: a=(14-month)/12, y=year+4800-a, m=month+12*a-3
                // jdn = day + (153*m+2)/5 + 365*y + y/4 - y/100 + y/400 - 32045
                // timestamp = (jdn - 2440588) * 86400 + h*3600 + min*60 + sec
                let year = self.scratch.alloc_i64();
                let month = self.scratch.alloc_i64();
                let day = self.scratch.alloc_i64();
                let hour = self.scratch.alloc_i64();
                let minute = self.scratch.alloc_i64();
                let second = self.scratch.alloc_i64();
                let a = self.scratch.alloc_i64();
                let y = self.scratch.alloc_i64();
                let m = self.scratch.alloc_i64();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(year); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(month); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(day); });
                self.emit_expr(&args[3]);
                wasm!(self.func, { local_set(hour); });
                self.emit_expr(&args[4]);
                wasm!(self.func, { local_set(minute); });
                self.emit_expr(&args[5]);
                wasm!(self.func, { local_set(second); });

                wasm!(self.func, {
                    i64_const(JDN_MONTH_ADJ); local_get(month); i64_sub; i64_const(MONTHS_PER_YEAR); i64_div_s; local_set(a);
                    local_get(year); i64_const(JDN_YEAR_BIAS); i64_add; local_get(a); i64_sub; local_set(y);
                    local_get(month); i64_const(MONTHS_PER_YEAR); local_get(a); i64_mul; i64_add; i64_const(JDN_MONTH_SHIFT); i64_sub; local_set(m);
                    local_get(day);
                    i64_const(JDN_DAYS_COEFF); local_get(m); i64_mul; i64_const(JDN_DAYS_ADJ); i64_add; i64_const(JDN_DAYS_DIV); i64_div_s;
                    i64_add;
                    i64_const(DAYS_PER_YEAR); local_get(y); i64_mul;
                    i64_add;
                    local_get(y); i64_const(JDN_LEAP_DIV); i64_div_s;
                    i64_add;
                    local_get(y); i64_const(JDN_CENTURY_DIV); i64_div_s;
                    i64_sub;
                    local_get(y); i64_const(JDN_LEAP_EXCEPTION_DIV); i64_div_s;
                    i64_add;
                    i64_const(JDN_EPOCH_ADJ); i64_sub;
                    i64_const(JDN_UNIX_EPOCH); i64_sub;
                    i64_const(SECS_PER_DAY); i64_mul;
                    local_get(hour); i64_const(SECS_PER_HOUR); i64_mul; i64_add;
                    local_get(minute); i64_const(SECS_PER_MINUTE); i64_mul; i64_add;
                    local_get(second); i64_add;
                });

                self.scratch.free_i64(m);
                self.scratch.free_i64(y);
                self.scratch.free_i64(a);
                self.scratch.free_i64(second);
                self.scratch.free_i64(minute);
                self.scratch.free_i64(hour);
                self.scratch.free_i64(day);
                self.scratch.free_i64(month);
                self.scratch.free_i64(year);
            }
            "year" | "month" | "day" => {
                // Inverse JDN algorithm to extract date component from timestamp.
                let ts = self.scratch.alloc_i64();
                let d = self.scratch.alloc_i64();
                let f = self.scratch.alloc_i64();
                let e = self.scratch.alloc_i64();
                let g = self.scratch.alloc_i64();
                let h = self.scratch.alloc_i64();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(ts);
                    // floor(ts / SECS_PER_DAY)
                    local_get(ts); i64_const(0); i64_ge_s;
                    if_i64;
                      local_get(ts); i64_const(SECS_PER_DAY); i64_div_s;
                    else_;
                      local_get(ts); i64_const(SECS_PER_DAY_M1); i64_sub; i64_const(SECS_PER_DAY); i64_div_s;
                    end;
                    local_set(d);
                    local_get(d); i64_const(JDN_UNIX_EPOCH); i64_add; local_set(d);
                    local_get(d); i64_const(JDN_INV_C1); i64_add;
                    i64_const(JDN_LEAP_DIV); local_get(d); i64_mul; i64_const(JDN_INV_C2); i64_add;
                    i64_const(JDN_DAYS_PER_400YR); i64_div_s; i64_const(JDN_GREG_CORR); i64_mul; i64_const(JDN_LEAP_DIV); i64_div_s;
                    i64_add; i64_const(JDN_INV_C3); i64_sub;
                    local_set(f);
                    i64_const(JDN_LEAP_DIV); local_get(f); i64_mul; i64_const(JDN_MONTH_SHIFT); i64_add; local_set(e);
                    local_get(e); i64_const(JDN_DAYS_PER_4YR); i64_rem_s; i64_const(JDN_LEAP_DIV); i64_div_s; local_set(g);
                    i64_const(JDN_DAYS_DIV); local_get(g); i64_mul; i64_const(JDN_DAYS_ADJ); i64_add; local_set(h);
                });

                match func {
                    "day" => {
                        wasm!(self.func, {
                            local_get(h); i64_const(JDN_DAYS_COEFF); i64_rem_s; i64_const(JDN_DAYS_DIV); i64_div_s; i64_const(1); i64_add;
                        });
                    }
                    "month" => {
                        wasm!(self.func, {
                            local_get(h); i64_const(JDN_DAYS_COEFF); i64_div_s; i64_const(JDN_MONTH_OFS); i64_add;
                            i64_const(MONTHS_PER_YEAR); i64_rem_s; i64_const(1); i64_add;
                        });
                    }
                    "year" => {
                        let mm = self.scratch.alloc_i64();
                        wasm!(self.func, {
                            local_get(h); i64_const(JDN_DAYS_COEFF); i64_div_s; i64_const(JDN_MONTH_OFS); i64_add;
                            i64_const(MONTHS_PER_YEAR); i64_rem_s; i64_const(1); i64_add;
                            local_set(mm);
                            local_get(e); i64_const(JDN_DAYS_PER_4YR); i64_div_s; i64_const(JDN_INV_YEAR_BIAS); i64_sub;
                            i64_const(JDN_MONTH_ADJ); local_get(mm); i64_sub; i64_const(MONTHS_PER_YEAR); i64_div_s;
                            i64_add;
                        });
                        self.scratch.free_i64(mm);
                    }
                    _ => unreachable!(),
                }

                self.scratch.free_i64(h);
                self.scratch.free_i64(g);
                self.scratch.free_i64(e);
                self.scratch.free_i64(f);
                self.scratch.free_i64(d);
                self.scratch.free_i64(ts);
            }
            "hour" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i64_const(SECS_PER_DAY); i64_rem_s;
                    i64_const(SECS_PER_DAY); i64_add; i64_const(SECS_PER_DAY); i64_rem_s;
                    i64_const(SECS_PER_HOUR); i64_div_s;
                });
            }
            "minute" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i64_const(SECS_PER_HOUR); i64_rem_s;
                    i64_const(SECS_PER_HOUR); i64_add; i64_const(SECS_PER_HOUR); i64_rem_s;
                    i64_const(SECS_PER_MINUTE); i64_div_s;
                });
            }
            "second" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i64_const(SECS_PER_MINUTE); i64_rem_s;
                    i64_const(SECS_PER_MINUTE); i64_add; i64_const(SECS_PER_MINUTE); i64_rem_s;
                });
            }
            "now" => {
                // Call WASI clock_time_get(id=0 realtime, precision=0, time_ptr)
                // Returns nanoseconds as i64 at time_ptr, convert to seconds.
                // alloc returns (8n+4), need 8-byte aligned for i64 store.
                let time_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(ALIGN8_BUF_BYTES); call(self.emitter.rt.alloc);
                    i32_const(ALIGN8_MASK_LOW); i32_add; i32_const(ALIGN8_MASK_NEG); i32_and;
                    local_set(time_ptr);
                    i32_const(0); // clock_id: realtime
                    i64_const(0); // precision
                    local_get(time_ptr);
                    call(self.emitter.rt.clock_time_get);
                    drop; // discard error code
                    // Load i64 nanoseconds, convert to seconds
                    local_get(time_ptr); i64_load(0);
                    i64_const(NANOS_PER_SEC); i64_div_u;
                });
                self.scratch.free_i32(time_ptr);
            }
            "add_days" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_const(SECS_PER_DAY); i64_mul; i64_add; });
            }
            "add_hours" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_const(SECS_PER_HOUR); i64_mul; i64_add; });
            }
            "add_minutes" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_const(SECS_PER_MINUTE); i64_mul; i64_add; });
            }
            "add_seconds" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_add; });
            }
            "from_unix" | "to_unix" => {
                self.emit_expr(&args[0]);
            }
            "diff_seconds" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_sub; });
            }
            "is_before" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_lt_s; });
            }
            "is_after" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_gt_s; });
            }
            "diff_days" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_sub; i64_const(SECS_PER_DAY); i64_div_s; });
            }
            "format" => {
                // Stub: return int.to_string(ts), ignore fmt
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.int_to_string); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { drop; });
            }
            "to_iso" => {
                // datetime.to_iso(ts) → String "YYYY-MM-DDTHH:MM:SSZ"
                let ts = self.scratch.alloc_i64();
                let ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(ts); });

                wasm!(self.func, {
                    i32_const(ISO_STRING_LEN); call(self.emitter.rt.string_alloc); local_set(ptr);
                });

                let d = self.scratch.alloc_i64();
                let f = self.scratch.alloc_i64();
                let e = self.scratch.alloc_i64();
                let g = self.scratch.alloc_i64();
                let h = self.scratch.alloc_i64();
                let yr = self.scratch.alloc_i64();
                let mo = self.scratch.alloc_i64();
                let dy = self.scratch.alloc_i64();
                let hr = self.scratch.alloc_i64();
                let mi = self.scratch.alloc_i64();
                let se = self.scratch.alloc_i64();

                wasm!(self.func, {
                    local_get(ts); i64_const(0); i64_ge_s;
                    if_i64;
                      local_get(ts); i64_const(SECS_PER_DAY); i64_div_s;
                    else_;
                      local_get(ts); i64_const(SECS_PER_DAY_M1); i64_sub; i64_const(SECS_PER_DAY); i64_div_s;
                    end;
                    local_set(d);
                    local_get(d); i64_const(JDN_UNIX_EPOCH); i64_add; local_set(d);
                    local_get(d); i64_const(JDN_INV_C1); i64_add;
                    i64_const(JDN_LEAP_DIV); local_get(d); i64_mul; i64_const(JDN_INV_C2); i64_add;
                    i64_const(JDN_DAYS_PER_400YR); i64_div_s; i64_const(JDN_GREG_CORR); i64_mul; i64_const(JDN_LEAP_DIV); i64_div_s;
                    i64_add; i64_const(JDN_INV_C3); i64_sub; local_set(f);
                    i64_const(JDN_LEAP_DIV); local_get(f); i64_mul; i64_const(JDN_MONTH_SHIFT); i64_add; local_set(e);
                    local_get(e); i64_const(JDN_DAYS_PER_4YR); i64_rem_s; i64_const(JDN_LEAP_DIV); i64_div_s; local_set(g);
                    i64_const(JDN_DAYS_DIV); local_get(g); i64_mul; i64_const(JDN_DAYS_ADJ); i64_add; local_set(h);
                    local_get(h); i64_const(JDN_DAYS_COEFF); i64_rem_s; i64_const(JDN_DAYS_DIV); i64_div_s; i64_const(1); i64_add; local_set(dy);
                    local_get(h); i64_const(JDN_DAYS_COEFF); i64_div_s; i64_const(JDN_MONTH_OFS); i64_add;
                    i64_const(MONTHS_PER_YEAR); i64_rem_s; i64_const(1); i64_add; local_set(mo);
                    local_get(e); i64_const(JDN_DAYS_PER_4YR); i64_div_s; i64_const(JDN_INV_YEAR_BIAS); i64_sub;
                    i64_const(JDN_MONTH_ADJ); local_get(mo); i64_sub; i64_const(MONTHS_PER_YEAR); i64_div_s;
                    i64_add; local_set(yr);
                    local_get(ts); i64_const(SECS_PER_DAY); i64_rem_s; i64_const(SECS_PER_DAY); i64_add; i64_const(SECS_PER_DAY); i64_rem_s;
                    local_set(d);
                    local_get(d); i64_const(SECS_PER_HOUR); i64_div_s; local_set(hr);
                    local_get(d); i64_const(SECS_PER_HOUR); i64_rem_s; i64_const(SECS_PER_MINUTE); i64_div_s; local_set(mi);
                    local_get(d); i64_const(SECS_PER_MINUTE); i64_rem_s; local_set(se);
                });

                let d_off = self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA);
                self.emit_write_decimal_digits(ptr, d_off, yr, 4);        // YYYY
                wasm!(self.func, { local_get(ptr); i32_const(ASCII_HYPHEN); i32_store8(d_off + 4); }); // -
                self.emit_write_decimal_digits(ptr, d_off + 5, mo, 2);    // MM
                wasm!(self.func, { local_get(ptr); i32_const(ASCII_HYPHEN); i32_store8(d_off + 7); }); // -
                self.emit_write_decimal_digits(ptr, d_off + 8, dy, 2);    // DD
                wasm!(self.func, { local_get(ptr); i32_const(ASCII_T); i32_store8(d_off + 10); }); // T
                self.emit_write_decimal_digits(ptr, d_off + 11, hr, 2);   // HH
                wasm!(self.func, { local_get(ptr); i32_const(ASCII_COLON); i32_store8(d_off + 13); }); // :
                self.emit_write_decimal_digits(ptr, d_off + 14, mi, 2);   // MM
                wasm!(self.func, { local_get(ptr); i32_const(ASCII_COLON); i32_store8(d_off + 16); }); // :
                self.emit_write_decimal_digits(ptr, d_off + 17, se, 2);   // SS
                wasm!(self.func, { local_get(ptr); i32_const(ASCII_Z); i32_store8(d_off + 19); }); // Z

                wasm!(self.func, { local_get(ptr); });

                self.scratch.free_i64(se);
                self.scratch.free_i64(mi);
                self.scratch.free_i64(hr);
                self.scratch.free_i64(dy);
                self.scratch.free_i64(mo);
                self.scratch.free_i64(yr);
                self.scratch.free_i64(h);
                self.scratch.free_i64(g);
                self.scratch.free_i64(e);
                self.scratch.free_i64(f);
                self.scratch.free_i64(d);
                self.scratch.free_i32(ptr);
                self.scratch.free_i64(ts);
            }
            "weekday" => {
                // (floor(ts/86400) + 4) % 7: 0=Sun..6=Sat
                let ts = self.scratch.alloc_i64();
                let wd = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(ts);
                    local_get(ts); i64_const(0); i64_ge_s;
                    if_i64;
                      local_get(ts); i64_const(SECS_PER_DAY); i64_div_s;
                    else_;
                      local_get(ts); i64_const(SECS_PER_DAY_M1); i64_sub; i64_const(SECS_PER_DAY); i64_div_s;
                    end;
                    i64_const(WEEKDAY_EPOCH_OFS); i64_add;
                    i64_const(DAYS_PER_WEEK); i64_rem_s;
                    i64_const(DAYS_PER_WEEK); i64_add; i64_const(DAYS_PER_WEEK); i64_rem_s;
                    local_set(wd);
                });

                let sun = self.emitter.intern_string("Sunday");
                let mon = self.emitter.intern_string("Monday");
                let tue = self.emitter.intern_string("Tuesday");
                let wed = self.emitter.intern_string("Wednesday");
                let thu = self.emitter.intern_string("Thursday");
                let fri = self.emitter.intern_string("Friday");
                let sat = self.emitter.intern_string("Saturday");

                wasm!(self.func, {
                    local_get(wd); i64_eqz;
                    if_i32; i32_const(sun as i32);
                    else_;
                      local_get(wd); i64_const(1); i64_eq;
                      if_i32; i32_const(mon as i32);
                      else_;
                        local_get(wd); i64_const(WD_TUE); i64_eq;
                        if_i32; i32_const(tue as i32);
                        else_;
                          local_get(wd); i64_const(WD_WED); i64_eq;
                          if_i32; i32_const(wed as i32);
                          else_;
                            local_get(wd); i64_const(WD_THU); i64_eq;
                            if_i32; i32_const(thu as i32);
                            else_;
                              local_get(wd); i64_const(WD_FRI); i64_eq;
                              if_i32; i32_const(fri as i32);
                              else_;
                                i32_const(sat as i32);
                              end;
                            end;
                          end;
                        end;
                      end;
                    end;
                });

                self.scratch.free_i64(wd);
                self.scratch.free_i64(ts);
            }
            "parse_iso" => {
                // datetime.parse_iso(s: String) → Result[Int, String]
                let s = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let yr = self.scratch.alloc_i64();
                let mo = self.scratch.alloc_i64();
                let dy = self.scratch.alloc_i64();
                let hr = self.scratch.alloc_i64();
                let mi = self.scratch.alloc_i64();
                let se = self.scratch.alloc_i64();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });

                let err_msg = self.emitter.intern_string("invalid datetime format");
                wasm!(self.func, {
                    local_get(s); i32_load(0); i32_const(ISO_MIN_INPUT_LEN); i32_lt_u;
                    if_i32;
                      i32_const(RESULT_ERR_ALLOC); call(self.emitter.rt.alloc); local_set(result);
                      local_get(result); i32_const(1); i32_store(0);
                      local_get(result); i32_const(err_msg as i32); i32_store(4);
                      local_get(result);
                    else_;
                });

                self.emit_parse_digits(s, 0, 4, yr);
                self.emit_parse_digits(s, 5, 2, mo);
                self.emit_parse_digits(s, 8, 2, dy);
                self.emit_parse_digits(s, 11, 2, hr);
                self.emit_parse_digits(s, 14, 2, mi);
                self.emit_parse_digits(s, 17, 2, se);

                let a = self.scratch.alloc_i64();
                let y = self.scratch.alloc_i64();
                let m = self.scratch.alloc_i64();
                wasm!(self.func, {
                    i64_const(JDN_MONTH_ADJ); local_get(mo); i64_sub; i64_const(MONTHS_PER_YEAR); i64_div_s; local_set(a);
                    local_get(yr); i64_const(JDN_YEAR_BIAS); i64_add; local_get(a); i64_sub; local_set(y);
                    local_get(mo); i64_const(MONTHS_PER_YEAR); local_get(a); i64_mul; i64_add; i64_const(JDN_MONTH_SHIFT); i64_sub; local_set(m);
                    local_get(dy);
                    i64_const(JDN_DAYS_COEFF); local_get(m); i64_mul; i64_const(JDN_DAYS_ADJ); i64_add; i64_const(JDN_DAYS_DIV); i64_div_s; i64_add;
                    i64_const(DAYS_PER_YEAR); local_get(y); i64_mul; i64_add;
                    local_get(y); i64_const(JDN_LEAP_DIV); i64_div_s; i64_add;
                    local_get(y); i64_const(JDN_CENTURY_DIV); i64_div_s; i64_sub;
                    local_get(y); i64_const(JDN_LEAP_EXCEPTION_DIV); i64_div_s; i64_add;
                    i64_const(JDN_EPOCH_ADJ); i64_sub;
                    i64_const(JDN_UNIX_EPOCH); i64_sub;
                    i64_const(SECS_PER_DAY); i64_mul;
                    local_get(hr); i64_const(SECS_PER_HOUR); i64_mul; i64_add;
                    local_get(mi); i64_const(SECS_PER_MINUTE); i64_mul; i64_add;
                    local_get(se); i64_add;
                    local_set(yr); // reuse as timestamp
                    // Build ok Result: [tag=0:i32][timestamp:i64] = 12 bytes
                    i32_const(RESULT_OK_ALLOC); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(0); i32_store(0);
                    local_get(result); local_get(yr); i64_store(4);
                    local_get(result);
                    end;
                });

                self.scratch.free_i64(m);
                self.scratch.free_i64(y);
                self.scratch.free_i64(a);
                self.scratch.free_i64(se);
                self.scratch.free_i64(mi);
                self.scratch.free_i64(hr);
                self.scratch.free_i64(dy);
                self.scratch.free_i64(mo);
                self.scratch.free_i64(yr);
                self.scratch.free_i32(result);
                self.scratch.free_i32(s);
            }
            "monotonic_ns" => {
                // WASI clock_time_get(id=1 monotonic, precision=1, time_ptr)
                // Returns nanoseconds as i64.
                // alloc returns (8n+4) which is 4-byte aligned, but
                // clock_time_get needs 8-byte aligned output ptr.
                // Allocate 16 bytes so we can round up to 8-byte boundary.
                let time_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(ALIGN8_BUF_BYTES); call(self.emitter.rt.alloc);
                    i32_const(ALIGN8_MASK_LOW); i32_add; i32_const(ALIGN8_MASK_NEG); i32_and;
                    local_set(time_ptr);
                    i32_const(1); // clock_id: monotonic
                    i64_const(1); // precision: 1ns
                    local_get(time_ptr);
                    call(self.emitter.rt.clock_time_get);
                    drop; // discard error code
                    local_get(time_ptr); i64_load(0);
                });
                self.scratch.free_i32(time_ptr);
            }
            _ => panic!(
                "[ICE] emit_wasm: no WASM dispatch for `datetime.{}` — \
                 add an arm in emit_datetime_call or resolve upstream",
                func
            ),
        }
    }

    /// Write N decimal digits of an i64 value to a string buffer at a given byte offset.
    pub(super) fn emit_write_decimal_digits(&mut self, ptr: u32, byte_offset: u32, val: u32, num_digits: u32) {
        let tmp = self.scratch.alloc_i64();
        wasm!(self.func, { local_get(val); local_set(tmp); });
        for i in (0..num_digits).rev() {
            let off = byte_offset + i;
            wasm!(self.func, {
                local_get(ptr);
                local_get(tmp); i64_const(DECIMAL_BASE); i64_rem_s;
                i64_const(ASCII_ZERO); i64_add;
                i32_wrap_i64;
                i32_store8(off);
                local_get(tmp); i64_const(DECIMAL_BASE); i64_div_s; local_set(tmp);
            });
        }
        self.scratch.free_i64(tmp);
    }

    /// Parse N decimal ASCII digits from a string buffer into an i64 local.
    pub(super) fn emit_parse_digits(&mut self, str_local: u32, char_offset: u32, num_digits: u32, dest: u32) {
        let data_off = self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA);
        wasm!(self.func, { i64_const(0); local_set(dest); });
        for i in 0..num_digits {
            let off = data_off + char_offset + i;
            wasm!(self.func, {
                local_get(dest); i64_const(DECIMAL_BASE); i64_mul;
                local_get(str_local); i32_load8_u(off);
                i64_extend_i32_u; i64_const(ASCII_ZERO); i64_sub;
                i64_add;
                local_set(dest);
            });
        }
    }

}
