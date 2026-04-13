//! datetime module + decimal helpers — WASM codegen dispatch.

use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use super::values;
use wasm_encoder::Instruction;

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
                    i64_const(14); local_get(month); i64_sub; i64_const(12); i64_div_s; local_set(a);
                    local_get(year); i64_const(4800); i64_add; local_get(a); i64_sub; local_set(y);
                    local_get(month); i64_const(12); local_get(a); i64_mul; i64_add; i64_const(3); i64_sub; local_set(m);
                    local_get(day);
                    i64_const(153); local_get(m); i64_mul; i64_const(2); i64_add; i64_const(5); i64_div_s;
                    i64_add;
                    i64_const(365); local_get(y); i64_mul;
                    i64_add;
                    local_get(y); i64_const(4); i64_div_s;
                    i64_add;
                    local_get(y); i64_const(100); i64_div_s;
                    i64_sub;
                    local_get(y); i64_const(400); i64_div_s;
                    i64_add;
                    i64_const(32045); i64_sub;
                    i64_const(2440588); i64_sub;
                    i64_const(86400); i64_mul;
                    local_get(hour); i64_const(3600); i64_mul; i64_add;
                    local_get(minute); i64_const(60); i64_mul; i64_add;
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
                    // floor(ts / 86400)
                    local_get(ts); i64_const(0); i64_ge_s;
                    if_i64;
                      local_get(ts); i64_const(86400); i64_div_s;
                    else_;
                      local_get(ts); i64_const(86399); i64_sub; i64_const(86400); i64_div_s;
                    end;
                    local_set(d);
                    local_get(d); i64_const(2440588); i64_add; local_set(d);
                    local_get(d); i64_const(1401); i64_add;
                    i64_const(4); local_get(d); i64_mul; i64_const(274277); i64_add;
                    i64_const(146097); i64_div_s; i64_const(3); i64_mul; i64_const(4); i64_div_s;
                    i64_add; i64_const(38); i64_sub;
                    local_set(f);
                    i64_const(4); local_get(f); i64_mul; i64_const(3); i64_add; local_set(e);
                    local_get(e); i64_const(1461); i64_rem_s; i64_const(4); i64_div_s; local_set(g);
                    i64_const(5); local_get(g); i64_mul; i64_const(2); i64_add; local_set(h);
                });

                match func {
                    "day" => {
                        wasm!(self.func, {
                            local_get(h); i64_const(153); i64_rem_s; i64_const(5); i64_div_s; i64_const(1); i64_add;
                        });
                    }
                    "month" => {
                        wasm!(self.func, {
                            local_get(h); i64_const(153); i64_div_s; i64_const(2); i64_add;
                            i64_const(12); i64_rem_s; i64_const(1); i64_add;
                        });
                    }
                    "year" => {
                        let mm = self.scratch.alloc_i64();
                        wasm!(self.func, {
                            local_get(h); i64_const(153); i64_div_s; i64_const(2); i64_add;
                            i64_const(12); i64_rem_s; i64_const(1); i64_add;
                            local_set(mm);
                            local_get(e); i64_const(1461); i64_div_s; i64_const(4716); i64_sub;
                            i64_const(14); local_get(mm); i64_sub; i64_const(12); i64_div_s;
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
                    i64_const(86400); i64_rem_s;
                    i64_const(86400); i64_add; i64_const(86400); i64_rem_s;
                    i64_const(3600); i64_div_s;
                });
            }
            "minute" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i64_const(3600); i64_rem_s;
                    i64_const(3600); i64_add; i64_const(3600); i64_rem_s;
                    i64_const(60); i64_div_s;
                });
            }
            "second" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i64_const(60); i64_rem_s;
                    i64_const(60); i64_add; i64_const(60); i64_rem_s;
                });
            }
            "now" => {
                // Call WASI clock_time_get(id=0 realtime, precision=0, time_ptr)
                // Returns nanoseconds as i64 at time_ptr, convert to seconds
                let time_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    // Allocate 8 bytes for i64 result (allocator guarantees 8-byte alignment)
                    i32_const(8); call(self.emitter.rt.alloc); local_set(time_ptr);
                    // clock_time_get(id=0, precision=0, time_ptr)
                    i32_const(0); // clock_id: realtime
                    i64_const(0); // precision
                    local_get(time_ptr); // output pointer (8-byte aligned)
                    call(self.emitter.rt.clock_time_get);
                    drop; // discard error code
                    // Load i64 nanoseconds, convert to seconds
                    local_get(time_ptr); i64_load(0);
                    i64_const(1000000000); i64_div_u;
                });
                self.scratch.free_i32(time_ptr);
            }
            "add_days" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_const(86400); i64_mul; i64_add; });
            }
            "add_hours" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_const(3600); i64_mul; i64_add; });
            }
            "add_minutes" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_const(60); i64_mul; i64_add; });
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
                wasm!(self.func, { i64_sub; i64_const(86400); i64_div_s; });
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
                    i32_const(24); call(self.emitter.rt.alloc); local_set(ptr);
                    local_get(ptr); i32_const(20); i32_store(0);
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
                      local_get(ts); i64_const(86400); i64_div_s;
                    else_;
                      local_get(ts); i64_const(86399); i64_sub; i64_const(86400); i64_div_s;
                    end;
                    local_set(d);
                    local_get(d); i64_const(2440588); i64_add; local_set(d);
                    local_get(d); i64_const(1401); i64_add;
                    i64_const(4); local_get(d); i64_mul; i64_const(274277); i64_add;
                    i64_const(146097); i64_div_s; i64_const(3); i64_mul; i64_const(4); i64_div_s;
                    i64_add; i64_const(38); i64_sub; local_set(f);
                    i64_const(4); local_get(f); i64_mul; i64_const(3); i64_add; local_set(e);
                    local_get(e); i64_const(1461); i64_rem_s; i64_const(4); i64_div_s; local_set(g);
                    i64_const(5); local_get(g); i64_mul; i64_const(2); i64_add; local_set(h);
                    local_get(h); i64_const(153); i64_rem_s; i64_const(5); i64_div_s; i64_const(1); i64_add; local_set(dy);
                    local_get(h); i64_const(153); i64_div_s; i64_const(2); i64_add;
                    i64_const(12); i64_rem_s; i64_const(1); i64_add; local_set(mo);
                    local_get(e); i64_const(1461); i64_div_s; i64_const(4716); i64_sub;
                    i64_const(14); local_get(mo); i64_sub; i64_const(12); i64_div_s;
                    i64_add; local_set(yr);
                    local_get(ts); i64_const(86400); i64_rem_s; i64_const(86400); i64_add; i64_const(86400); i64_rem_s;
                    local_set(d);
                    local_get(d); i64_const(3600); i64_div_s; local_set(hr);
                    local_get(d); i64_const(3600); i64_rem_s; i64_const(60); i64_div_s; local_set(mi);
                    local_get(d); i64_const(60); i64_rem_s; local_set(se);
                });

                self.emit_write_decimal_digits(ptr, 4, yr, 4);
                wasm!(self.func, { local_get(ptr); i32_const(45); i32_store8(8); });
                self.emit_write_decimal_digits(ptr, 9, mo, 2);
                wasm!(self.func, { local_get(ptr); i32_const(45); i32_store8(11); });
                self.emit_write_decimal_digits(ptr, 12, dy, 2);
                wasm!(self.func, { local_get(ptr); i32_const(84); i32_store8(14); });
                self.emit_write_decimal_digits(ptr, 15, hr, 2);
                wasm!(self.func, { local_get(ptr); i32_const(58); i32_store8(17); });
                self.emit_write_decimal_digits(ptr, 18, mi, 2);
                wasm!(self.func, { local_get(ptr); i32_const(58); i32_store8(20); });
                self.emit_write_decimal_digits(ptr, 21, se, 2);
                wasm!(self.func, { local_get(ptr); i32_const(90); i32_store8(23); });

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
                      local_get(ts); i64_const(86400); i64_div_s;
                    else_;
                      local_get(ts); i64_const(86399); i64_sub; i64_const(86400); i64_div_s;
                    end;
                    i64_const(4); i64_add;
                    i64_const(7); i64_rem_s;
                    i64_const(7); i64_add; i64_const(7); i64_rem_s;
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
                        local_get(wd); i64_const(2); i64_eq;
                        if_i32; i32_const(tue as i32);
                        else_;
                          local_get(wd); i64_const(3); i64_eq;
                          if_i32; i32_const(wed as i32);
                          else_;
                            local_get(wd); i64_const(4); i64_eq;
                            if_i32; i32_const(thu as i32);
                            else_;
                              local_get(wd); i64_const(5); i64_eq;
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
                    local_get(s); i32_load(0); i32_const(19); i32_lt_u;
                    if_i32;
                      i32_const(8); call(self.emitter.rt.alloc); local_set(result);
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
                    i64_const(14); local_get(mo); i64_sub; i64_const(12); i64_div_s; local_set(a);
                    local_get(yr); i64_const(4800); i64_add; local_get(a); i64_sub; local_set(y);
                    local_get(mo); i64_const(12); local_get(a); i64_mul; i64_add; i64_const(3); i64_sub; local_set(m);
                    local_get(dy);
                    i64_const(153); local_get(m); i64_mul; i64_const(2); i64_add; i64_const(5); i64_div_s; i64_add;
                    i64_const(365); local_get(y); i64_mul; i64_add;
                    local_get(y); i64_const(4); i64_div_s; i64_add;
                    local_get(y); i64_const(100); i64_div_s; i64_sub;
                    local_get(y); i64_const(400); i64_div_s; i64_add;
                    i64_const(32045); i64_sub;
                    i64_const(2440588); i64_sub;
                    i64_const(86400); i64_mul;
                    local_get(hr); i64_const(3600); i64_mul; i64_add;
                    local_get(mi); i64_const(60); i64_mul; i64_add;
                    local_get(se); i64_add;
                    local_set(yr); // reuse as timestamp
                    // Build ok Result: [tag=0:i32][timestamp:i64] = 12 bytes
                    i32_const(12); call(self.emitter.rt.alloc); local_set(result);
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
            _ => {
                self.emit_stub_call_named("datetime", func, args);
            }
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
                local_get(tmp); i64_const(10); i64_rem_s;
                i64_const(48); i64_add;
                i32_wrap_i64;
                i32_store8(off);
                local_get(tmp); i64_const(10); i64_div_s; local_set(tmp);
            });
        }
        self.scratch.free_i64(tmp);
    }

    /// Parse N decimal ASCII digits from a string buffer into an i64 local.
    pub(super) fn emit_parse_digits(&mut self, str_local: u32, char_offset: u32, num_digits: u32, dest: u32) {
        wasm!(self.func, { i64_const(0); local_set(dest); });
        for i in 0..num_digits {
            let off = 4 + char_offset + i;
            wasm!(self.func, {
                local_get(dest); i64_const(10); i64_mul;
                local_get(str_local); i32_load8_u(off);
                i64_extend_i32_u; i64_const(48); i64_sub;
                i64_add;
                local_set(dest);
            });
        }
    }

}
